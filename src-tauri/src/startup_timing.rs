use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime};

const FILE_NAME: &str = "startup-timing.jsonl";
const RETAIN_RUNS: usize = 10;
const SCHEMA_VERSION: u8 = 1;

const RENDERER_PHASES: &[&str] = &[
    "renderer-bootstrap-start",
    "shell-settings-ready",
    "today-scan-start",
    "today-scan-ready",
    "today-first-render",
    "quota-start",
    "quota-ready",
    "quota-failed",
    "slow-usage-start",
    "month-preload-ready",
    "alltime-preload-ready",
    "slow-usage-ready",
    "slow-usage-failed",
    "background-complete",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartupMark {
    phase: String,
    elapsed_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartupRecord {
    schema_version: u8,
    started_at: String,
    marks: Vec<StartupMark>,
}

#[derive(Debug)]
struct StartupState {
    directory: Option<PathBuf>,
    record: StartupRecord,
}

#[derive(Debug)]
pub(crate) struct StartupTiming {
    started: Instant,
    state: Mutex<StartupState>,
}

impl StartupTiming {
    pub(crate) fn new() -> Self {
        let started_at = DateTime::<Utc>::from(SystemTime::now()).to_rfc3339();
        Self {
            started: Instant::now(),
            state: Mutex::new(StartupState {
                directory: None,
                record: StartupRecord {
                    schema_version: SCHEMA_VERSION,
                    started_at,
                    marks: vec![StartupMark {
                        phase: "process-start".to_owned(),
                        elapsed_ms: 0.0,
                    }],
                },
            }),
        }
    }

    pub(crate) fn initialize(&self, directory: PathBuf) {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.directory = Some(directory);
    }

    pub(crate) fn record_internal(&self, phase: &'static str) -> f64 {
        self.record_phase(phase)
    }

    pub(crate) fn record_renderer(&self, phase: &str) -> Result<f64, String> {
        if !RENDERER_PHASES.contains(&phase) {
            return Err(format!("unsupported startup timing phase: {phase}"));
        }
        let elapsed = self.record_phase(phase);
        if phase == "background-complete" {
            self.persist()?;
        }
        Ok(elapsed)
    }

    pub(crate) fn persist(&self) -> Result<(), String> {
        let (directory, record) = {
            let guard = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (guard.directory.clone(), guard.record.clone())
        };
        let Some(directory) = directory else {
            return Ok(());
        };
        persist_record(&directory, &record)
    }

    fn record_phase(&self, phase: &str) -> f64 {
        let elapsed_ms = (self.started.elapsed().as_secs_f64() * 10_000.0).round() / 10.0;
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mark) = guard
            .record
            .marks
            .iter_mut()
            .find(|mark| mark.phase == phase)
        {
            mark.elapsed_ms = elapsed_ms;
        } else {
            guard.record.marks.push(StartupMark {
                phase: phase.to_owned(),
                elapsed_ms,
            });
        }
        guard
            .record
            .marks
            .sort_by(|left, right| left.elapsed_ms.total_cmp(&right.elapsed_ms));
        elapsed_ms
    }
}

fn log_path(directory: &Path) -> PathBuf {
    directory.join(FILE_NAME)
}

fn read_records(path: &Path) -> Vec<StartupRecord> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<StartupRecord>(line).ok())
        .collect()
}

fn persist_record(directory: &Path, current: &StartupRecord) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("failed to prepare startup timing directory: {error}"))?;
    let path = log_path(directory);
    let mut records = read_records(&path);
    records.retain(|record| record.started_at != current.started_at);
    records.push(current.clone());
    if records.len() > RETAIN_RUNS {
        let discard = records.len() - RETAIN_RUNS;
        records.drain(0..discard);
    }

    let mut output = Vec::new();
    for record in records {
        serde_json::to_writer(&mut output, &record)
            .map_err(|error| format!("failed to encode startup timing record: {error}"))?;
        output.push(b'\n');
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .map_err(|error| format!("failed to open startup timing log: {error}"))?;
    file.write_all(&output)
        .and_then(|_| file.flush())
        .map_err(|error| format!("failed to write startup timing log: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "token-lens-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn record_for(index: usize) -> StartupRecord {
        StartupRecord {
            schema_version: SCHEMA_VERSION,
            started_at: format!("2026-09-09T00:00:{index:02}Z"),
            marks: vec![StartupMark {
                phase: "process-start".to_owned(),
                elapsed_ms: 0.0,
            }],
        }
    }

    #[test]
    fn startup_log_retains_only_the_latest_ten_runs() {
        let directory = temp_dir("startup-retention");
        for index in 0..12 {
            persist_record(&directory, &record_for(index)).expect("persist startup timing");
        }
        let records = read_records(&log_path(&directory));
        assert_eq!(records.len(), 10);
        assert_eq!(records.first().unwrap().started_at, "2026-09-09T00:00:02Z");
        assert_eq!(records.last().unwrap().started_at, "2026-09-09T00:00:11Z");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn renderer_phase_is_allowlisted_and_background_completion_persists() {
        let directory = temp_dir("startup-renderer");
        let timing = StartupTiming::new();
        timing.initialize(directory.clone());
        timing
            .record_renderer("renderer-bootstrap-start")
            .expect("renderer phase");
        assert!(timing.record_renderer("credential-value").is_err());
        timing
            .record_renderer("background-complete")
            .expect("persist final startup timing");

        let records = read_records(&log_path(&directory));
        assert_eq!(records.len(), 1);
        let phases: Vec<_> = records[0]
            .marks
            .iter()
            .map(|mark| mark.phase.as_str())
            .collect();
        assert!(phases.contains(&"process-start"));
        assert!(phases.contains(&"renderer-bootstrap-start"));
        assert!(phases.contains(&"background-complete"));
        assert!(!phases.contains(&"credential-value"));
        let _ = fs::remove_dir_all(directory);
    }
}
