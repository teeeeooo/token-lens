use crate::provider_cli_auth;
use portable_pty::CommandBuilder;
use std::env;
#[cfg(target_os = "windows")]
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const AUTH_TOUCH_TIMEOUT: Duration = Duration::from_secs(60);

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

pub(crate) fn refresh_credential_via_startup<F>(
    home: &Path,
    read_signature: F,
) -> Result<(), String>
where
    F: FnMut() -> Option<String>,
{
    let binary = discover_binary(home)
        .ok_or_else(|| "Claude CLI is not installed or discoverable".to_owned())?;
    let mut command = bare_command_builder(&binary);
    command.cwd(home);
    command.env("PWD", home);
    command.env("DISABLE_AUTOUPDATER", "1");
    command.env_remove("CLAUDE_CODE_OAUTH_TOKEN");
    for key in env::vars_os().map(|(key, _)| key) {
        if key.to_string_lossy().starts_with("ANTHROPIC_") {
            command.env_remove(key);
        }
    }
    provider_cli_auth::run_until_credential_change(
        "Claude",
        command,
        AUTH_TOUCH_TIMEOUT,
        read_signature,
    )
}

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

fn discover_binary(home: &Path) -> Option<PathBuf> {
    if let Some(path) = env::var_os("TOKEN_LENS_CLAUDE_BIN") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }

    #[cfg(target_os = "macos")]
    for candidate in [
        home.join(".local/bin/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let local = home.join(".local/bin/claude.exe");
        if local.is_file() {
            return Some(local);
        }
        if let Some(appdata) = env::var_os("APPDATA").map(PathBuf::from) {
            let npm = appdata.join("npm/claude.cmd");
            if npm.is_file() {
                return Some(npm);
            }
        }
        if let Some(binary) = discover_windows_winget_binary() {
            return Some(binary);
        }
        provider_cli_auth::find_executable_on_path(&["claude.exe", "claude.cmd", "claude.bat"])
    }

    #[cfg(not(target_os = "windows"))]
    provider_cli_auth::find_executable_on_path(&["claude"])
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
    use super::classify_refresh_error;

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
}
