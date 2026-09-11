use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const RETENTION_DAYS: i64 = 3;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024;
const DEDUPE_MS: u64 = 5 * 60 * 1000;
const FILE_PREFIX: &str = "provider-errors-";
const FILE_SUFFIX: &str = ".jsonl";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderIncident {
    pub provider: &'static str,
    pub category: &'static str,
    pub code: &'static str,
    pub stage: &'static str,
    pub result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_reread: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_stage: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_source: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_good_used: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogRecord<'a> {
    schema_version: u8,
    timestamp: String,
    #[serde(flatten)]
    incident: &'a ProviderIncident,
}

#[derive(Debug, Default)]
struct LogState {
    directory: Option<PathBuf>,
    last_cleanup_day: Option<NaiveDate>,
    recent_signatures: HashMap<String, u64>,
}
fn state() -> &'static Mutex<LogState> {
    static STATE: OnceLock<Mutex<LogState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(LogState::default()))
}

pub(crate) fn initialize(directory: PathBuf) {
    let _ = fs::create_dir_all(&directory);
    let today = utc_now().date_naive();
    let mut guard = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.directory = Some(directory.clone());
    guard.last_cleanup_day = Some(today);
    let _ = cleanup_directory(&directory, today, MAX_TOTAL_BYTES);
}

pub(crate) fn record(incident: ProviderIncident) {
    let now = utc_now();
    let now_ms = now_ms();
    let day = now.date_naive();
    let signature = incident_signature(&incident);
    let mut guard = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(directory) = guard.directory.clone() else {
        return;
    };

    if guard.last_cleanup_day != Some(day) {
        let _ = cleanup_directory(&directory, day, MAX_TOTAL_BYTES);
        guard.last_cleanup_day = Some(day);
        guard
            .recent_signatures
            .retain(|_, seen| now_ms.saturating_sub(*seen) < DEDUPE_MS);
    }
    if guard
        .recent_signatures
        .get(&signature)
        .is_some_and(|seen| now_ms.saturating_sub(*seen) < DEDUPE_MS)
    {
        return;
    }
    let record = LogRecord {
        schema_version: 1,
        timestamp: now.to_rfc3339(),
        incident: &incident,
    };
    let Ok(mut line) = serde_json::to_vec(&record) else {
        return;
    };
    line.push(b'\n');

    let path = log_path(&directory, day);
    if !make_room(&directory, day, &path, line.len() as u64) {
        return;
    }
    let file = OpenOptions::new().create(true).append(true).open(&path);
    let Ok(mut file) = file else { return };
    if file.write_all(&line).is_ok() {
        guard.recent_signatures.insert(signature, now_ms);
    }
}

fn incident_signature(incident: &ProviderIncident) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        incident.provider,
        incident.category,
        incident.code,
        incident.stage,
        incident.result,
        incident.recovery_code.unwrap_or(""),
        incident.trigger_code.unwrap_or(""),
        incident.trigger_stage.unwrap_or(""),
        incident.discovery_code.unwrap_or(""),
        incident.cli_source.unwrap_or("")
    )
}

fn utc_now() -> DateTime<Utc> {
    DateTime::<Utc>::from(SystemTime::now())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn log_path(directory: &Path, day: NaiveDate) -> PathBuf {
    directory.join(format!(
        "{FILE_PREFIX}{}{FILE_SUFFIX}",
        day.format("%Y-%m-%d")
    ))
}

fn cleanup_directory(
    directory: &Path,
    today: NaiveDate,
    max_total_bytes: u64,
) -> std::io::Result<()> {
    let mut files = provider_log_files(directory)?;
    for (path, day, _) in &files {
        if today.signed_duration_since(*day).num_days() >= RETENTION_DAYS {
            let _ = fs::remove_file(path);
        }
    }
    files = provider_log_files(directory)?;
    let mut total = files.iter().map(|(_, _, size)| *size).sum::<u64>();
    files.sort_by_key(|(_, day, _)| *day);
    for (path, _, size) in files {
        if total <= max_total_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

fn provider_log_files(directory: &Path) -> std::io::Result<Vec<(PathBuf, NaiveDate, u64)>> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(files);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(day) = day_from_path(&path) else {
            continue;
        };
        let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        files.push((path, day, size));
    }
    Ok(files)
}
fn day_from_path(path: &Path) -> Option<NaiveDate> {
    let name = path.file_name()?.to_str()?;
    let raw = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()
}

fn make_room(directory: &Path, today: NaiveDate, current: &Path, incoming: u64) -> bool {
    let _ = cleanup_directory(directory, today, MAX_TOTAL_BYTES);
    let mut files = provider_log_files(directory).unwrap_or_default();
    let mut total = files.iter().map(|(_, _, size)| *size).sum::<u64>();
    if total.saturating_add(incoming) <= MAX_TOTAL_BYTES {
        return true;
    }
    files.sort_by_key(|(_, day, _)| *day);
    for (path, _, size) in files {
        if path == current {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
        if total.saturating_add(incoming) <= MAX_TOTAL_BYTES {
            return true;
        }
    }
    total.saturating_add(incoming) <= MAX_TOTAL_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("token-lens-{label}-{}", now_ms()));
        fs::create_dir_all(&dir).expect("temp log directory");
        dir
    }
    #[test]
    fn record_writes_sanitized_error_only_jsonl_and_deduplicates() {
        let dir = temp_dir("log-record");
        initialize(dir.clone());
        let incident = ProviderIncident {
            provider: "claude",
            category: "auth",
            code: "HTTP_401",
            stage: "oauth_usage",
            result: "recovered_cli",
            credential_reread: Some(true),
            credential_changed: Some(false),
            cli_fallback: Some(true),
            recovery_code: None,
            trigger_code: None,
            trigger_stage: None,
            discovery_code: None,
            cli_source: Some("path"),
            retry_after_seconds: None,
            cooldown_seconds: None,
            last_good_used: None,
        };
        record(incident.clone());
        record(incident);
        let path = log_path(&dir, utc_now().date_naive());
        let text = fs::read_to_string(path).expect("provider incident log");
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("HTTP_401"));
        assert!(text.contains("credentialReread"));
        assert!(text.contains("\"cliSource\":\"path\""));
        assert!(!text.contains("accessToken"));
        assert!(!text.contains("refreshToken"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn recovery_trigger_fields_serialize_separately_from_recovery_identity() {
        let incident = ProviderIncident {
            provider: "claude",
            category: "auth",
            code: "CREDENTIAL_RECOVERY",
            stage: "credential_recovery",
            result: "credential_change_observed",
            credential_reread: Some(true),
            credential_changed: Some(true),
            cli_fallback: Some(true),
            recovery_code: Some("CLI_CREDENTIAL_CHANGE_OBSERVED"),
            trigger_code: Some("HTTP_401"),
            trigger_stage: Some("oauth_usage"),
            discovery_code: None,
            cli_source: Some("path"),
            retry_after_seconds: None,
            cooldown_seconds: None,
            last_good_used: None,
        };
        let json = serde_json::to_string(&incident).expect("serialize incident");
        assert!(json.contains("\"code\":\"CREDENTIAL_RECOVERY\""));
        assert!(json.contains("\"stage\":\"credential_recovery\""));
        assert!(json.contains("\"triggerCode\":\"HTTP_401\""));
        assert!(json.contains("\"triggerStage\":\"oauth_usage\""));
    }

    #[test]
    fn cleanup_removes_logs_older_than_three_days() {
        let dir = temp_dir("log-retention");
        let today = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        fs::write(
            log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 4).unwrap()),
            b"old",
        )
        .unwrap();
        fs::write(
            log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()),
            b"keep",
        )
        .unwrap();
        cleanup_directory(&dir, today, MAX_TOTAL_BYTES).unwrap();
        assert!(!log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 4).unwrap()).exists());
        assert!(log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cleanup_enforces_total_size_from_oldest_first() {
        let dir = temp_dir("log-size");
        let today = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        fs::write(
            log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()),
            vec![b'a'; 600],
        )
        .unwrap();
        fs::write(
            log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 6).unwrap()),
            vec![b'b'; 600],
        )
        .unwrap();
        cleanup_directory(&dir, today, 700).unwrap();
        assert!(!log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()).exists());
        assert!(log_path(&dir, NaiveDate::from_ymd_opt(2026, 9, 6).unwrap()).exists());
        let _ = fs::remove_dir_all(dir);
    }
}
