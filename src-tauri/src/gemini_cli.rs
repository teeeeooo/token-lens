use crate::provider_cli_auth;
use portable_pty::CommandBuilder;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

const AUTH_TOUCH_TIMEOUT: Duration = Duration::from_secs(240);

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

pub(crate) fn refresh_credential_via_startup<F>(
    home: &Path,
    read_signature: F,
) -> Result<(), String>
where
    F: FnMut() -> Option<String>,
{
    let binary = discover_binary(home)
        .ok_or_else(|| "Gemini CLI is not installed or discoverable".to_owned())?;
    let mut command = bare_command_builder(&binary);
    command.cwd(home);
    command.env("PWD", home);
    provider_cli_auth::run_until_credential_change(
        "Gemini",
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

fn discover_binary(_home: &Path) -> Option<PathBuf> {
    if let Some(path) = env::var_os("TOKEN_LENS_GEMINI_BIN") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }

    #[cfg(target_os = "macos")]
    for candidate in [
        _home.join(".local/bin/gemini"),
        _home.join(".npm-global/bin/gemini"),
        PathBuf::from("/opt/homebrew/bin/gemini"),
        PathBuf::from("/usr/local/bin/gemini"),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA").map(PathBuf::from) {
            let npm = appdata.join("npm/gemini.cmd");
            if npm.is_file() {
                return Some(npm);
            }
        }
        provider_cli_auth::find_executable_on_path(&["gemini.exe", "gemini.cmd", "gemini.bat"])
    }

    #[cfg(not(target_os = "windows"))]
    provider_cli_auth::find_executable_on_path(&["gemini"])
}

#[cfg(test)]
mod tests {
    use super::classify_refresh_error;

    #[test]
    fn refresh_errors_are_sanitized_and_classified() {
        assert_eq!(
            classify_refresh_error("Gemini CLI auth refresh timed out after 240s").0,
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
}
