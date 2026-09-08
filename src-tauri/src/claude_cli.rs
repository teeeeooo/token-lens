use crate::domain::{QuotaWindow, QuotaWindowKind};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SOURCE: &str = "claude-cli";
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
const STARTUP_MIN_DELAY: Duration = Duration::from_millis(1800);
const STARTUP_FALLBACK_DELAY: Duration = Duration::from_secs(8);
const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const ENTER_INTERVAL: Duration = Duration::from_millis(800);
const SETTLE_AFTER_RESULT: Duration = Duration::from_millis(1200);
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const SESSION_ID_FILE: &str = ".token-lens-session-id";

pub(crate) fn classify_error(error: &str) -> (&'static str, &'static str) {
    let lower = error.to_ascii_lowercase();
    if lower.contains("not installed") || lower.contains("not discoverable") {
        return (
            "CLI_NOT_FOUND",
            "Claude CLI is not installed or discoverable",
        );
    }
    if lower.contains("authentication")
        || lower.contains("oauth session expired")
        || lower.contains("not logged in")
    {
        return (
            "CLI_AUTH_UNAVAILABLE",
            "Claude CLI authentication is unavailable",
        );
    }
    if lower.contains("rate limit") {
        return ("CLI_RATE_LIMITED", "Claude CLI usage is rate limited");
    }
    if lower.contains("timed out") || lower.contains("exited before returning usage") {
        return ("CLI_TIMEOUT", "Claude CLI usage probe did not complete");
    }
    if lower.contains("too much output") {
        return (
            "CLI_OUTPUT_LIMIT",
            "Claude CLI usage probe exceeded its output limit",
        );
    }
    if lower.contains("subscription does not expose")
        || lower.contains("did not expose supported quota windows")
    {
        return (
            "CLI_QUOTA_UNAVAILABLE",
            "Claude CLI did not expose numeric quota windows",
        );
    }
    if lower.contains("failed to load usage data") {
        return ("CLI_USAGE_FAILED", "Claude CLI failed to load usage data");
    }
    if lower.contains("pty") || lower.contains("spawn") || lower.contains("launch") {
        return (
            "CLI_LAUNCH_FAILED",
            "Claude CLI usage probe could not start",
        );
    }
    if lower.contains("probe directory")
        || lower.contains("probe settings")
        || lower.contains("session id")
    {
        return ("CLI_PROBE_IO", "Claude CLI probe storage is unavailable");
    }
    ("CLI_ERROR", "Claude CLI usage probe failed")
}

pub(crate) fn read_usage(home: &Path) -> Result<Vec<QuotaWindow>, String> {
    let binary = discover_binary(home)
        .ok_or_else(|| "Claude CLI is not installed or discoverable".to_owned())?;
    let output = capture_usage(&binary, home)?;
    parse_usage_output(&output)
}

pub(crate) fn auth_logged_in(home: &Path) -> Result<bool, String> {
    let binary = discover_binary(home)
        .ok_or_else(|| "Claude CLI is not installed or discoverable".to_owned())?;
    let mut command = auth_status_command(&binary);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command.env_remove("CLAUDE_CODE_OAUTH_TOKEN");
    for key in env::vars_os().map(|(key, _)| key) {
        if key.to_string_lossy().starts_with("ANTHROPIC_") {
            command.env_remove(key);
        }
    }
    let mut child = command
        .spawn()
        .map_err(|_| "Claude CLI auth status could not start".to_owned())?;
    let started = Instant::now();
    loop {
        if started.elapsed() >= AUTH_STATUS_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Claude CLI auth status timed out".to_owned());
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|_| "Claude CLI auth status could not be read".to_owned())?;
                if !output.status.success() {
                    return Err("Claude CLI auth status failed".to_owned());
                }
                return parse_auth_status_output(&output.stdout);
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return Err("Claude CLI auth status failed".to_owned()),
        }
    }
}

fn parse_auth_status_output(output: &[u8]) -> Result<bool, String> {
    let value = serde_json::from_slice::<serde_json::Value>(output)
        .map_err(|_| "Claude CLI auth status returned invalid JSON".to_owned())?;
    value
        .get("loggedIn")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Claude CLI auth status omitted loggedIn".to_owned())
}

fn auth_status_command(binary: &Path) -> Command {
    #[cfg(target_os = "windows")]
    if binary
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("cmd") || value.eq_ignore_ascii_case("bat"))
    {
        let line = format!("\"{}\" auth status", binary.display());
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/s", "/c", &line]);
        return command;
    }
    let mut command = Command::new(binary);
    command.args(["auth", "status"]);
    command
}

fn discover_binary(home: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("TOKEN_LENS_CLAUDE_BIN") {
        push_candidate(&mut candidates, PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    {
        push_candidate(&mut candidates, home.join(".local/bin/claude"));
        push_candidate(&mut candidates, PathBuf::from("/opt/homebrew/bin/claude"));
        push_candidate(&mut candidates, PathBuf::from("/usr/local/bin/claude"));
    }

    #[cfg(target_os = "windows")]
    {
        push_candidate(&mut candidates, home.join(".local/bin/claude.exe"));
        if let Some(appdata) = env::var_os("APPDATA").map(PathBuf::from) {
            push_candidate(&mut candidates, appdata.join("npm/claude.cmd"));
        }
        if let Some(binary) = discover_windows_winget_binary() {
            push_candidate(&mut candidates, binary);
        }
        push_candidate(&mut candidates, PathBuf::from("claude.exe"));
        push_candidate(&mut candidates, PathBuf::from("claude.cmd"));
    }

    push_candidate(&mut candidates, PathBuf::from("claude"));
    candidates
        .into_iter()
        .find(|path| !path.is_absolute() || path.is_file())
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

fn push_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidate.as_os_str().is_empty() && !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}
fn probe_directory(home: &Path) -> PathBuf {
    home.join(".token-lens/claude-probe")
}

fn prepare_probe_directory(home: &Path) -> Result<PathBuf, String> {
    let directory = probe_directory(home);
    let local_settings = directory.join(".claude/settings.local.json");
    fs::create_dir_all(local_settings.parent().expect("settings parent"))
        .map_err(|error| format!("Claude CLI probe directory unavailable: {error}"))?;
    let settings = serde_json::json!({ "disableDeepLinkRegistration": "disable" });
    fs::write(
        &local_settings,
        serde_json::to_vec_pretty(&settings).expect("static JSON must encode"),
    )
    .map_err(|error| format!("Claude CLI probe settings unavailable: {error}"))?;
    Ok(directory)
}

fn load_or_create_session_id(directory: &Path) -> Result<String, String> {
    let path = directory.join(SESSION_ID_FILE);
    if let Ok(existing) = fs::read_to_string(&path) {
        let existing = existing.trim();
        if looks_like_uuid(existing) {
            return Ok(existing.to_owned());
        }
    }
    let id = generated_session_id();
    fs::write(&path, format!("{id}\n"))
        .map_err(|error| format!("Claude CLI probe session id unavailable: {error}"))?;
    Ok(id)
}
fn looks_like_uuid(value: &str) -> bool {
    value.len() == 36
        && [8, 13, 18, 23]
            .iter()
            .all(|index| value.as_bytes()[*index] == b'-')
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

fn generated_session_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mixed = now ^ ((std::process::id() as u128) << 64) ^ 0x544f4b454e4c454e535f434c41554445;
    let mut hex = format!("{mixed:032x}");
    hex.replace_range(12..13, "4");
    hex.replace_range(16..17, "8");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn claude_config_root(home: &Path) -> PathBuf {
    env::var_os("CLAUDE_CONFIG_DIR")
        .and_then(|value| value.to_string_lossy().split(',').next().map(PathBuf::from))
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".claude"))
}
fn cleanup_probe_artifacts(home: &Path, probe: &Path) {
    let project = claude_project_directory_name(probe);
    let directory = claude_config_root(home).join("projects").join(project);
    let Ok(entries) = fs::read_dir(&directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            let _ = fs::remove_file(path);
        }
    }
    if fs::read_dir(&directory)
        .ok()
        .is_some_and(|mut entries| entries.next().is_none())
    {
        let _ = fs::remove_dir(directory);
    }
}

fn claude_project_directory_name(directory: &Path) -> String {
    let raw = directory.to_string_lossy();
    let mut sanitized = raw
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    if sanitized.len() <= 200 {
        return sanitized;
    }
    sanitized.truncate(200);
    format!("{sanitized}-{}", javascript_hash_base36(&raw))
}
fn javascript_hash_base36(value: &str) -> String {
    let mut hash: i32 = 0;
    for code_unit in value.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(code_unit as i32);
    }
    let mut magnitude = i64::from(hash).unsigned_abs();
    if magnitude == 0 {
        return "0".to_owned();
    }
    let mut out = Vec::new();
    while magnitude > 0 {
        let digit = (magnitude % 36) as u8;
        out.push(if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        });
        magnitude /= 36;
    }
    out.reverse();
    String::from_utf8(out).expect("base36 is ASCII")
}

fn capture_usage(binary: &Path, home: &Path) -> Result<String, String> {
    let probe = prepare_probe_directory(home)?;
    cleanup_probe_artifacts(home, &probe);
    let session_id = load_or_create_session_id(&probe)?;
    let result = capture_usage_inner(binary, &probe, &session_id);
    cleanup_probe_artifacts(home, &probe);
    result
}
fn command_builder(binary: &Path, session_id: &str, probe: &Path) -> CommandBuilder {
    #[cfg(target_os = "windows")]
    let mut command = if binary
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("cmd") || value.eq_ignore_ascii_case("bat"))
    {
        let line = format!(
            "\"{}\" --allowed-tools \"\" --strict-mcp-config --session-id {}",
            binary.display(),
            session_id
        );
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/d", "/s", "/c", &line]);
        command
    } else {
        let mut command = CommandBuilder::new(binary);
        command.args([
            "--allowed-tools",
            "",
            "--strict-mcp-config",
            "--session-id",
            session_id,
        ]);
        command
    };

    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut command = CommandBuilder::new(binary);
        command.args([
            "--allowed-tools",
            "",
            "--strict-mcp-config",
            "--session-id",
            session_id,
        ]);
        command
    };

    command.cwd(probe);
    command.env("PWD", probe);
    command.env("DISABLE_AUTOUPDATER", "1");
    command.env_remove("CLAUDE_CODE_OAUTH_TOKEN");
    for key in env::vars_os().map(|(key, _)| key) {
        if key.to_string_lossy().starts_with("ANTHROPIC_") {
            command.env_remove(key);
        }
    }
    command
}

fn capture_usage_inner(binary: &Path, probe: &Path, session_id: &str) -> Result<String, String> {
    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize {
            rows: 50,
            cols: 160,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Claude CLI PTY unavailable: {error}"))?;
    let mut child = pair
        .slave
        .spawn_command(command_builder(binary, session_id, probe))
        .map_err(|error| format!("Claude CLI launch failed: {error}"))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Claude CLI PTY reader unavailable: {error}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Claude CLI PTY writer unavailable: {error}"))?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader_thread = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if tx.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let started = Instant::now();
    let mut usage_sent = false;
    let mut last_enter = Instant::now();
    let mut stop_seen_at = None;
    let mut output = Vec::new();
    let mut sent_prompts = Vec::new();

    let capture_result = loop {
        if started.elapsed() >= PROBE_TIMEOUT {
            break Err("Claude CLI usage probe timed out".to_owned());
        }
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(80)) {
            if output.len().saturating_add(chunk.len()) > MAX_OUTPUT_BYTES {
                break Err("Claude CLI usage probe produced too much output".to_owned());
            }
            output.extend_from_slice(&chunk);
        }
        let clean = String::from_utf8_lossy(&strip_ansi_escapes::strip(&output)).into_owned();
        let normalized = normalized_scan(&clean);
        for (needle, keys) in prompt_responses() {
            if normalized.contains(needle) && !sent_prompts.contains(&needle) {
                let _ = writer.write_all(keys.as_bytes());
                let _ = writer.flush();
                sent_prompts.push(needle);
            }
        }

        if !usage_sent
            && ((started.elapsed() >= STARTUP_MIN_DELAY
                && capture_is_interactive_ready(&normalized))
                || started.elapsed() >= STARTUP_FALLBACK_DELAY)
        {
            if let Err(error) = writer.write_all(b"/usage\r").and_then(|_| writer.flush()) {
                break Err(format!("Claude CLI PTY write failed: {error}"));
            }
            usage_sent = true;
            last_enter = Instant::now();
        }

        if usage_sent && last_enter.elapsed() >= ENTER_INTERVAL {
            let _ = writer.write_all(b"\r");
            let _ = writer.flush();
            last_enter = Instant::now();
        }

        if usage_sent && capture_has_terminal_result(&normalized) {
            stop_seen_at.get_or_insert_with(Instant::now);
        }
        if stop_seen_at.is_some_and(|seen| seen.elapsed() >= SETTLE_AFTER_RESULT) {
            break Ok(clean);
        }
        if child.try_wait().ok().flatten().is_some() {
            break Err("Claude CLI exited before returning usage".to_owned());
        }
    };
    let _ = writer.write_all(b"/exit\r");
    let _ = writer.flush();
    let _ = child.kill();
    let _ = child.wait();
    drop(writer);
    drop(pair.master);
    let _ = reader_thread.join();
    capture_result
}

fn prompt_responses() -> &'static [(&'static str, &'static str)] {
    &[
        ("doyoutrustthefilesinthisfolder", "y\r"),
        ("quicksafetycheck", "\r"),
        ("yesitrustthisfolder", "\r"),
        ("readytocodehere", "\r"),
        ("pressentertocontinue", "\r"),
        ("showplanusagelimits", "\r"),
        ("showplan", "\r"),
    ]
}

fn normalized_scan(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn capture_is_interactive_ready(normalized: &str) -> bool {
    !normalized.contains("waitingforauthentication")
        && !normalized.contains("refreshing")
        && (normalized.contains("claudecode")
            || normalized.contains("readytocodehere")
            || normalized.contains("doyoutrustthefilesinthisfolder"))
}

fn capture_has_terminal_result(normalized: &str) -> bool {
    let has_session = normalized.contains("currentsession") && normalized.contains('%');
    let has_usage_credits = normalized.contains("usagecredits")
        && (normalized.contains("spent") || normalized.contains("%used"));
    let has_cowork_credit = normalized.contains("claudecodeandcoworkcredit")
        && (normalized.contains("%used") || normalized.contains("%remaining"));
    has_session
        || has_usage_credits
        || has_cowork_credit
        || normalized.contains("currentlyusingyoursubscription")
        || normalized.contains("failedtoloadusagedata")
        || normalized.contains("oauthsessionexpired")
        || normalized.contains("authenticationerror")
        || normalized.contains("notloggedin")
        || normalized.contains("ratelimit")
}

fn parse_usage_output(raw: &str) -> Result<Vec<QuotaWindow>, String> {
    let stripped = strip_ansi_escapes::strip(raw.as_bytes());
    let clean = String::from_utf8_lossy(&stripped).replace('\r', "\n");
    let lower = clean.to_lowercase();
    if lower.contains("oauth session expired") || lower.contains("not logged in") {
        return Err("Claude CLI authentication is unavailable; run `claude login`".to_owned());
    }
    if lower.contains("failed to load usage data") {
        return Err("Claude CLI failed to load usage data".to_owned());
    }
    if lower.contains("rate limit") && !lower.contains("current session") {
        return Err("Claude CLI usage is rate limited".to_owned());
    }

    let lines = clean
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let session = percent_near_labels(&lines, &["current session"]);
    let weekly = percent_near_weekly(&lines);
    let mut windows = Vec::new();
    if let Some(remaining) = session {
        windows.push(cli_window(QuotaWindowKind::Session, "5h", remaining));
    }
    if let Some(remaining) = weekly {
        windows.push(cli_window(QuotaWindowKind::Weekly, "Weekly", remaining));
    }
    windows.extend(parse_enterprise_credit_windows(&clean));
    if windows.is_empty() {
        if lower.contains("currently using your subscription")
            && lower.contains("claude code usage")
        {
            return Err(
                "Claude CLI subscription does not expose numeric session quota data".to_owned(),
            );
        }
        return Err("Claude CLI /usage did not expose supported quota windows".to_owned());
    }
    Ok(windows)
}

fn cli_window(kind: QuotaWindowKind, label: &str, remaining_percent: f64) -> QuotaWindow {
    let remaining_percent = remaining_percent.clamp(0.0, 100.0);
    QuotaWindow {
        kind,
        label: label.to_owned(),
        metric: "quota",
        additional: false,
        used: None,
        limit: None,
        remaining: None,
        used_percent: Some(100.0 - remaining_percent),
        remaining_percent: Some(remaining_percent),
        remaining_label: None,
        resets_at: None,
        currency: None,
        show_meter: true,
        source: SOURCE,
    }
}

fn parse_enterprise_credit_windows(clean: &str) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    if let Some(section) = section_after_last_label(clean, "Claude Code and Cowork credit") {
        if let Some(remaining_percent) = remaining_percent_in_text(&section) {
            windows.push(QuotaWindow {
                kind: QuotaWindowKind::Billing,
                label: "Claude Code and Cowork credit".to_owned(),
                metric: "quota",
                additional: true,
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some(100.0 - remaining_percent),
                remaining_percent: Some(remaining_percent),
                remaining_label: None,
                // The CLI currently exposes an expiry date without a precise timestamp.
                // Do not misrepresent `Expires` as a quota reset instant.
                resets_at: None,
                currency: None,
                show_meter: true,
                source: SOURCE,
            });
        }
    }
    if let Some(section) = section_after_last_label(clean, "Usage credits") {
        if let Some((used, limit)) = dollar_spend_pair(&section) {
            let remaining = (limit - used).max(0.0);
            let used_percent = (limit > 0.0).then_some((used / limit * 100.0).clamp(0.0, 100.0));
            windows.push(QuotaWindow {
                kind: QuotaWindowKind::Billing,
                label: "Usage credits".to_owned(),
                metric: "spend",
                additional: false,
                used: Some(used),
                limit: Some(limit),
                remaining: Some(remaining),
                used_percent,
                remaining_percent: used_percent.map(|value| 100.0 - value),
                remaining_label: None,
                // `Resets Oct 1 (Asia/Seoul)` has no time-of-day. Keep the value
                // truthful instead of inventing an RFC3339 reset instant.
                resets_at: None,
                currency: Some("USD".to_owned()),
                show_meter: true,
                source: SOURCE,
            });
        }
    }
    windows
}

fn section_after_last_label(text: &str, label: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let start = lower.rfind(&label.to_ascii_lowercase())?;
    Some(text[start..].chars().take(600).collect())
}

fn remaining_percent_in_text(text: &str) -> Option<f64> {
    let percent_index = text.find('%')?;
    let prefix = text[..percent_index].trim_end();
    let start = prefix
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '.')
        .last()
        .map(|(index, _)| index)?;
    let raw = prefix[start..].parse::<f64>().ok()?.clamp(0.0, 100.0);
    let suffix = normalized_scan(
        &text[percent_index + 1..]
            .chars()
            .take(80)
            .collect::<String>(),
    );
    if suffix.contains("used") || suffix.contains("spent") || suffix.contains("consumed") {
        Some(100.0 - raw)
    } else if suffix.contains("left")
        || suffix.contains("remaining")
        || suffix.contains("available")
    {
        Some(raw)
    } else {
        None
    }
}

fn dollar_spend_pair(text: &str) -> Option<(f64, f64)> {
    let lower = text.to_ascii_lowercase();
    let spent = lower.find("spent")?;
    let prefix = &text[..spent];
    let mut values = Vec::new();
    let bytes = prefix.as_bytes();
    let mut index = 0;
    while index < bytes.len() && values.len() < 2 {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        index += 1;
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
            index += 1;
        }
        if index > start {
            if let Ok(value) = prefix[start..index].parse::<f64>() {
                values.push(value);
            }
        }
    }
    match values.as_slice() {
        [used, limit]
            if used.is_finite() && limit.is_finite() && *used >= 0.0 && *limit >= *used =>
        {
            Some((*used, *limit))
        }
        _ => None,
    }
}

fn percent_near_labels(lines: &[&str], labels: &[&str]) -> Option<f64> {
    labels
        .iter()
        .find_map(|label| percent_near_label(lines, label))
}
fn percent_near_weekly(lines: &[&str]) -> Option<f64> {
    for label in ["current week (all models)", "current week"] {
        for (index, line) in lines.iter().enumerate() {
            let normalized = normalized_label(line);
            if !normalized.contains(&normalized_label(label)) {
                continue;
            }
            if label == "current week"
                && ["opus", "sonnet", "haiku"]
                    .iter()
                    .any(|model| normalized.contains(model))
            {
                continue;
            }
            if let Some(value) = percent_in_window(lines, index) {
                return Some(value);
            }
        }
    }
    None
}

fn percent_near_label(lines: &[&str], label: &str) -> Option<f64> {
    let label = normalized_label(label);
    lines.iter().enumerate().find_map(|(index, line)| {
        normalized_label(line)
            .contains(&label)
            .then(|| percent_in_window(lines, index))
            .flatten()
    })
}

fn normalized_label(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}
fn percent_in_window(lines: &[&str], start: usize) -> Option<f64> {
    for (offset, line) in lines.iter().skip(start).take(12).enumerate() {
        if offset > 0 {
            let label = normalized_label(line);
            if label.contains("currentsession") || label.contains("currentweek") {
                break;
            }
        }
        if let Some(value) = percent_from_line(line) {
            return Some(value);
        }
    }
    None
}

fn percent_from_line(line: &str) -> Option<f64> {
    let percent_index = line.find('%')?;
    let prefix = line[..percent_index].trim_end();
    let start = prefix
        .char_indices()
        .rev()
        .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '.')
        .last()
        .map(|(index, _)| index)?;
    let raw = prefix[start..].parse::<f64>().ok()?.clamp(0.0, 100.0);
    let lower = line.to_ascii_lowercase();
    if ["used", "spent", "consumed"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return Some(100.0 - raw);
    }
    if ["left", "remaining", "available"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return Some(raw);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_used_and_remaining_percentages_from_usage_panel() {
        let output = r#"
            Current session
            37% used
            Resets in 2 hr
            Current week (all models)
            61% remaining
        "#;
        let windows = parse_usage_output(output).expect("usage panel");
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].remaining_percent, Some(63.0));
        assert_eq!(windows[1].remaining_percent, Some(61.0));
    }

    #[test]
    fn accepts_session_only_managed_account_output() {
        let output = "Current session\n12% left\n";
        let windows = parse_usage_output(output).expect("session-only usage");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].kind, QuotaWindowKind::Session);
        assert_eq!(windows[0].remaining_percent, Some(12.0));
    }

    #[test]
    fn does_not_treat_scoped_weekly_window_as_canonical_weekly() {
        let output = "Current session\n80% left\nCurrent week (Sonnet)\n50% left\n";
        let windows = parse_usage_output(output).expect("usage panel");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].kind, QuotaWindowKind::Session);
    }
    #[test]
    fn subscription_notice_without_numeric_quota_is_not_fabricated() {
        let error = parse_usage_output(
            "You are currently using your subscription to power your Claude Code usage",
        )
        .expect_err("subscription notice is not quota data");
        assert!(error.contains("does not expose numeric"));
    }

    #[test]
    fn parses_enterprise_credit_usage_with_glued_tui_spacing() {
        let output = "Claude Code and Cowork credit\n100%used\nExpires September 10\nUsage credits%0%used$0.01 / $38.00spent\nResets Oct 1 (Asia/Seoul)";
        let windows = parse_usage_output(output).expect("enterprise credit usage");
        assert_eq!(windows.len(), 2);
        let cowork = windows
            .iter()
            .find(|window| window.label == "Claude Code and Cowork credit")
            .expect("cowork credit");
        assert_eq!(cowork.kind, QuotaWindowKind::Billing);
        assert_eq!(cowork.remaining_percent, Some(0.0));
        assert_eq!(cowork.resets_at, None);
        let spend = windows
            .iter()
            .find(|window| window.label == "Usage credits")
            .expect("usage credits");
        assert_eq!(spend.metric, "spend");
        assert_eq!(spend.used, Some(0.01));
        assert_eq!(spend.limit, Some(38.0));
        assert_eq!(spend.remaining, Some(37.99));
        assert_eq!(spend.currency.as_deref(), Some("USD"));
        assert_eq!(spend.resets_at, None);
    }

    #[test]
    fn auth_status_json_reads_only_logged_in_state() {
        assert!(
            parse_auth_status_output(br#"{"loggedIn":true,"authMethod":"claude.ai"}"#)
                .expect("auth status")
        );
        assert!(!parse_auth_status_output(br#"{"loggedIn":false}"#).expect("auth status"));
    }

    #[test]
    fn enterprise_credit_render_is_a_terminal_result() {
        assert!(capture_has_terminal_result(&normalized_scan(
            "Usage credits%0%used$0.01/$38.00spent"
        )));
        assert!(capture_has_terminal_result(&normalized_scan(
            "Claude Code and Cowork credit 100% used"
        )));
    }

    #[test]
    fn interactive_ready_detection_waits_out_authentication_spinner() {
        assert!(!capture_is_interactive_ready(&normalized_scan(
            "Claude Code Waiting for authentication..."
        )));
        assert!(capture_is_interactive_ready(&normalized_scan(
            "Claude Code Ready to code here?"
        )));
    }

    #[test]
    fn project_directory_mapping_matches_claude_shape() {
        let path = if cfg!(windows) {
            Path::new(r"C:\Users\tester\.token-lens\claude-probe")
        } else {
            Path::new("/Users/tester/.token-lens/claude-probe")
        };
        let mapped = claude_project_directory_name(path);
        assert!(mapped.contains("Users-tester"));
        assert!(!mapped.contains('/') && !mapped.contains('\\'));
    }

    #[test]
    fn generated_probe_session_id_is_uuid_shaped() {
        assert!(looks_like_uuid(&generated_session_id()));
    }
}
