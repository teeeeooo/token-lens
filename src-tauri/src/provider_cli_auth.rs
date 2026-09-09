use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const CREDENTIAL_SETTLE_DELAY: Duration = Duration::from_secs(2);

pub(crate) fn find_executable_on_path(names: &[&str]) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    find_executable_in_paths(env::split_paths(&path), names)
}

fn find_executable_in_paths<I>(paths: I, names: &[&str]) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    paths.into_iter().find_map(|directory| {
        names.iter().find_map(|name| {
            let candidate = directory.join(name);
            candidate.is_file().then_some(candidate)
        })
    })
}

pub(crate) fn run_until_credential_change<F>(
    provider: &'static str,
    command: CommandBuilder,
    timeout: Duration,
    mut read_signature: F,
) -> Result<(), String>
where
    F: FnMut() -> Option<String>,
{
    let before = read_signature();
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: 50,
            cols: 160,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("{provider} CLI PTY unavailable: {error}"))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("{provider} CLI PTY reader unavailable: {error}"))?;
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("{provider} CLI launch failed: {error}"))?;
    drop(pair.slave);
    // Drain the pseudo-terminal only to prevent provider TUI backpressure. Token Lens
    // never parses, stores, or logs these bytes; credential state is the only signal.
    let reader_thread = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });

    let started = Instant::now();
    let result = loop {
        let after = read_signature();
        if credential_changed(before.as_deref(), after.as_deref()) {
            let remaining = timeout.saturating_sub(started.elapsed());
            thread::sleep(CREDENTIAL_SETTLE_DELAY.min(remaining));
            let settled = read_signature();
            if credential_changed(before.as_deref(), settled.as_deref()) {
                break Ok(());
            }
        }

        match child.try_wait() {
            Ok(Some(_)) => {
                let after_exit = read_signature();
                if credential_changed(before.as_deref(), after_exit.as_deref()) {
                    break Ok(());
                }
                break Err(format!(
                    "{provider} CLI exited before refreshing its access credential"
                ));
            }
            Ok(None) => {}
            Err(error) => {
                break Err(format!("{provider} CLI process status failed: {error}"));
            }
        }
        let elapsed = started.elapsed();
        if elapsed >= timeout {
            break Err(format!(
                "{provider} CLI auth refresh timed out after {}s",
                timeout.as_secs()
            ));
        }
        thread::sleep(POLL_INTERVAL.min(timeout.saturating_sub(elapsed)));
    };

    let _ = child.kill();
    let _ = child.wait();
    drop(pair.master);
    let _ = reader_thread.join();
    result
}

fn credential_changed(before: Option<&str>, after: Option<&str>) -> bool {
    match (before, after) {
        (None, Some(_)) => true,
        (Some(before), Some(after)) => before != after,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{credential_changed, find_executable_in_paths, run_until_credential_change};
    use portable_pty::CommandBuilder;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn path_discovery_returns_only_existing_candidates() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "token-lens-path-discovery-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create path fixture");
        let binary = directory.join("provider-fixture.cmd");
        fs::write(&binary, "fixture").expect("write path fixture");
        let found = find_executable_in_paths(
            vec![directory.clone()],
            &["missing.exe", "provider-fixture.cmd"],
        );
        let _ = fs::remove_dir_all(&directory);
        assert_eq!(found.as_deref(), Some(binary.as_path()));
    }

    #[test]
    fn detects_new_or_rotated_credentials_only() {
        assert!(credential_changed(None, Some("token-a")));
        assert!(credential_changed(Some("token-a"), Some("token-b")));
        assert!(!credential_changed(None, None));
        assert!(!credential_changed(Some("token-a"), None));
        assert!(!credential_changed(Some("token-a"), Some("token-a")));
    }

    #[test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "GitHub Actions Windows runners do not provide a reliable interactive ConPTY session"
    )]
    fn pty_auth_touch_reports_clean_exit_without_credential_change() {
        let command = unchanged_exit_command();
        let result =
            run_until_credential_change("Fixture", command, Duration::from_secs(2), || {
                Some("unchanged".to_owned())
            });
        assert!(
            result
                .as_ref()
                .is_err_and(|error| error.contains("exited before refreshing")),
            "{result:?}"
        );
    }

    #[test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "GitHub Actions Windows runners do not provide a reliable interactive ConPTY session"
    )]
    fn pty_auth_touch_returns_after_provider_owned_credential_change() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "token-lens-auth-touch-{}-{nonce}.txt",
            std::process::id()
        ));
        fs::write(&path, "before").expect("write auth-touch fixture");
        let command = credential_writer_command(&path);
        let result =
            run_until_credential_change("Fixture", command, Duration::from_secs(5), || {
                fs::read_to_string(&path).ok()
            });
        let _ = fs::remove_file(&path);
        assert!(result.is_ok(), "{result:?}");
    }

    #[cfg(not(target_os = "windows"))]
    fn unchanged_exit_command() -> CommandBuilder {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 0.1"]);
        command
    }

    #[cfg(target_os = "windows")]
    fn unchanged_exit_command() -> CommandBuilder {
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", "exit /b 0"]);
        command
    }

    #[cfg(not(target_os = "windows"))]
    fn credential_writer_command(path: &std::path::Path) -> CommandBuilder {
        let escaped = path.to_string_lossy().replace('\'', "'\\''");
        let script = format!("sleep 0.2; printf after > '{escaped}'; sleep 10");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", &script]);
        command
    }

    #[cfg(target_os = "windows")]
    fn credential_writer_command(path: &std::path::Path) -> CommandBuilder {
        let escaped = path.to_string_lossy().replace('"', "\"");
        let script = format!(">\"{escaped}\" <nul set /p =after & ping -n 10 127.0.0.1 >nul");
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", &script]);
        command
    }
}
