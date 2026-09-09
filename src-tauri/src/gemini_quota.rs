use crate::domain::{QuotaProvider, QuotaReport, QuotaWindow, QuotaWindowKind, SupportedProvider};
use crate::gemini_cli;
use crate::google_code_assist::{self, CodeAssistError, LoadSnapshot, QuotaBucket};
use crate::provider_error_log::{self, ProviderIncident};
#[cfg(any(target_os = "windows", test))]
use aes_gcm::{
    aead::{consts::U16, AeadInPlace, KeyInit},
    aes::Aes256,
    AesGcm, Nonce, Tag,
};
use chrono::DateTime;
#[cfg(any(target_os = "windows", test))]
use scrypt::{scrypt, Params as ScryptParams};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE: &str = "gemini-code-assist";
const TOKEN_EXPIRY_SAFETY_MS: u64 = 30_000;
const MAX_RATE_LIMIT_COOLDOWN_MS: u64 = 60 * 60 * 1000;
const RATE_LIMIT_BACKOFF_STEPS_MS: [u64; 4] = [
    5 * 60 * 1000,
    15 * 60 * 1000,
    30 * 60 * 1000,
    60 * 60 * 1000,
];
const LAST_GOOD_TTL_MS: u64 = 30 * 60 * 1000;
const STALE_DIAGNOSTIC_PREFIX: &str = "Stale Gemini quota";
const AUTH_REFRESH_FAILURE_COOLDOWN_MS: u64 = 5 * 60 * 1000;
static AUTH_REFRESH_RUNNING: AtomicBool = AtomicBool::new(false);
static AUTH_REFRESH_RETRY_AFTER_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
struct StoredGeminiCredential {
    access_token: Option<String>,
    expiry_date: Option<u64>,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGeminiKeychainCredential {
    token: Option<StoredGeminiKeychainToken>,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredGeminiKeychainToken {
    access_token: Option<String>,
    expires_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct StoredGeminiAccounts {
    active: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedGeminiProvider {
    captured_at_ms: u64,
    provider: QuotaProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CredentialBaseline {
    Missing,
    Present(u64),
}

#[derive(Debug, Default)]
struct GeminiRuntimeState {
    cooldown_until_ms: u64,
    rate_limit_streak: u32,
    last_rate_limit_cooldown_ms: u64,
    auth_recovery_baseline: Option<CredentialBaseline>,
    last_good: Option<CachedGeminiProvider>,
}

fn runtime_state() -> &'static Mutex<GeminiRuntimeState> {
    static STATE: OnceLock<Mutex<GeminiRuntimeState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(GeminiRuntimeState::default()))
}

fn credential_baseline(snapshot: Option<&CredentialSnapshot>) -> CredentialBaseline {
    let Some(snapshot) = snapshot else {
        return CredentialBaseline::Missing;
    };
    let mut hasher = DefaultHasher::new();
    snapshot.access_token.hash(&mut hasher);
    snapshot.expiry.hash(&mut hasher);
    CredentialBaseline::Present(hasher.finish())
}

fn begin_auth_recovery(baseline: CredentialBaseline) {
    let mut state = runtime_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.auth_recovery_baseline = Some(baseline);
}

fn clear_auth_recovery() {
    let mut state = runtime_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.auth_recovery_baseline = None;
}

fn credential_change_allows_probe(
    baseline: CredentialBaseline,
    current: CredentialBaseline,
    current_usable: bool,
) -> bool {
    current_usable && matches!(current, CredentialBaseline::Present(_)) && current != baseline
}

fn auth_recovery_wait_detail(
    baseline: CredentialBaseline,
    current: CredentialBaseline,
    current_usable: bool,
    running: bool,
    retry_after: u64,
    now: u64,
) -> Option<String> {
    if credential_change_allows_probe(baseline, current, current_usable) {
        return None;
    }
    if running {
        return Some("Gemini CLI credential refresh already in progress".to_owned());
    }
    (retry_after > now).then(|| {
        format!(
            "Gemini CLI credential refresh cooling down; retry in about {}s",
            retry_after.saturating_sub(now).div_ceil(1000)
        )
    })
}

fn auth_recovery_fetch_guard(home: &Path, now: u64) -> Option<String> {
    let running = AUTH_REFRESH_RUNNING.load(Ordering::Acquire);
    let retry_after = AUTH_REFRESH_RETRY_AFTER_MS.load(Ordering::Acquire);
    if !running && retry_after <= now {
        return None;
    }
    let baseline = {
        let state = runtime_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.auth_recovery_baseline
    }?;
    let current_snapshot = read_credential_snapshot(home);
    let current = credential_baseline(current_snapshot.as_ref());
    let current_usable = current_snapshot
        .clone()
        .and_then(CredentialSnapshot::into_valid)
        .is_some();
    if credential_change_allows_probe(baseline, current, current_usable) {
        let mut state = runtime_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.auth_recovery_baseline = Some(current);
        drop(state);
        AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
        return None;
    }
    auth_recovery_wait_detail(baseline, current, current_usable, running, retry_after, now)
}

pub(crate) async fn enrich_quota_report(home: &Path, report: QuotaReport) -> QuotaReport {
    let fallback = report.clone();
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || enrich_quota_report_sync(&home, report))
        .await
        .unwrap_or(fallback)
}

fn enrich_quota_report_sync(home: &Path, mut report: QuotaReport) -> QuotaReport {
    let now = now_ms();
    if report
        .providers
        .iter()
        .find(|provider| provider.provider == SupportedProvider::Gemini)
        .is_some_and(provider_has_usable_quota)
    {
        cache_last_good(&report, now);
        clear_rate_limit_state();
        clear_auth_recovery();
        AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
        return report;
    }

    if let Some(remaining_ms) = active_cooldown_remaining_ms(now) {
        apply_failure_with_cache(
            &mut report,
            format!(
                "Gemini quota rate-limit cooldown; retry in about {}s",
                remaining_ms.div_ceil(1000)
            ),
            now,
        );
        return report;
    }

    if let Some(detail) = auth_recovery_fetch_guard(home, now) {
        apply_failure_with_cache(&mut report, detail, now);
        return report;
    }

    // Gemini quota remains API-backed. On missing/rejected OAuth credentials, Token Lens
    // only starts the official Gemini CLI and waits for the CLI to refresh its own credential;
    // it never sends a model prompt or scrapes `/stats` TUI output.
    let attempt = read_provider_with_reloaded_credential(home);
    match attempt.result {
        Ok(provider) => {
            apply_success(&mut report, provider, now);
            if let Some(trigger) = attempt.initial_unauthorized {
                record_incident(
                    trigger,
                    "recovered_credential_reread",
                    GeminiIncidentRecovery {
                        credential_reread: attempt.credential_reread,
                        credential_changed: attempt.credential_changed,
                        ..GeminiIncidentRecovery::default()
                    },
                );
            }
        }
        Err(DirectAttemptError::MissingCredential) => {
            return recover_auth_in_background(
                home,
                report,
                GeminiFailure::MissingCredential,
                false,
                None,
            );
        }
        Err(DirectAttemptError::Fetch(error)) if error.is_unauthorized() => {
            clear_rate_limit_state();
            return recover_auth_in_background(
                home,
                report,
                error,
                attempt.credential_reread,
                attempt.credential_changed,
            );
        }
        Err(DirectAttemptError::Fetch(error)) => {
            let cooldown_ms = if let Some(retry_after_ms) = error.retry_after_ms() {
                clear_auth_recovery();
                AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
                Some(set_rate_limit_cooldown(now, retry_after_ms))
            } else {
                None
            };
            let detail = match (&error, cooldown_ms) {
                (GeminiFailure::RateLimited { stage, .. }, Some(cooldown_ms)) => format!(
                    "Gemini {stage} rate limited (HTTP 429); retry in about {}s",
                    cooldown_ms.div_ceil(1000)
                ),
                _ => error.diagnostic(),
            };
            let presentation = apply_failure_with_cache(&mut report, detail, now);
            record_incident(
                error,
                presentation.result_label(),
                GeminiIncidentRecovery {
                    credential_reread: attempt.credential_reread,
                    credential_changed: attempt.credential_changed,
                    cooldown_ms,
                    last_good_used: presentation.last_good_used(),
                    ..GeminiIncidentRecovery::default()
                },
            );
        }
    }
    report
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GeminiFailure {
    MissingCredential,
    Unauthorized {
        stage: &'static str,
    },
    Forbidden {
        stage: &'static str,
    },
    RateLimited {
        stage: &'static str,
        retry_after_ms: u64,
    },
    Transport {
        stage: &'static str,
    },
    Http {
        stage: &'static str,
        status: i32,
    },
    InvalidPayload {
        stage: &'static str,
    },
    InvalidRequest {
        stage: &'static str,
    },
    NoProject,
    NoBuckets,
}

impl GeminiFailure {
    fn from_code_assist(stage: &'static str, error: CodeAssistError) -> Self {
        match error {
            CodeAssistError::Unauthorized => Self::Unauthorized { stage },
            CodeAssistError::Forbidden => Self::Forbidden { stage },
            CodeAssistError::RateLimited { retry_after_ms } => Self::RateLimited {
                stage,
                retry_after_ms,
            },
            CodeAssistError::Transport => Self::Transport { stage },
            CodeAssistError::Http(status) => Self::Http { stage, status },
            CodeAssistError::InvalidPayload => Self::InvalidPayload { stage },
            CodeAssistError::InvalidRequest => Self::InvalidRequest { stage },
        }
    }

    fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized { .. })
    }

    fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_ms, .. } => Some(*retry_after_ms),
            _ => None,
        }
    }

    fn diagnostic(&self) -> String {
        match self {
            Self::MissingCredential => {
                "Gemini credential: no readable non-expired access token".to_owned()
            }
            Self::Unauthorized { stage } => format!("Gemini {stage} returned HTTP 401"),
            Self::Forbidden { stage } => format!("Gemini {stage} returned HTTP 403"),
            Self::RateLimited {
                stage,
                retry_after_ms,
            } => format!(
                "Gemini {stage} rate limited (HTTP 429); retry in about {}s",
                retry_after_ms.div_ceil(1000)
            ),
            Self::Transport { stage } => format!("Gemini {stage} request failed"),
            Self::Http { stage, status } => format!("Gemini {stage} returned HTTP {status}"),
            Self::InvalidPayload { stage } => {
                format!("Gemini {stage} returned an invalid payload")
            }
            Self::InvalidRequest { stage } => {
                format!("Gemini {stage} request could not be encoded")
            }
            Self::NoProject => "Gemini loadCodeAssist returned no project".to_owned(),
            Self::NoBuckets => "Gemini retrieveUserQuota returned no model buckets".to_owned(),
        }
    }

    fn log_fields(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::MissingCredential => ("auth", "CREDENTIAL_UNAVAILABLE", "credential_discovery"),
            Self::Unauthorized { stage } => ("auth", "HTTP_401", stage),
            Self::Forbidden { stage } => ("auth", "HTTP_403", stage),
            Self::RateLimited { stage, .. } => ("rate_limit", "HTTP_429", stage),
            Self::Transport { stage } => ("transport", "TRANSPORT_ERROR", stage),
            Self::Http { stage, .. } => ("provider", "HTTP_ERROR", stage),
            Self::InvalidPayload { stage } => ("payload", "INVALID_PAYLOAD", stage),
            Self::InvalidRequest { stage } => ("request", "INVALID_REQUEST", stage),
            Self::NoProject => ("provider", "NO_PROJECT", "loadCodeAssist"),
            Self::NoBuckets => ("payload", "NO_QUOTA_WINDOWS", "retrieveUserQuota"),
        }
    }
}

struct DirectProviderAttempt {
    result: Result<QuotaProvider, DirectAttemptError>,
    credential_reread: bool,
    credential_changed: Option<bool>,
    initial_unauthorized: Option<GeminiFailure>,
}

enum DirectAttemptError {
    MissingCredential,
    Fetch(GeminiFailure),
}

fn read_provider_with_reloaded_credential(home: &Path) -> DirectProviderAttempt {
    read_provider_with_reloaded_credential_with(home, read_valid_credential, |credential| {
        fetch_provider(home, &credential.access_token)
    })
}

fn read_provider_with_reloaded_credential_with<R, F>(
    home: &Path,
    mut read_credential: R,
    mut fetch: F,
) -> DirectProviderAttempt
where
    R: FnMut(&Path) -> Option<ValidCredential>,
    F: FnMut(&ValidCredential) -> Result<QuotaProvider, GeminiFailure>,
{
    let Some(credential) = read_credential(home) else {
        return DirectProviderAttempt {
            result: Err(DirectAttemptError::MissingCredential),
            credential_reread: false,
            credential_changed: None,
            initial_unauthorized: None,
        };
    };
    match fetch(&credential) {
        Ok(provider) => DirectProviderAttempt {
            result: Ok(provider),
            credential_reread: false,
            credential_changed: None,
            initial_unauthorized: None,
        },
        Err(error) if error.is_unauthorized() => {
            let initial_unauthorized = Some(error.clone());
            let reloaded = read_credential(home);
            let changed = reloaded
                .as_ref()
                .is_some_and(|next| next.access_token != credential.access_token);
            let result = if changed {
                fetch(reloaded.as_ref().expect("changed credential must exist"))
                    .map_err(DirectAttemptError::Fetch)
            } else {
                Err(DirectAttemptError::Fetch(error))
            };
            DirectProviderAttempt {
                result,
                credential_reread: true,
                credential_changed: Some(changed),
                initial_unauthorized,
            }
        }
        Err(error) => DirectProviderAttempt {
            result: Err(DirectAttemptError::Fetch(error)),
            credential_reread: false,
            credential_changed: None,
            initial_unauthorized: None,
        },
    }
}

fn fetch_provider(home: &Path, access_token: &str) -> Result<QuotaProvider, GeminiFailure> {
    let requested_project = configured_project();
    let load =
        google_code_assist::load_code_assist_typed(access_token, requested_project.as_deref())
            .map_err(|error| GeminiFailure::from_code_assist("loadCodeAssist", error))?;
    let Some(project_id) = load.project_id.as_deref() else {
        // Token Lens is a monitor. Never call onboardUser to create or attach a project.
        return Err(GeminiFailure::NoProject);
    };
    let buckets = google_code_assist::retrieve_user_quota_typed(access_token, project_id)
        .map_err(|error| GeminiFailure::from_code_assist("retrieveUserQuota", error))?;
    let windows = normalize_buckets(buckets);
    if windows.is_empty() {
        return Err(GeminiFailure::NoBuckets);
    }
    Ok(QuotaProvider {
        provider: SupportedProvider::Gemini,
        plan: normalize_plan(load),
        account_email: read_active_account(home),
        diagnostic: None,
        windows,
        reset_credits: None,
        credit_status: None,
        spend_control: None,
    })
}

fn recover_auth_in_background(
    home: &Path,
    mut report: QuotaReport,
    failure: GeminiFailure,
    credential_reread: bool,
    credential_changed: Option<bool>,
) -> QuotaReport {
    let now = now_ms();
    let discovery_code = matches!(failure, GeminiFailure::MissingCredential)
        .then(|| credential_discovery_code(home));
    begin_auth_recovery(credential_baseline(read_credential_snapshot(home).as_ref()));
    let retry_after_ms = AUTH_REFRESH_RETRY_AFTER_MS.load(Ordering::Acquire);
    if retry_after_ms > now {
        let presentation = apply_failure_with_cache(
            &mut report,
            format!(
                "{}; Gemini CLI credential refresh cooling down; retry in about {}s",
                failure.diagnostic(),
                retry_after_ms.saturating_sub(now).div_ceil(1000)
            ),
            now,
        );
        record_incident(
            failure,
            presentation.result_label(),
            GeminiIncidentRecovery {
                credential_reread,
                credential_changed,
                cli_fallback: true,
                recovery_code: Some("CLI_REFRESH_COOLDOWN"),
                discovery_code,
                cli_source: None,
                cooldown_ms: None,
                last_good_used: presentation.last_good_used(),
            },
        );
        return report;
    }
    let home = home.to_path_buf();
    let failure_for_job = failure.clone();
    let started = crate::provider_cli_auth::spawn_refresh_once(&AUTH_REFRESH_RUNNING, move || {
        let refresh = gemini_cli::refresh_credential_via_startup(&home, || {
            read_credential_snapshot(&home).map(|credential| credential.signature())
        });
        let cli_source = refresh.cli_source;
        match refresh.result {
            Ok(()) => {
                AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
                clear_auth_recovery();
                record_incident(
                    failure_for_job,
                    "credential_change_observed",
                    GeminiIncidentRecovery {
                        credential_reread: true,
                        credential_changed: Some(true),
                        cli_fallback: true,
                        recovery_code: Some("CLI_CREDENTIAL_CHANGE_OBSERVED"),
                        discovery_code,
                        cli_source,
                        ..GeminiIncidentRecovery::default()
                    },
                );
            }
            Err(error) => {
                AUTH_REFRESH_RETRY_AFTER_MS.store(
                    now_ms().saturating_add(AUTH_REFRESH_FAILURE_COOLDOWN_MS),
                    Ordering::Release,
                );
                let (code, _) = gemini_cli::classify_refresh_error(&error);
                record_incident(
                    failure_for_job,
                    "cli_refresh_failed",
                    GeminiIncidentRecovery {
                        credential_reread,
                        credential_changed: credential_changed.or(Some(false)),
                        cli_fallback: true,
                        recovery_code: Some(code),
                        discovery_code,
                        cli_source,
                        ..GeminiIncidentRecovery::default()
                    },
                );
            }
        }
    });

    let detail = if started {
        format!(
            "{}; Gemini CLI credential refresh started in background",
            failure.diagnostic()
        )
    } else {
        format!(
            "{}; Gemini CLI credential refresh already in progress",
            failure.diagnostic()
        )
    };
    let presentation = apply_failure_with_cache(&mut report, detail, now);
    record_incident(
        failure,
        presentation.result_label(),
        GeminiIncidentRecovery {
            credential_reread,
            credential_changed,
            cli_fallback: true,
            recovery_code: Some(if started {
                "CLI_REFRESH_STARTED"
            } else {
                "CLI_REFRESH_IN_PROGRESS"
            }),
            discovery_code,
            cli_source: None,
            cooldown_ms: None,
            last_good_used: presentation.last_good_used(),
        },
    );
    report
}

fn apply_success(report: &mut QuotaReport, provider: QuotaProvider, now: u64) {
    if let Some(existing) = report
        .providers
        .iter_mut()
        .find(|item| item.provider == SupportedProvider::Gemini)
    {
        *existing = provider;
    } else {
        report.providers.push(provider);
    }
    clear_rate_limit_state();
    clear_auth_recovery();
    AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
    cache_last_good(report, now);
}

fn provider_has_usable_quota(provider: &QuotaProvider) -> bool {
    provider.windows.iter().any(|window| {
        window.remaining_percent.is_some()
            || window.used_percent.is_some()
            || window.remaining.is_some()
            || window.used.is_some()
    })
}

fn set_diagnostic(report: &mut QuotaReport, detail: impl Into<String>) {
    let detail = detail.into();
    if let Some(provider) = report
        .providers
        .iter_mut()
        .find(|provider| provider.provider == SupportedProvider::Gemini)
    {
        provider.diagnostic = Some(detail);
        return;
    }
    report.providers.push(QuotaProvider {
        provider: SupportedProvider::Gemini,
        plan: None,
        account_email: None,
        diagnostic: Some(detail),
        windows: Vec::new(),
        reset_credits: None,
        credit_status: None,
        spend_control: None,
    });
}

fn cache_last_good(report: &QuotaReport, captured_at_ms: u64) {
    let Some(provider) = report.providers.iter().find(|provider| {
        provider.provider == SupportedProvider::Gemini && provider_has_usable_quota(provider)
    }) else {
        return;
    };
    let mut provider = provider.clone();
    provider.diagnostic = None;
    let mut state = runtime_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.last_good = Some(CachedGeminiProvider {
        captured_at_ms,
        provider,
    });
}

fn active_cooldown_remaining_ms(now: u64) -> Option<u64> {
    let state = runtime_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (state.cooldown_until_ms > now).then_some(state.cooldown_until_ms - now)
}

fn rate_limit_backoff_floor_ms(streak: u32) -> u64 {
    let index = streak
        .saturating_sub(1)
        .min((RATE_LIMIT_BACKOFF_STEPS_MS.len() - 1) as u32) as usize;
    RATE_LIMIT_BACKOFF_STEPS_MS[index]
}

fn effective_rate_limit_cooldown_ms(
    streak: u32,
    retry_after_ms: u64,
    previous_cooldown_ms: u64,
) -> u64 {
    retry_after_ms
        .clamp(1_000, MAX_RATE_LIMIT_COOLDOWN_MS)
        .max(rate_limit_backoff_floor_ms(streak))
        .max(previous_cooldown_ms.min(MAX_RATE_LIMIT_COOLDOWN_MS))
}

fn set_rate_limit_cooldown(now: u64, retry_after_ms: u64) -> u64 {
    let mut state = runtime_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.rate_limit_streak = state.rate_limit_streak.saturating_add(1);
    let cooldown_ms = effective_rate_limit_cooldown_ms(
        state.rate_limit_streak,
        retry_after_ms,
        state.last_rate_limit_cooldown_ms,
    );
    state.last_rate_limit_cooldown_ms = cooldown_ms;
    state.cooldown_until_ms = state.cooldown_until_ms.max(now.saturating_add(cooldown_ms));
    cooldown_ms
}

fn clear_rate_limit_state() {
    let mut state = runtime_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.cooldown_until_ms = 0;
    state.rate_limit_streak = 0;
    state.last_rate_limit_cooldown_ms = 0;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailurePresentation {
    Stale,
    Unavailable,
}

impl FailurePresentation {
    fn result_label(self) -> &'static str {
        match self {
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
        }
    }

    fn last_good_used(self) -> bool {
        self == Self::Stale
    }
}

fn apply_failure_with_cache(
    report: &mut QuotaReport,
    detail: impl Into<String>,
    now: u64,
) -> FailurePresentation {
    let detail = detail.into();
    let cached = {
        let state = runtime_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.last_good.clone()
    };
    let Some(mut cached) =
        cached.filter(|cached| now.saturating_sub(cached.captured_at_ms) <= LAST_GOOD_TTL_MS)
    else {
        set_diagnostic(report, detail);
        return FailurePresentation::Unavailable;
    };
    cached
        .provider
        .windows
        .retain(|window| window_not_expired(window, now));
    if !provider_has_usable_quota(&cached.provider) {
        set_diagnostic(report, detail);
        return FailurePresentation::Unavailable;
    }
    let stale = format!("{STALE_DIAGNOSTIC_PREFIX} · {detail}");
    cached.provider.diagnostic = Some(stale);
    if let Some(existing) = report
        .providers
        .iter_mut()
        .find(|provider| provider.provider == SupportedProvider::Gemini)
    {
        *existing = cached.provider;
    } else {
        report.providers.push(cached.provider);
    }
    FailurePresentation::Stale
}

fn window_not_expired(window: &QuotaWindow, now: u64) -> bool {
    let Some(reset) = window.resets_at.as_deref() else {
        return true;
    };
    DateTime::parse_from_rfc3339(reset)
        .map(|value| value.timestamp_millis().max(0) as u64 > now)
        .unwrap_or(true)
}

#[derive(Debug, Default)]
struct GeminiIncidentRecovery {
    credential_reread: bool,
    credential_changed: Option<bool>,
    cli_fallback: bool,
    recovery_code: Option<&'static str>,
    discovery_code: Option<&'static str>,
    cli_source: Option<&'static str>,
    cooldown_ms: Option<u64>,
    last_good_used: bool,
}

fn record_incident(failure: GeminiFailure, result: &'static str, recovery: GeminiIncidentRecovery) {
    let retry_after_ms = failure.retry_after_ms();
    let (category, code, stage) = failure.log_fields();
    provider_error_log::record(ProviderIncident {
        provider: "gemini",
        category,
        code,
        stage,
        result,
        credential_reread: recovery.credential_reread.then_some(true),
        credential_changed: recovery.credential_changed,
        cli_fallback: recovery.cli_fallback.then_some(true),
        recovery_code: recovery.recovery_code,
        discovery_code: recovery.discovery_code,
        cli_source: recovery.cli_source,
        retry_after_seconds: retry_after_ms.map(|value| value.div_ceil(1000)),
        cooldown_seconds: recovery.cooldown_ms.map(|value| value.div_ceil(1000)),
        last_good_used: recovery.last_good_used.then_some(true),
    });
}

struct ValidCredential {
    access_token: String,
}

#[derive(Debug, Clone)]
struct CredentialSnapshot {
    access_token: String,
    expiry: Option<u64>,
}

impl CredentialSnapshot {
    fn signature(&self) -> String {
        format!("{}:{}", self.access_token, self.expiry.unwrap_or_default())
    }

    fn into_valid(self) -> Option<ValidCredential> {
        if self
            .expiry
            .is_some_and(|value| value <= now_ms().saturating_add(TOKEN_EXPIRY_SAFETY_MS))
        {
            return None;
        }
        Some(ValidCredential {
            access_token: self.access_token,
        })
    }
}

fn credential_snapshot(
    access_token: Option<String>,
    expiry: Option<u64>,
) -> Option<CredentialSnapshot> {
    let access_token = access_token?.trim().to_owned();
    (!access_token.is_empty()).then_some(CredentialSnapshot {
        access_token,
        expiry,
    })
}

fn read_credential_snapshot(home: &Path) -> Option<CredentialSnapshot> {
    #[cfg(target_os = "windows")]
    {
        if let Some(credential) = read_windows_keychain_snapshot() {
            return Some(credential);
        }
        if let Some(credential) = read_windows_file_keychain_snapshot(home) {
            return Some(credential);
        }
    }

    let path = gemini_home(home).join("oauth_creds.json");
    let bytes = fs::read(path).ok()?;
    let raw = serde_json::from_slice::<StoredGeminiCredential>(&bytes).ok()?;
    credential_snapshot(raw.access_token, raw.expiry_date)
}

fn read_valid_credential(home: &Path) -> Option<ValidCredential> {
    read_credential_snapshot(home)?.into_valid()
}

fn credential_discovery_code(home: &Path) -> &'static str {
    #[cfg(target_os = "windows")]
    {
        if let Some(bytes) = read_windows_keychain_blob() {
            return if parse_keychain_snapshot(&bytes).is_some() {
                "WINDOWS_CREDENTIAL_BECAME_AVAILABLE_DURING_DISCOVERY"
            } else {
                "WINDOWS_CREDENTIAL_PARSE_FAILED"
            };
        }
        let file_keychain = gemini_home(home).join("gemini-credentials.json");
        if file_keychain.exists() {
            let Ok(text) = fs::read_to_string(file_keychain) else {
                return "FILE_KEYCHAIN_READ_FAILED";
            };
            let Some(hostname) = env::var("COMPUTERNAME").ok() else {
                return "FILE_KEYCHAIN_ENV_UNAVAILABLE";
            };
            let Some(username) = env::var("USERNAME").ok().or_else(|| env::var("USER").ok()) else {
                return "FILE_KEYCHAIN_ENV_UNAVAILABLE";
            };
            return if parse_file_keychain_snapshot(&text, &hostname, &username).is_some() {
                "FILE_KEYCHAIN_BECAME_AVAILABLE_DURING_DISCOVERY"
            } else {
                "FILE_KEYCHAIN_PARSE_FAILED"
            };
        }
    }

    let oauth_file = gemini_home(home).join("oauth_creds.json");
    if oauth_file.exists() {
        let Ok(bytes) = fs::read(oauth_file) else {
            return "OAUTH_FILE_READ_FAILED";
        };
        let parsed = serde_json::from_slice::<StoredGeminiCredential>(&bytes)
            .ok()
            .and_then(|raw| credential_snapshot(raw.access_token, raw.expiry_date));
        return if parsed.is_some() {
            "OAUTH_FILE_BECAME_AVAILABLE_DURING_DISCOVERY"
        } else {
            "OAUTH_FILE_PARSE_FAILED"
        };
    }
    "STORE_NOT_FOUND"
}

#[cfg(any(target_os = "windows", test))]
fn parse_keychain_snapshot(bytes: &[u8]) -> Option<CredentialSnapshot> {
    let raw = serde_json::from_slice::<StoredGeminiKeychainCredential>(bytes).ok()?;
    let token = raw.token?;
    credential_snapshot(token.access_token, token.expires_at)
}

#[cfg(test)]
fn parse_keychain_credential(bytes: &[u8]) -> Option<ValidCredential> {
    parse_keychain_snapshot(bytes)?.into_valid()
}

#[cfg(target_os = "windows")]
fn read_windows_keychain_snapshot() -> Option<CredentialSnapshot> {
    read_windows_keychain_blob().and_then(|bytes| parse_keychain_snapshot(&bytes))
}

#[cfg(target_os = "windows")]
fn read_windows_keychain_blob() -> Option<Vec<u8>> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target = OsStr::new("gemini-cli-oauth/main-account")
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let mut credential: *mut CREDENTIALW = ptr::null_mut();
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } != 0;
    if !ok || credential.is_null() {
        return None;
    }
    let bytes = unsafe {
        let item = &*credential;
        std::slice::from_raw_parts(item.CredentialBlob, item.CredentialBlobSize as usize).to_vec()
    };
    unsafe { CredFree(credential.cast()) };
    Some(bytes)
}

#[cfg(target_os = "windows")]
fn read_windows_file_keychain_snapshot(home: &Path) -> Option<CredentialSnapshot> {
    let hostname = env::var("COMPUTERNAME").ok()?;
    let username = env::var("USERNAME")
        .ok()
        .or_else(|| env::var("USER").ok())?;
    let text = fs::read_to_string(gemini_home(home).join("gemini-credentials.json")).ok()?;
    parse_file_keychain_snapshot(&text, &hostname, &username)
}

#[cfg(any(target_os = "windows", test))]
fn parse_file_keychain_snapshot(
    encrypted: &str,
    hostname: &str,
    username: &str,
) -> Option<CredentialSnapshot> {
    let plaintext = decrypt_file_keychain(encrypted, hostname, username)?;
    let store = serde_json::from_slice::<serde_json::Value>(&plaintext).ok()?;
    let secret = store
        .get("gemini-cli-oauth")?
        .get("main-account")?
        .as_str()?;
    parse_keychain_snapshot(secret.as_bytes())
}

#[cfg(test)]
fn parse_file_keychain_credential(
    encrypted: &str,
    hostname: &str,
    username: &str,
) -> Option<ValidCredential> {
    parse_file_keychain_snapshot(encrypted, hostname, username)?.into_valid()
}

#[cfg(any(target_os = "windows", test))]
fn decrypt_file_keychain(encrypted: &str, hostname: &str, username: &str) -> Option<Vec<u8>> {
    type Aes256Gcm16 = AesGcm<Aes256, U16>;
    let mut parts = encrypted.trim().split(':');
    let iv = decode_hex(parts.next()?)?;
    let tag = decode_hex(parts.next()?)?;
    let mut ciphertext = decode_hex(parts.next()?)?;
    if parts.next().is_some() || iv.len() != 16 || tag.len() != 16 {
        return None;
    }

    let salt = format!("{hostname}-{username}-gemini-cli");
    let params = ScryptParams::new(14, 8, 1, 32).ok()?;
    let mut key = [0u8; 32];
    scrypt(b"gemini-cli-oauth", salt.as_bytes(), &params, &mut key).ok()?;
    let cipher = Aes256Gcm16::new_from_slice(&key).ok()?;
    cipher
        .decrypt_in_place_detached(
            Nonce::<U16>::from_slice(&iv),
            b"",
            &mut ciphertext,
            Tag::from_slice(&tag),
        )
        .ok()?;
    Some(ciphertext)
}

#[cfg(any(target_os = "windows", test))]
fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

#[cfg(any(target_os = "windows", test))]
fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn read_active_account(home: &Path) -> Option<String> {
    let path = gemini_home(home).join("google_accounts.json");
    let raw = serde_json::from_slice::<StoredGeminiAccounts>(&fs::read(path).ok()?).ok()?;
    let active = raw.active?.trim().to_ascii_lowercase();
    (!active.is_empty()).then_some(active)
}

fn gemini_home(home: &Path) -> PathBuf {
    env::var_os("GEMINI_CLI_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".gemini"))
}

fn configured_project() -> Option<String> {
    env::var("GOOGLE_CLOUD_PROJECT")
        .ok()
        .or_else(|| env::var("GOOGLE_CLOUD_PROJECT_ID").ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_plan(load: LoadSnapshot) -> Option<String> {
    load.plan
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_buckets(buckets: Vec<QuotaBucket>) -> Vec<QuotaWindow> {
    let mut by_model: HashMap<String, QuotaWindow> = HashMap::new();
    for bucket in buckets {
        let Some(model_id) = bucket
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(fraction) = bucket.remaining_fraction.filter(|value| value.is_finite()) else {
            continue;
        };
        let fraction = fraction.clamp(0.0, 1.0);
        let remaining_percent = fraction * 100.0;
        let used_percent = (100.0 - remaining_percent).clamp(0.0, 100.0);
        let remaining = bucket
            .remaining_amount
            .as_deref()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0);
        let limit =
            remaining.and_then(|value| (fraction > 0.0).then_some((value / fraction).round()));
        let used = limit
            .zip(remaining)
            .map(|(limit, remaining)| (limit - remaining).max(0.0));
        let window = QuotaWindow {
            kind: QuotaWindowKind::Other,
            label: model_id.to_owned(),
            metric: "quota",
            additional: true,
            used,
            limit,
            remaining,
            used_percent: Some(used_percent),
            remaining_percent: Some(remaining_percent),
            remaining_label: None,
            resets_at: bucket.reset_time.and_then(clean_string),
            currency: None,
            show_meter: true,
            source: SOURCE,
        };
        let replace = by_model
            .get(model_id)
            .and_then(|current| current.remaining_percent)
            .map_or(true, |current| remaining_percent < current);
        if replace {
            by_model.insert(model_id.to_owned(), window);
        }
    }
    let mut windows = by_model.into_values().collect::<Vec<_>>();
    windows.sort_by(|a, b| {
        a.remaining_percent
            .partial_cmp(&b.remaining_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
    windows
}

fn clean_string(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
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

    static RUNTIME_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn bucket(model: &str, fraction: f64, amount: Option<&str>) -> QuotaBucket {
        QuotaBucket {
            model_id: Some(model.into()),
            remaining_fraction: Some(fraction),
            remaining_amount: amount.map(str::to_owned),
            reset_time: Some("2026-09-06T00:00:00Z".into()),
        }
    }

    #[test]
    fn model_buckets_are_allowlist_free_and_keep_most_constrained_duplicate() {
        let windows = normalize_buckets(vec![
            bucket("gemini-future-pro", 0.8, None),
            bucket("gemini-future-pro", 0.4, None),
            bucket("gemini-new-flash", 0.9, None),
        ]);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "gemini-future-pro");
        assert_eq!(windows[0].remaining_percent, Some(40.0));
        assert!(windows.iter().all(|window| window.additional));
    }

    #[test]
    fn remaining_amount_is_preserved_without_inventing_a_currency() {
        let [window] = normalize_buckets(vec![bucket("gemini-pro", 0.25, Some("250"))])
            .try_into()
            .expect("one window");
        assert_eq!(window.remaining, Some(250.0));
        assert_eq!(window.limit, Some(1000.0));
        assert_eq!(window.used, Some(750.0));
        assert_eq!(window.currency, None);
    }

    fn provider_fixture(reset_time: &str) -> QuotaProvider {
        QuotaProvider {
            provider: SupportedProvider::Gemini,
            plan: Some("Test".into()),
            account_email: None,
            diagnostic: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Other,
                label: "gemini-test".into(),
                metric: "quota",
                additional: true,
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some(25.0),
                remaining_percent: Some(75.0),
                remaining_label: None,
                resets_at: Some(reset_time.into()),
                currency: None,
                show_meter: true,
                source: SOURCE,
            }],
            reset_credits: None,
            credit_status: None,
            spend_control: None,
        }
    }

    #[test]
    fn retries_401_once_when_provider_rotated_the_gemini_access_token() {
        let mut credentials = vec![
            ValidCredential {
                access_token: "token-a".into(),
            },
            ValidCredential {
                access_token: "token-b".into(),
            },
        ]
        .into_iter();
        let mut fetched = Vec::new();
        let attempt = read_provider_with_reloaded_credential_with(
            Path::new("/unused"),
            |_| credentials.next(),
            |credential| {
                fetched.push(credential.access_token.clone());
                if credential.access_token == "token-a" {
                    Err(GeminiFailure::Unauthorized {
                        stage: "retrieveUserQuota",
                    })
                } else {
                    Ok(provider_fixture("2099-01-01T00:00:00Z"))
                }
            },
        );
        assert!(attempt.result.is_ok());
        assert!(attempt.credential_reread);
        assert_eq!(attempt.credential_changed, Some(true));
        assert_eq!(fetched, vec!["token-a", "token-b"]);
    }

    #[test]
    fn does_not_retry_401_when_gemini_credential_is_unchanged() {
        let mut credentials = vec![
            ValidCredential {
                access_token: "token-a".into(),
            },
            ValidCredential {
                access_token: "token-a".into(),
            },
        ]
        .into_iter();
        let mut fetch_count = 0;
        let attempt = read_provider_with_reloaded_credential_with(
            Path::new("/unused"),
            |_| credentials.next(),
            |_| {
                fetch_count += 1;
                Err(GeminiFailure::Unauthorized {
                    stage: "loadCodeAssist",
                })
            },
        );
        assert!(matches!(
            attempt.result,
            Err(DirectAttemptError::Fetch(
                GeminiFailure::Unauthorized { .. }
            ))
        ));
        assert!(attempt.credential_reread);
        assert_eq!(attempt.credential_changed, Some(false));
        assert_eq!(fetch_count, 1);
    }

    #[test]
    fn changed_gemini_token_is_retried_only_once_even_if_it_is_also_rejected() {
        let mut credentials = vec![
            ValidCredential {
                access_token: "token-a".into(),
            },
            ValidCredential {
                access_token: "token-b".into(),
            },
            ValidCredential {
                access_token: "token-c".into(),
            },
        ]
        .into_iter();
        let mut fetch_count = 0;
        let attempt = read_provider_with_reloaded_credential_with(
            Path::new("/unused"),
            |_| credentials.next(),
            |_| {
                fetch_count += 1;
                Err(GeminiFailure::Unauthorized {
                    stage: "retrieveUserQuota",
                })
            },
        );
        assert!(attempt.result.is_err());
        assert_eq!(attempt.credential_changed, Some(true));
        assert_eq!(fetch_count, 2);
    }

    #[test]
    fn gemini_rate_limit_uses_sticky_adaptive_provider_cooldown() {
        let _guard = RUNTIME_TEST_LOCK.lock().unwrap();
        clear_rate_limit_state();
        assert_eq!(set_rate_limit_cooldown(1_000, 1_000), 5 * 60 * 1000);
        assert_eq!(
            active_cooldown_remaining_ms(2_000),
            Some(5 * 60 * 1000 - 1_000)
        );
        clear_rate_limit_state();
        assert_eq!(
            set_rate_limit_cooldown(1_000, 60 * 60 * 1000),
            60 * 60 * 1000
        );
        assert_eq!(
            set_rate_limit_cooldown(60 * 60 * 1000 + 2_000, 1_000),
            60 * 60 * 1000
        );
        clear_rate_limit_state();
    }

    #[test]
    fn gemini_auth_recovery_probe_requires_a_usable_credential_change() {
        let missing = CredentialBaseline::Missing;
        let token_a = credential_baseline(Some(&CredentialSnapshot {
            access_token: "token-a".to_owned(),
            expiry: Some(u64::MAX),
        }));
        let token_b = credential_baseline(Some(&CredentialSnapshot {
            access_token: "token-b".to_owned(),
            expiry: Some(u64::MAX),
        }));
        assert!(!credential_change_allows_probe(missing, missing, false));
        assert!(credential_change_allows_probe(missing, token_a, true));
        assert!(!credential_change_allows_probe(token_a, token_a, true));
        assert!(credential_change_allows_probe(token_a, token_b, true));
        assert!(!credential_change_allows_probe(token_a, token_b, false));
    }

    #[test]
    fn gemini_auth_recovery_waits_without_reprobing_an_unchanged_credential() {
        let token_a = credential_baseline(Some(&CredentialSnapshot {
            access_token: "token-a".to_owned(),
            expiry: Some(u64::MAX),
        }));
        let token_b = credential_baseline(Some(&CredentialSnapshot {
            access_token: "token-b".to_owned(),
            expiry: Some(u64::MAX),
        }));
        assert!(auth_recovery_wait_detail(token_a, token_a, true, true, 0, 1_000).is_some());
        assert!(auth_recovery_wait_detail(token_a, token_a, true, false, 301_000, 1_000).is_some());
        assert!(auth_recovery_wait_detail(token_a, token_b, true, true, 301_000, 1_000).is_none());
        assert!(
            auth_recovery_wait_detail(token_a, token_b, false, false, 301_000, 1_000).is_some()
        );
    }

    #[test]
    fn gemini_repeated_429_backoff_does_not_collapse_to_tiny_retry_after_values() {
        assert_eq!(effective_rate_limit_cooldown_ms(1, 1_000, 0), 5 * 60 * 1000);
        assert_eq!(
            effective_rate_limit_cooldown_ms(2, 1_000, 5 * 60 * 1000),
            15 * 60 * 1000
        );
        assert_eq!(
            effective_rate_limit_cooldown_ms(3, 1_000, 15 * 60 * 1000),
            30 * 60 * 1000
        );
        assert_eq!(
            effective_rate_limit_cooldown_ms(4, 1_000, 30 * 60 * 1000),
            60 * 60 * 1000
        );
        assert_eq!(
            effective_rate_limit_cooldown_ms(2, 1_000, 60 * 60 * 1000),
            60 * 60 * 1000
        );
    }

    #[test]
    fn gemini_last_good_is_stale_then_expires_and_reset_windows_are_filtered() {
        let _guard = RUNTIME_TEST_LOCK.lock().unwrap();
        {
            let mut state = runtime_state().lock().unwrap();
            *state = GeminiRuntimeState {
                cooldown_until_ms: 0,
                last_good: Some(CachedGeminiProvider {
                    captured_at_ms: 1_000,
                    provider: provider_fixture("2099-01-01T00:00:00Z"),
                }),
                ..GeminiRuntimeState::default()
            };
        }
        let mut report = QuotaReport {
            generated_at_ms: 2_000,
            providers: Vec::new(),
            source: "test",
        };
        let presentation = apply_failure_with_cache(&mut report, "temporary failure", 2_000);
        assert_eq!(presentation, FailurePresentation::Stale);
        assert!(report.providers[0]
            .diagnostic
            .as_deref()
            .is_some_and(|value| value.starts_with(STALE_DIAGNOSTIC_PREFIX)));

        let mut expired = QuotaReport {
            generated_at_ms: LAST_GOOD_TTL_MS + 2_000,
            providers: Vec::new(),
            source: "test",
        };
        let presentation =
            apply_failure_with_cache(&mut expired, "temporary failure", LAST_GOOD_TTL_MS + 2_001);
        assert_eq!(presentation, FailurePresentation::Unavailable);
        assert!(!window_not_expired(
            &provider_fixture("2000-01-01T00:00:00Z").windows[0],
            now_ms()
        ));
        let mut state = runtime_state().lock().unwrap();
        *state = GeminiRuntimeState::default();
    }

    #[test]
    fn gemini_failure_codes_distinguish_401_403_and_429() {
        assert_eq!(
            GeminiFailure::from_code_assist("loadCodeAssist", CodeAssistError::Unauthorized)
                .log_fields(),
            ("auth", "HTTP_401", "loadCodeAssist")
        );
        assert_eq!(
            GeminiFailure::from_code_assist("loadCodeAssist", CodeAssistError::Forbidden)
                .log_fields(),
            ("auth", "HTTP_403", "loadCodeAssist")
        );
        assert_eq!(
            GeminiFailure::from_code_assist(
                "retrieveUserQuota",
                CodeAssistError::RateLimited {
                    retry_after_ms: 60_000,
                }
            )
            .log_fields(),
            ("rate_limit", "HTTP_429", "retrieveUserQuota")
        );
    }

    #[test]
    fn existing_tokscale_gemini_quota_remains_authoritative() {
        let provider = QuotaProvider {
            provider: SupportedProvider::Gemini,
            plan: Some("Future".into()),
            account_email: None,
            diagnostic: None,
            windows: vec![QuotaWindow {
                kind: QuotaWindowKind::Daily,
                label: "Daily".into(),
                metric: "quota",
                additional: false,
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some(10.0),
                remaining_percent: Some(90.0),
                remaining_label: None,
                resets_at: None,
                currency: None,
                show_meter: true,
                source: "tokscale",
            }],
            reset_credits: None,
            credit_status: None,
            spend_control: None,
        };
        assert!(provider_has_usable_quota(&provider));
    }

    #[tokio::test]
    #[ignore = "requires a local Gemini CLI credential; live API runs only while its access token is already valid"]
    async fn live_gemini_quota_smoke() {
        let home = env::var_os("HOME").map(PathBuf::from).expect("HOME");
        let Some(credential) = read_valid_credential(&home) else {
            return;
        };
        let load = google_code_assist::load_code_assist(
            &credential.access_token,
            configured_project().as_deref(),
        )
        .expect("load live Gemini Code Assist state");
        let Some(project) = load.project_id.as_deref() else {
            return;
        };
        let windows = normalize_buckets(
            google_code_assist::retrieve_user_quota(&credential.access_token, project)
                .expect("read live Gemini quota"),
        );
        assert!(!windows.is_empty(), "expected live Gemini quota buckets");
        assert!(windows.iter().all(|window| window.source == SOURCE));
    }

    #[test]
    fn current_gemini_keychain_shape_reads_only_access_token_and_expiry() {
        let future = now_ms() + 60_000;
        let fixture = serde_json::json!({
            "serverName": "main-account",
            "token": {
                "accessToken": "keychain-access",
                "refreshToken": "DO_NOT_READ_OR_USE",
                "tokenType": "Bearer",
                "expiresAt": future
            },
            "updatedAt": now_ms()
        });
        let bytes = serde_json::to_vec(&fixture).expect("keychain fixture");
        let parsed = parse_keychain_credential(&bytes).expect("valid current credential");
        assert_eq!(parsed.access_token, "keychain-access");
    }

    #[test]
    fn encrypted_file_keychain_matches_gemini_cli_node_crypto_format() {
        let fixture = "000102030405060708090a0b0c0d0e0f:4cce9d5c5d3fdf79b6d3b1034e119489:434b76d776b0e43555058fd747d6d635c228ccf9a0744385826972d4160076ee21c3c84fe1dca16614c0e5efb7e20428329a400d289b7e679483a07452ff96c1b16825655477f940a77b40b3177ae6424b45c25314706bc8288c19da4ac5ff53bc421b36ac7562458482d9a83520908139c74a87e54d2b2edc6b800de14dd863ea8dc4749232cfb7b818d25eabe3774350719b6d573412ec1ae06cc54817920e";
        let parsed = parse_file_keychain_credential(fixture, "TEST-HOST", "tester")
            .expect("Gemini CLI encrypted file credential should decrypt");
        assert_eq!(parsed.access_token, "test-access-token");
    }

    #[test]
    fn credential_without_expiry_can_be_validated_by_the_read_only_api_call() {
        let parsed = credential_snapshot(Some("existing-access-token".into()), None)
            .and_then(CredentialSnapshot::into_valid)
            .expect("unknown expiry should not discard an existing access token");
        assert_eq!(parsed.access_token, "existing-access-token");
    }

    #[test]
    fn credential_signature_detects_expiry_only_refreshes() {
        let before = credential_snapshot(Some("same-access-token".into()), Some(1))
            .expect("stored credential");
        let after = credential_snapshot(Some("same-access-token".into()), Some(2))
            .expect("stored credential");
        assert_ne!(before.signature(), after.signature());
        assert!(before.into_valid().is_none());
    }

    #[test]
    fn expired_credentials_are_rejected_without_refreshing_or_deserializing_refresh_token() {
        let raw = serde_json::from_str::<StoredGeminiCredential>(
            r#"{"access_token":"token","expiry_date":1,"refresh_token":"DO_NOT_USE"}"#,
        )
        .expect("credential fixture");
        assert_eq!(raw.access_token.as_deref(), Some("token"));
        assert_eq!(raw.expiry_date, Some(1));
    }
}
