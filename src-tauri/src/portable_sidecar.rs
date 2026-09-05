use flate2::read::GzDecoder;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const PORTABLE_FOOTER_MAGIC: &[u8; 8] = b"TLTS0001";
const PORTABLE_FOOTER_LEN: u64 = 24;

#[derive(Debug, Clone)]
pub(crate) struct PortableSidecar {
    binary: PathBuf,
    _guard: Arc<PortableSidecarGuard>,
}

#[derive(Debug)]
struct PortableSidecarGuard {
    directory: PathBuf,
    lock: Option<File>,
}

impl Drop for PortableSidecarGuard {
    fn drop(&mut self) {
        self.lock.take();
        let _ = fs::remove_dir_all(&self.directory);
    }
}
impl PortableSidecar {
    pub(crate) fn prepare(binary_name: &str) -> Result<Option<Self>, String> {
        let executable = env::current_exe()
            .map_err(|error| format!("failed to resolve Token Lens executable: {error}"))?;
        Self::prepare_from(&executable, binary_name)
    }

    fn prepare_from(executable: &Path, binary_name: &str) -> Result<Option<Self>, String> {
        let Some(payload) = embedded_payload_metadata(executable)? else {
            return Ok(None);
        };
        let root = env::temp_dir().join("TokenLensPortable");
        cleanup_stale_portable_dirs(&root);
        let directory = create_portable_directory(&root)?;
        let lock = match create_active_lock(&directory.join(".lock")) {
            Ok(lock) => lock,
            Err(error) => {
                let _ = fs::remove_dir_all(&directory);
                return Err(error);
            }
        };
        let binary = directory.join(binary_name);
        if let Err(error) = extract_embedded_payload(executable, payload, &binary) {
            drop(lock);
            let _ = fs::remove_dir_all(&directory);
            return Err(error);
        }
        Ok(Some(Self {
            binary,
            _guard: Arc::new(PortableSidecarGuard {
                directory,
                lock: Some(lock),
            }),
        }))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.binary
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmbeddedPayload {
    offset: u64,
    compressed_len: u64,
    raw_len: u64,
}

fn embedded_payload_metadata(executable: &Path) -> Result<Option<EmbeddedPayload>, String> {
    let mut file = File::open(executable)
        .map_err(|error| format!("failed to open Token Lens executable: {error}"))?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("failed to inspect Token Lens executable: {error}"))?
        .len();
    if file_len < PORTABLE_FOOTER_LEN {
        return Ok(None);
    }

    file.seek(SeekFrom::End(-(PORTABLE_FOOTER_LEN as i64)))
        .map_err(|error| format!("failed to seek Token Lens portable footer: {error}"))?;
    let mut footer = [0u8; PORTABLE_FOOTER_LEN as usize];
    file.read_exact(&mut footer)
        .map_err(|error| format!("failed to read Token Lens portable footer: {error}"))?;
    if &footer[..8] != PORTABLE_FOOTER_MAGIC {
        return Ok(None);
    }

    let compressed_len = u64::from_le_bytes(footer[8..16].try_into().unwrap());
    let raw_len = u64::from_le_bytes(footer[16..24].try_into().unwrap());
    if compressed_len == 0 || raw_len == 0 || compressed_len > file_len - PORTABLE_FOOTER_LEN {
        return Err("invalid embedded tokScale portable payload metadata".to_owned());
    }
    Ok(Some(EmbeddedPayload {
        offset: file_len - PORTABLE_FOOTER_LEN - compressed_len,
        compressed_len,
        raw_len,
    }))
}

fn extract_embedded_payload(
    executable: &Path,
    payload: EmbeddedPayload,
    output: &Path,
) -> Result<(), String> {
    let mut source = File::open(executable)
        .map_err(|error| format!("failed to open portable Token Lens executable: {error}"))?;
    source
        .seek(SeekFrom::Start(payload.offset))
        .map_err(|error| format!("failed to seek embedded tokScale payload: {error}"))?;
    let compressed = source.take(payload.compressed_len);
    let mut decoder = GzDecoder::new(compressed);
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| format!("failed to create temporary tokScale sidecar: {error}"))?;
    let written = std::io::copy(&mut decoder, &mut destination)
        .map_err(|error| format!("failed to extract embedded tokScale sidecar: {error}"))?;
    destination
        .flush()
        .map_err(|error| format!("failed to flush temporary tokScale sidecar: {error}"))?;
    if written != payload.raw_len {
        return Err(format!(
            "embedded tokScale size mismatch: expected {}, extracted {}",
            payload.raw_len, written
        ));
    }
    Ok(())
}

fn create_portable_directory(root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("failed to create Token Lens portable temp root: {error}"))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..32u8 {
        let candidate = root.join(format!(
            "run-{}-{timestamp:x}-{attempt:02x}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create Token Lens portable temp directory: {error}"
                ))
            }
        }
    }
    Err("failed to allocate a unique Token Lens portable temp directory".to_owned())
}

fn create_active_lock(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    options.share_mode(0);
    options
        .open(path)
        .map_err(|error| format!("failed to create Token Lens portable lock: {error}"))
}

fn cleanup_stale_portable_dirs(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || portable_dir_is_active(&path) {
            continue;
        }
        let _ = fs::remove_dir_all(path);
    }
}
fn portable_dir_is_active(directory: &Path) -> bool {
    let lock_path = directory.join(".lock");
    if !lock_path.is_file() {
        return false;
    }
    #[cfg(windows)]
    {
        let mut options = OpenOptions::new();
        options.read(true).write(true).share_mode(0);
        return options.open(lock_path).is_err();
    }
    #[cfg(not(windows))]
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    fn test_root(label: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "token-lens-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn embedded_portable_payload_round_trips_tokscale_bytes() {
        let root = test_root("portable-test");
        fs::create_dir_all(&root).expect("create test dir");
        let executable = root.join("Token-Lens.exe");
        let sidecar = b"fake-tokscale-payload".repeat(128);

        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&sidecar).expect("compress sidecar");
        let compressed = encoder.finish().expect("finish gzip");
        let mut image = b"MZ-fake-token-lens".to_vec();
        image.extend_from_slice(&compressed);
        image.extend_from_slice(PORTABLE_FOOTER_MAGIC);
        image.extend_from_slice(&(compressed.len() as u64).to_le_bytes());
        image.extend_from_slice(&(sidecar.len() as u64).to_le_bytes());
        fs::write(&executable, image).expect("write portable fixture");

        let prepared = PortableSidecar::prepare_from(&executable, "tokscale.exe")
            .expect("prepare portable sidecar")
            .expect("portable footer");
        assert_eq!(fs::read(prepared.path()).expect("read sidecar"), sidecar);
        let run_directory = prepared
            .path()
            .parent()
            .expect("run directory")
            .to_path_buf();
        drop(prepared);
        assert!(!run_directory.exists());
        fs::remove_dir_all(root).expect("clean test dir");
    }

    #[test]
    fn ordinary_executable_has_no_embedded_portable_payload() {
        let root = test_root("no-portable-test");
        fs::create_dir_all(&root).expect("create test dir");
        let executable = root.join("Token-Lens.exe");
        fs::write(&executable, b"MZ-ordinary-token-lens").expect("write executable fixture");
        assert!(PortableSidecar::prepare_from(&executable, "tokscale.exe")
            .expect("inspect")
            .is_none());
        fs::remove_dir_all(root).expect("clean test dir");
    }

    #[cfg(windows)]
    #[test]
    fn cleanup_keeps_active_runs_and_removes_unlocked_stale_runs() {
        let root = test_root("portable-cleanup-test");
        fs::create_dir_all(&root).expect("create cleanup root");
        let active = root.join("active");
        let stale = root.join("stale");
        fs::create_dir(&active).expect("create active dir");
        fs::create_dir(&stale).expect("create stale dir");
        let active_lock = create_active_lock(&active.join(".lock")).expect("active lock");
        let stale_lock = create_active_lock(&stale.join(".lock")).expect("stale lock");
        drop(stale_lock);

        cleanup_stale_portable_dirs(&root);
        assert!(active.is_dir());
        assert!(!stale.exists());

        drop(active_lock);
        cleanup_stale_portable_dirs(&root);
        assert!(!active.exists());
        fs::remove_dir_all(root).expect("clean cleanup root");
    }
}
