use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::env;
use std::io::{Read, Write};
#[cfg(target_os = "windows")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const CREDENTIAL_SETTLE_DELAY: Duration = Duration::from_secs(2);

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
pub(crate) enum WindowsConsoleEnvironment {
    Default,
    ClaudeSanitized,
}

#[cfg(target_os = "windows")]
struct WinHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(target_os = "windows")]
impl Drop for WinHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn windows_environment_block(cwd: &Path, policy: WindowsConsoleEnvironment) -> Vec<u16> {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::OsStrExt;

    let mut vars: Vec<(OsString, OsString)> = env::vars_os()
        .filter(|(key, _)| {
            let upper = key.to_string_lossy().to_ascii_uppercase();
            if upper == "PWD" {
                return false;
            }
            if matches!(policy, WindowsConsoleEnvironment::ClaudeSanitized)
                && (upper == "CLAUDE_CODE_OAUTH_TOKEN" || upper.starts_with("ANTHROPIC_"))
            {
                return false;
            }
            if matches!(policy, WindowsConsoleEnvironment::ClaudeSanitized)
                && upper == "DISABLE_AUTOUPDATER"
            {
                return false;
            }
            true
        })
        .collect();
    vars.push((OsString::from("PWD"), cwd.as_os_str().to_os_string()));
    if matches!(policy, WindowsConsoleEnvironment::ClaudeSanitized) {
        vars.push((OsString::from("DISABLE_AUTOUPDATER"), OsString::from("1")));
    }
    vars.sort_by(|left, right| {
        left.0
            .to_string_lossy()
            .to_ascii_uppercase()
            .cmp(&right.0.to_string_lossy().to_ascii_uppercase())
    });

    let mut block = Vec::new();
    for (key, value) in vars {
        block.extend(OsStr::new(&key).encode_wide());
        block.push(b'=' as u16);
        block.extend(OsStr::new(&value).encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

#[cfg(target_os = "windows")]
pub(crate) fn run_hidden_console_until_credential_change<F>(
    provider: &'static str,
    provider_command: &str,
    cwd: &Path,
    timeout: Duration,
    environment: WindowsConsoleEnvironment,
    mut read_signature: F,
) -> Result<(), String>
where
    F: FnMut() -> Option<String>,
{
    use std::ffi::OsString;
    use std::io;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, ResumeThread, TerminateProcess, WaitForSingleObject, CREATE_NEW_CONSOLE,
        CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTF_USESHOWWINDOW,
        STARTUPINFOW,
    };

    let before = read_signature();
    let comspec = env::var_os("ComSpec").unwrap_or_else(|| {
        let root = env::var_os("SystemRoot").unwrap_or_else(|| OsString::from(r"C:\Windows"));
        PathBuf::from(root)
            .join("System32/cmd.exe")
            .into_os_string()
    });
    let application = windows_wide_null(&comspec);
    let cwd_wide = windows_wide_null(cwd.as_os_str());
    let mut command_line = windows_wide_null(
        OsString::from(format!(
            "\"{}\" /d /s /c \"{}\"",
            PathBuf::from(&comspec).display(),
            provider_command
        ))
        .as_os_str(),
    );
    let environment_block = windows_environment_block(cwd, environment);

    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(format!(
            "{provider} CLI real-console job creation failed: {}",
            io::Error::last_os_error()
        ));
    }
    let job = WinHandle(job);

    let mut startup = STARTUPINFOW::default();
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.dwFlags = STARTF_USESHOWWINDOW;
    startup.wShowWindow = 0; // SW_HIDE
    let mut process_info = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NEW_CONSOLE | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            environment_block.as_ptr().cast(),
            cwd_wide.as_ptr(),
            &startup,
            &mut process_info,
        )
    };
    if created == 0 {
        return Err(format!(
            "{provider} CLI real-console launch failed: {}",
            io::Error::last_os_error()
        ));
    }
    let process = WinHandle(process_info.hProcess);
    let thread_handle = WinHandle(process_info.hThread);

    if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
        let error = io::Error::last_os_error();
        unsafe {
            TerminateProcess(process.0, 127);
        }
        return Err(format!(
            "{provider} CLI real-console job assignment failed: {error}"
        ));
    }
    if unsafe { ResumeThread(thread_handle.0) } == u32::MAX {
        let error = io::Error::last_os_error();
        unsafe {
            TerminateJobObject(job.0, 127);
        }
        return Err(format!(
            "{provider} CLI real-console resume failed: {error}"
        ));
    }
    drop(thread_handle);

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

        match unsafe { WaitForSingleObject(process.0, 0) } {
            WAIT_OBJECT_0 => {
                let after_exit = read_signature();
                if credential_changed(before.as_deref(), after_exit.as_deref()) {
                    break Ok(());
                }
                break Err(format!(
                    "{provider} CLI exited before refreshing its access credential"
                ));
            }
            WAIT_TIMEOUT => {}
            status => {
                break Err(format!(
                    "{provider} CLI real-console process status failed: wait status {status}"
                ));
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

    let cleanup_error = if unsafe { TerminateJobObject(job.0, 127) } == 0 {
        Some(io::Error::last_os_error())
    } else {
        None
    };
    unsafe {
        WaitForSingleObject(process.0, 2_000);
    }
    if let Some(error) = cleanup_error {
        return Err(format!(
            "{provider} CLI real-console cleanup failed: {error}"
        ));
    }
    result
}

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

pub(crate) fn spawn_refresh_once<F>(running: &'static AtomicBool, job: F) -> bool
where
    F: FnOnce() + Send + 'static,
{
    if running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    thread::spawn(move || {
        struct ResetRunning(&'static AtomicBool);
        impl Drop for ResetRunning {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _reset = ResetRunning(running);
        job();
    });
    true
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PtyInputTouch {
    pub bytes: &'static [u8],
    pub after: Duration,
}

pub(crate) fn run_until_credential_change<F>(
    provider: &'static str,
    command: CommandBuilder,
    timeout: Duration,
    read_signature: F,
) -> Result<(), String>
where
    F: FnMut() -> Option<String>,
{
    run_until_credential_change_with_input(provider, command, timeout, None, read_signature)
}

pub(crate) fn run_until_credential_change_with_input<F>(
    provider: &'static str,
    command: CommandBuilder,
    timeout: Duration,
    input_touch: Option<PtyInputTouch>,
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
    let mut writer = if input_touch.is_some() {
        Some(
            pair.master
                .take_writer()
                .map_err(|error| format!("{provider} CLI PTY writer unavailable: {error}"))?,
        )
    } else {
        None
    };
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
    let mut input_sent = false;
    let result = loop {
        if !input_sent {
            if let Some(touch) = input_touch {
                if started.elapsed() >= touch.after {
                    let Some(writer) = writer.as_mut() else {
                        break Err(format!("{provider} CLI PTY writer unavailable"));
                    };
                    if let Err(error) = writer.write_all(touch.bytes).and_then(|_| writer.flush()) {
                        break Err(format!("{provider} CLI PTY input failed: {error}"));
                    }
                    input_sent = true;
                }
            }
        }

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
    drop(writer);
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
    use super::{
        credential_changed, find_executable_in_paths, run_until_credential_change,
        run_until_credential_change_with_input, spawn_refresh_once, PtyInputTouch,
    };
    use portable_pty::CommandBuilder;
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::thread;
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
    fn background_refresh_runs_once_and_releases_gate() {
        let running: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        assert!(spawn_refresh_once(running, move || {
            started_tx.send(()).expect("signal background start");
            release_rx.recv().expect("release background refresh");
        }));
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("background refresh started");
        assert!(!spawn_refresh_once(running, || {}));
        release_tx.send(()).expect("release first refresh");
        for _ in 0..100 {
            if !running.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!running.load(Ordering::Acquire));
        assert!(spawn_refresh_once(running, || {}));
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

    #[test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "GitHub Actions Windows runners do not provide a reliable interactive ConPTY session"
    )]
    fn pty_input_touch_can_trigger_provider_owned_credential_change() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "token-lens-auth-input-touch-{}-{nonce}.txt",
            std::process::id()
        ));
        fs::write(&path, "before").expect("write auth-touch fixture");
        let command = credential_writer_after_input_command(&path);
        let result = run_until_credential_change_with_input(
            "Fixture",
            command,
            Duration::from_secs(5),
            Some(PtyInputTouch {
                bytes: b"/status\r",
                after: Duration::from_millis(100),
            }),
            || fs::read_to_string(&path).ok(),
        );
        let _ = fs::remove_file(&path);
        assert!(result.is_ok(), "{result:?}");
    }

    #[cfg(not(target_os = "windows"))]
    fn unchanged_exit_command() -> CommandBuilder {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 0.1"]);
        command
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn hidden_real_console_returns_after_credential_change() {
        use super::{run_hidden_console_until_credential_change, WindowsConsoleEnvironment};
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "token-lens-real-console-auth-{}-{nonce}.txt",
            std::process::id()
        ));
        fs::write(&path, "before").expect("write real-console fixture");
        let escaped = path.to_string_lossy();
        let command = format!(">\"{escaped}\" <nul set /p =after & ping -n 10 127.0.0.1 >nul");
        let result = run_hidden_console_until_credential_change(
            "Fixture",
            &command,
            std::env::temp_dir().as_path(),
            Duration::from_secs(8),
            WindowsConsoleEnvironment::Default,
            || fs::read_to_string(&path).ok(),
        );
        let _ = fs::remove_file(&path);
        assert!(result.is_ok(), "{result:?}");
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

    #[cfg(not(target_os = "windows"))]
    fn credential_writer_after_input_command(path: &std::path::Path) -> CommandBuilder {
        let escaped = path.to_string_lossy().replace('\'', "'\\''");
        let script = format!("IFS= read -r _line; printf after > '{escaped}'; sleep 10");
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", &script]);
        command
    }

    #[cfg(target_os = "windows")]
    fn credential_writer_after_input_command(path: &std::path::Path) -> CommandBuilder {
        let escaped = path.to_string_lossy();
        let script =
            format!("set /p line= & >\"{escaped}\" <nul set /p =after & ping -n 10 127.0.0.1 >nul");
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", &script]);
        command
    }

    #[cfg(target_os = "windows")]
    fn credential_writer_command(path: &std::path::Path) -> CommandBuilder {
        let escaped = path.to_string_lossy();
        let script = format!(">\"{escaped}\" <nul set /p =after & ping -n 10 127.0.0.1 >nul");
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", &script]);
        command
    }
}
