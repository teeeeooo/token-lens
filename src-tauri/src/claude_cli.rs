use crate::provider_cli_auth;
#[cfg(not(target_os = "windows"))]
use portable_pty::CommandBuilder;
use std::env;
#[cfg(target_os = "windows")]
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const AUTH_TOUCH_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(not(target_os = "windows"))]
const STATUS_TOUCH_DELAY: Duration = Duration::from_secs(5);
#[cfg(not(target_os = "windows"))]
const STATUS_TOUCH_INPUT: &[u8] = b"/status\r";

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
            "Claude CLI is not installed or discoverable",
        );
    }
    if lower.contains("timed out") {
        return (
            "CLI_TIMEOUT",
            "Claude CLI did not refresh its access credential before timeout",
        );
    }
    if lower.contains("exited before refreshing") {
        return (
            "CLI_CREDENTIAL_UNCHANGED",
            "Claude CLI exited without refreshing its access credential",
        );
    }
    if lower.contains("pty") || lower.contains("spawn") || lower.contains("launch") {
        return (
            "CLI_LAUNCH_FAILED",
            "Claude CLI auth refresh could not start",
        );
    }
    ("CLI_REFRESH_FAILED", "Claude CLI auth refresh failed")
}

pub(crate) fn refresh_credential_via_startup<F>(home: &Path, read_signature: F) -> RefreshAttempt
where
    F: FnMut() -> Option<String>,
{
    let Some(binary) = discover_binary(home) else {
        return RefreshAttempt {
            result: Err("Claude CLI is not installed or discoverable".to_owned()),
            cli_source: None,
        };
    };
    #[cfg(target_os = "windows")]
    let result = provider_cli_auth::run_hidden_console_until_credential_change(
        "Claude",
        &windows_provider_command(&binary),
        home,
        AUTH_TOUCH_TIMEOUT,
        provider_cli_auth::WindowsConsoleEnvironment::ClaudeSanitized,
        read_signature,
    );

    #[cfg(not(target_os = "windows"))]
    let result = {
        let mut command = discovered_command_builder(&binary);
        command.cwd(home);
        command.env("PWD", home);
        command.env("DISABLE_AUTOUPDATER", "1");
        command.env_remove("CLAUDE_CODE_OAUTH_TOKEN");
        for key in env::vars_os().map(|(key, _)| key) {
            if key.to_string_lossy().starts_with("ANTHROPIC_") {
                command.env_remove(key);
            }
        }
        provider_cli_auth::run_until_credential_change_with_input(
            "Claude",
            command,
            AUTH_TOUCH_TIMEOUT,
            Some(provider_cli_auth::PtyInputTouch {
                bytes: STATUS_TOUCH_INPUT,
                after: STATUS_TOUCH_DELAY,
            }),
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
    if binary.shell_name {
        "claude".to_owned()
    } else {
        format!("\"{}\"", binary.path.display())
    }
}

#[cfg(not(target_os = "windows"))]
fn discovered_command_builder(binary: &DiscoveredBinary) -> CommandBuilder {
    #[cfg(target_os = "windows")]
    if binary.shell_name {
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", "claude"]);
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
        let line = format!("\"{}\"", binary.display());
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", &line]);
        return command;
    }
    CommandBuilder::new(binary)
}

fn discover_binary(home: &Path) -> Option<DiscoveredBinary> {
    if let Some(path) = env::var_os("TOKEN_LENS_CLAUDE_BIN") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(DiscoveredBinary {
                path,
                source: "override",
                shell_name: false,
            });
        }
    }

    #[cfg(target_os = "windows")]
    {
        let path_candidate =
            provider_cli_auth::find_executable_on_path(&["claude.exe", "claude.cmd", "claude.bat"]);
        let local_candidate = home
            .join(".local/bin/claude.exe")
            .is_file()
            .then(|| home.join(".local/bin/claude.exe"));
        let appdata_candidate = env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("npm/claude.cmd"))
            .filter(|path| path.is_file());
        choose_windows_candidate(
            path_candidate,
            local_candidate,
            appdata_candidate,
            discover_windows_winget_binary(),
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        for (candidate, source) in [
            (home.join(".local/bin/claude"), "home_local"),
            (PathBuf::from("/opt/homebrew/bin/claude"), "homebrew"),
            (PathBuf::from("/usr/local/bin/claude"), "usr_local"),
        ] {
            if candidate.is_file() {
                return Some(DiscoveredBinary {
                    path: candidate,
                    source,
                    shell_name: false,
                });
            }
        }
        provider_cli_auth::find_executable_on_path(&["claude"]).map(|path| DiscoveredBinary {
            path,
            source: "path",
            shell_name: false,
        })
    }
}

#[cfg(any(target_os = "windows", test))]
fn choose_windows_candidate(
    path_candidate: Option<PathBuf>,
    local_candidate: Option<PathBuf>,
    appdata_candidate: Option<PathBuf>,
    winget_candidate: Option<PathBuf>,
) -> Option<DiscoveredBinary> {
    for (candidate, source, shell_name) in [
        (path_candidate, "path", true),
        (local_candidate, "home_local", false),
        (appdata_candidate, "appdata_npm", false),
        (winget_candidate, "winget", false),
    ] {
        if let Some(path) = candidate {
            return Some(DiscoveredBinary {
                path,
                source,
                shell_name,
            });
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn discover_windows_winget_binary() -> Option<PathBuf> {
    let root = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)?
        .join("Microsoft/WinGet/Packages");
    let entries = fs::read_dir(root).ok()?;
    entries.flatten().find_map(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("Anthropic.ClaudeCode_") {
            return None;
        }
        let binary = entry.path().join("claude.exe");
        binary.is_file().then_some(binary)
    })
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "windows"))]
    use super::STATUS_TOUCH_INPUT;
    use super::{choose_windows_candidate, classify_refresh_error};
    use std::path::PathBuf;

    #[test]
    fn refresh_errors_are_sanitized_and_classified() {
        assert_eq!(
            classify_refresh_error("Claude CLI auth refresh timed out after 60s").0,
            "CLI_TIMEOUT"
        );
        assert_eq!(
            classify_refresh_error("Claude CLI exited before refreshing its access credential").0,
            "CLI_CREDENTIAL_UNCHANGED"
        );
        assert_eq!(
            classify_refresh_error("Claude CLI launch failed").0,
            "CLI_LAUNCH_FAILED"
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn auth_touch_uses_status_slash_command() {
        assert_eq!(STATUS_TOUCH_INPUT, b"/status\r");
    }

    #[test]
    fn windows_candidate_prefers_shell_path_before_fallbacks() {
        let selected = choose_windows_candidate(
            Some(PathBuf::from(r"C:\tools\claude.cmd")),
            Some(PathBuf::from(r"C:\Users\user\.local\bin\claude.exe")),
            Some(PathBuf::from(
                r"C:\Users\user\AppData\Roaming\npm\claude.cmd",
            )),
            Some(PathBuf::from(r"C:\WinGet\claude.exe")),
        )
        .expect("Claude candidate");
        assert_eq!(selected.source, "path");
        assert!(selected.shell_name);
    }
}
