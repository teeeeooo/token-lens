use crate::provider_cli_auth;
#[cfg(not(target_os = "windows"))]
use portable_pty::CommandBuilder;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const AUTH_TOUCH_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(any(target_os = "windows", test))]
const AUTH_TOUCH_ARGUMENTS: &str = "--list-sessions -e none --skip-trust";
#[cfg(not(target_os = "windows"))]
const AUTH_TOUCH_ARGS: [&str; 4] = ["--list-sessions", "-e", "none", "--skip-trust"];

#[derive(Debug)]
pub(crate) struct RefreshAttempt {
    pub result: Result<(), String>,
    pub cli_source: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct DiscoveredBinary {
    path: PathBuf,
    source: &'static str,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    shell_name: bool,
}

pub(crate) fn classify_refresh_error(error: &str) -> (&'static str, &'static str) {
    let lower = error.to_ascii_lowercase();
    if lower.contains("not installed") || lower.contains("not discoverable") {
        return (
            "CLI_NOT_FOUND",
            "Gemini CLI is not installed or discoverable",
        );
    }
    if lower.contains("timed out") {
        return (
            "CLI_TIMEOUT",
            "Gemini CLI did not refresh its access credential before timeout",
        );
    }
    if lower.contains("exited before refreshing") {
        return (
            "CLI_CREDENTIAL_UNCHANGED",
            "Gemini CLI exited without refreshing its access credential",
        );
    }
    if lower.contains("pty") || lower.contains("spawn") || lower.contains("launch") {
        return (
            "CLI_LAUNCH_FAILED",
            "Gemini CLI auth refresh could not start",
        );
    }
    ("CLI_REFRESH_FAILED", "Gemini CLI auth refresh failed")
}

pub(crate) fn refresh_credential_via_startup<F>(home: &Path, read_signature: F) -> RefreshAttempt
where
    F: FnMut() -> Option<String>,
{
    let Some(binary) = discover_binary(home) else {
        return RefreshAttempt {
            result: Err("Gemini CLI is not installed or discoverable".to_owned()),
            cli_source: None,
        };
    };
    let probe_dir = auth_probe_directory();
    if let Err(error) = fs::create_dir_all(&probe_dir) {
        return RefreshAttempt {
            result: Err(format!(
                "Gemini CLI auth probe directory unavailable: {error}"
            )),
            cli_source: Some(binary.source),
        };
    }
    #[cfg(target_os = "windows")]
    let result = provider_cli_auth::run_hidden_console_until_credential_change(
        "Gemini",
        &windows_provider_command(&binary),
        &probe_dir,
        AUTH_TOUCH_TIMEOUT,
        provider_cli_auth::WindowsConsoleEnvironment::Default,
        read_signature,
    );

    #[cfg(not(target_os = "windows"))]
    let result = {
        let mut command = discovered_command_builder(&binary);
        command.cwd(&probe_dir);
        command.env("PWD", &probe_dir);
        provider_cli_auth::run_until_credential_change(
            "Gemini",
            command,
            AUTH_TOUCH_TIMEOUT,
            read_signature,
        )
    };

    RefreshAttempt {
        result,
        cli_source: Some(binary.source),
    }
}

#[cfg(target_os = "windows")]
fn windows_provider_command(binary: &DiscoveredBinary) -> String {
    let executable = if binary.shell_name {
        "gemini".to_owned()
    } else {
        format!("\"{}\"", binary.path.display())
    };
    format!("{executable} {AUTH_TOUCH_ARGUMENTS}")
}

#[cfg(not(target_os = "windows"))]
fn discovered_command_builder(binary: &DiscoveredBinary) -> CommandBuilder {
    #[cfg(target_os = "windows")]
    if binary.shell_name {
        let mut command = CommandBuilder::new("cmd.exe");
        command.args([
            "/d",
            "/s",
            "/c",
            "gemini --list-sessions -e none --skip-trust",
        ]);
        return command;
    }
    bare_command_builder(&binary.path)
}

#[cfg(not(target_os = "windows"))]
fn bare_command_builder(binary: &Path) -> CommandBuilder {
    #[cfg(target_os = "windows")]
    if binary
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("cmd") || value.eq_ignore_ascii_case("bat"))
    {
        let line = format!(
            "\"{}\" --list-sessions -e none --skip-trust",
            binary.display()
        );
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", &line]);
        return command;
    }
    let mut command = CommandBuilder::new(binary);
    command.args(AUTH_TOUCH_ARGS);
    command
}

fn auth_probe_directory() -> PathBuf {
    env::temp_dir().join(format!(
        "token-lens-gemini-auth-probe-{}",
        std::process::id()
    ))
}

fn discover_binary(_home: &Path) -> Option<DiscoveredBinary> {
    if let Some(path) = env::var_os("TOKEN_LENS_GEMINI_BIN") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(DiscoveredBinary {
                path,
                source: "override",
                shell_name: false,
            });
        }
    }

    #[cfg(target_os = "macos")]
    for (candidate, source) in [
        (_home.join(".local/bin/gemini"), "home_local"),
        (_home.join(".npm-global/bin/gemini"), "home_npm"),
        (PathBuf::from("/opt/homebrew/bin/gemini"), "homebrew"),
        (PathBuf::from("/usr/local/bin/gemini"), "usr_local"),
    ] {
        if candidate.is_file() {
            return Some(DiscoveredBinary {
                path: candidate,
                source,
                shell_name: false,
            });
        }
    }

    #[cfg(target_os = "windows")]
    {
        let path_candidate =
            provider_cli_auth::find_executable_on_path(&["gemini.exe", "gemini.cmd", "gemini.bat"]);
        let appdata_candidate = env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|appdata| appdata.join("npm/gemini.cmd"))
            .filter(|candidate| candidate.is_file());
        choose_windows_candidate(path_candidate, appdata_candidate)
    }

    #[cfg(not(target_os = "windows"))]
    provider_cli_auth::find_executable_on_path(&["gemini"]).map(|path| DiscoveredBinary {
        path,
        source: "path",
        shell_name: false,
    })
}

#[cfg(any(target_os = "windows", test))]
fn choose_windows_candidate(
    path_candidate: Option<PathBuf>,
    appdata_candidate: Option<PathBuf>,
) -> Option<DiscoveredBinary> {
    path_candidate
        .map(|path| DiscoveredBinary {
            path,
            source: "path",
            shell_name: true,
        })
        .or_else(|| {
            appdata_candidate.map(|path| DiscoveredBinary {
                path,
                source: "appdata_npm",
                shell_name: false,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::{
        auth_probe_directory, choose_windows_candidate, classify_refresh_error,
        AUTH_TOUCH_ARGUMENTS,
    };
    use std::path::PathBuf;

    #[test]
    fn refresh_errors_are_sanitized_and_classified() {
        assert_eq!(
            classify_refresh_error("Gemini CLI auth refresh timed out after 60s").0,
            "CLI_TIMEOUT"
        );
        assert_eq!(
            classify_refresh_error("Gemini CLI exited before refreshing its access credential").0,
            "CLI_CREDENTIAL_UNCHANGED"
        );
        assert_eq!(
            classify_refresh_error("Gemini CLI launch failed").0,
            "CLI_LAUNCH_FAILED"
        );
    }
    #[test]
    fn auth_touch_uses_zero_inference_session_listing_in_isolated_temp_scope() {
        assert_eq!(AUTH_TOUCH_ARGUMENTS, "--list-sessions -e none --skip-trust");
        let probe = auth_probe_directory();
        assert!(probe.starts_with(std::env::temp_dir()));
        assert!(probe
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with("token-lens-gemini-auth-probe-")));
    }

    #[test]
    fn windows_candidate_prefers_shell_path_over_appdata_fallback() {
        let selected = choose_windows_candidate(
            Some(PathBuf::from(r"C:\tools\gemini.cmd")),
            Some(PathBuf::from(
                r"C:\Users\user\AppData\Roaming\npm\gemini.cmd",
            )),
        )
        .expect("Gemini candidate");
        assert_eq!(selected.source, "path");
        assert!(selected.shell_name);
    }

    #[test]
    fn windows_candidate_uses_appdata_only_when_shell_path_is_absent() {
        let selected = choose_windows_candidate(
            None,
            Some(PathBuf::from(
                r"C:\Users\user\AppData\Roaming\npm\gemini.cmd",
            )),
        )
        .expect("Gemini candidate");
        assert_eq!(selected.source, "appdata_npm");
        assert!(!selected.shell_name);
    }
}
