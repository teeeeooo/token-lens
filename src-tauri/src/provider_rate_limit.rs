use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const STATE_FILE_NAME: &str = "provider-rate-limits.json";
const MAX_PERSISTED_HORIZON_MS: u64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ProviderRateLimit {
    Claude,
    Gemini,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StoredRateLimits {
    #[serde(default)]
    claude_until_ms: u64,
    #[serde(default)]
    gemini_until_ms: u64,
}

#[derive(Debug, Default)]
struct RateLimitState {
    path: Option<PathBuf>,
    stored: StoredRateLimits,
}

fn state() -> &'static Mutex<RateLimitState> {
    static STATE: OnceLock<Mutex<RateLimitState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(RateLimitState::default()))
}

pub(crate) fn initialize(config_dir: PathBuf) {
    let _ = fs::create_dir_all(&config_dir);
    let path = config_dir.join(STATE_FILE_NAME);
    let now = now_ms();
    let stored = read_state(&path)
        .map(|stored| normalize_loaded_state(stored, now))
        .unwrap_or_default();
    let mut guard = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.path = Some(path);
    guard.stored = stored;
}

pub(crate) fn remaining_ms(provider: ProviderRateLimit, now: u64) -> Option<u64> {
    let guard = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let until = deadline(&guard.stored, provider);
    (until > now).then_some(until - now)
}

pub(crate) fn persist_until(provider: ProviderRateLimit, until_ms: u64) {
    let mut guard = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let current = deadline(&guard.stored, provider);
    if until_ms <= current {
        return;
    }
    set_deadline(&mut guard.stored, provider, until_ms);
    persist_locked(&guard);
}

pub(crate) fn clear(provider: ProviderRateLimit) {
    let mut guard = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if deadline(&guard.stored, provider) == 0 {
        return;
    }
    set_deadline(&mut guard.stored, provider, 0);
    persist_locked(&guard);
}

fn deadline(stored: &StoredRateLimits, provider: ProviderRateLimit) -> u64 {
    match provider {
        ProviderRateLimit::Claude => stored.claude_until_ms,
        ProviderRateLimit::Gemini => stored.gemini_until_ms,
    }
}

fn set_deadline(stored: &mut StoredRateLimits, provider: ProviderRateLimit, until_ms: u64) {
    match provider {
        ProviderRateLimit::Claude => stored.claude_until_ms = until_ms,
        ProviderRateLimit::Gemini => stored.gemini_until_ms = until_ms,
    }
}

fn persist_locked(state: &RateLimitState) {
    let Some(path) = state.path.as_deref() else {
        return;
    };
    let _ = write_state(path, &state.stored);
}

fn read_state(path: &Path) -> Option<StoredRateLimits> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_state(path: &Path, stored: &StoredRateLimits) -> Result<(), String> {
    let text = serde_json::to_string(stored)
        .map_err(|error| format!("failed to serialize provider rate-limit state: {error}"))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("failed to write provider rate-limit state: {error}"))
}

fn normalize_loaded_state(mut stored: StoredRateLimits, now: u64) -> StoredRateLimits {
    let max_until = now.saturating_add(MAX_PERSISTED_HORIZON_MS);
    for until in [&mut stored.claude_until_ms, &mut stored.gemini_until_ms] {
        if *until <= now {
            *until = 0;
        } else if *until > max_until {
            *until = max_until;
        }
    }
    stored
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_state_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("token-lens-{label}-{}.json", now_ms()))
    }

    #[test]
    fn state_round_trip_preserves_only_provider_deadlines() {
        let path = temp_state_path("rate-limit-round-trip");
        let expected = StoredRateLimits {
            claude_until_ms: 11_000,
            gemini_until_ms: 22_000,
        };
        write_state(&path, &expected).expect("write state");
        assert_eq!(read_state(&path), Some(expected));
        let text = fs::read_to_string(&path).expect("read state text");
        assert!(!text.contains("token"));
        assert!(!text.contains("account"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn loaded_deadlines_expire_and_are_bounded_to_one_hour() {
        let now = 1_000_000;
        let normalized = normalize_loaded_state(
            StoredRateLimits {
                claude_until_ms: now - 1,
                gemini_until_ms: now + MAX_PERSISTED_HORIZON_MS + 999_999,
            },
            now,
        );
        assert_eq!(normalized.claude_until_ms, 0);
        assert_eq!(normalized.gemini_until_ms, now + MAX_PERSISTED_HORIZON_MS);
    }
}
