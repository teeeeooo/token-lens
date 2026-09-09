use crate::claude_cli;
use crate::domain::{QuotaProvider, QuotaReport, QuotaWindow, QuotaWindowKind, SupportedProvider};
use crate::provider_error_log::{self, ProviderIncident};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;
use std::env;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE: &str = "claude-oauth-usage";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const HTTP_TIMEOUT_SECONDS: u64 = 12;
const DEFAULT_RATE_LIMIT_COOLDOWN_MS: u64 = 60_000;
const MAX_RATE_LIMIT_COOLDOWN_MS: u64 = 60 * 60 * 1000;
const RATE_LIMIT_BACKOFF_STEPS_MS: [u64; 4] = [
    5 * 60 * 1000,
    15 * 60 * 1000,
    30 * 60 * 1000,
    60 * 60 * 1000,
];
const LAST_GOOD_TTL_MS: u64 = 30 * 60 * 1000;
const STALE_DIAGNOSTIC_PREFIX: &str = "Stale Claude quota";
const AUTH_REFRESH_FAILURE_COOLDOWN_MS: u64 = 5 * 60 * 1000;
static AUTH_REFRESH_RUNNING: AtomicBool = AtomicBool::new(false);
static AUTH_REFRESH_RETRY_AFTER_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
enum UsageFetchError {
    Unauthorized,
    RateLimited { retry_after_ms: u64 },
    Other(String),
}

impl UsageFetchError {
    fn diagnostic(&self) -> String {
        match self {
            Self::Unauthorized => "Claude usage returned HTTP 401".to_owned(),
            Self::RateLimited { retry_after_ms } => format!(
                "Claude usage rate limited (HTTP 429); retry in about {}s",
                retry_after_ms.div_ceil(1000)
            ),
            Self::Other(detail) => detail.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct CachedClaudeProvider {
    captured_at_ms: u64,
    provider: QuotaProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CredentialBaseline {
    Missing,
    Present(u64),
}

#[derive(Debug, Default)]
struct ClaudeRuntimeState {
    cooldown_until_ms: u64,
    rate_limit_streak: u32,
    last_rate_limit_cooldown_ms: u64,
    auth_recovery_baseline: Option<CredentialBaseline>,
    last_good: Option<CachedClaudeProvider>,
}

fn runtime_state() -> &'static Mutex<ClaudeRuntimeState> {
    static STATE: OnceLock<Mutex<ClaudeRuntimeState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ClaudeRuntimeState::default()))
}

fn credential_baseline(token: Option<&str>) -> CredentialBaseline {
    let Some(token) = token else {
        return CredentialBaseline::Missing;
    };
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
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
) -> bool {
    matches!(current, CredentialBaseline::Present(_)) && current != baseline
}

fn auth_recovery_wait_detail(
    baseline: CredentialBaseline,
    current: CredentialBaseline,
    running: bool,
    retry_after: u64,
    now: u64,
) -> Option<String> {
    if credential_change_allows_probe(baseline, current) {
        return None;
    }
    if running {
        return Some("Claude CLI credential refresh already in progress".to_owned());
    }
    (retry_after > now).then(|| {
        format!(
            "Claude CLI credential refresh cooling down; retry in about {}s",
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
    let current_token = read_access_token(home);
    let current = credential_baseline(current_token.as_deref());
    if credential_change_allows_probe(baseline, current) {
        let mut state = runtime_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.auth_recovery_baseline = Some(current);
        drop(state);
        AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
        return None;
    }
    auth_recovery_wait_detail(baseline, current, running, retry_after, now)
}

pub(crate) async fn enrich_quota_report(home: &Path, report: QuotaReport) -> QuotaReport {
    let fallback = report.clone();
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || enrich_quota_report_sync(&home, report))
        .await
        .unwrap_or(fallback)
}

fn enrich_quota_report_sync(home: &Path, mut report: QuotaReport) -> QuotaReport {
    let needs = report
        .providers
        .iter()
        .find(|provider| provider.provider == SupportedProvider::Claude)
        .map_or(true, provider_needs_enrichment);
    if !needs {
        cache_last_good(&report, now_ms());
        clear_rate_limit_state();
        clear_auth_recovery();
        AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
        return report;
    }

    let now = now_ms();
    if let Some(remaining_ms) = active_cooldown_remaining_ms(now) {
        apply_failure_with_cache(
            &mut report,
            format!(
                "Claude usage rate-limit cooldown; retry in about {}s",
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

    let direct = read_usage_with_reloaded_credential(home);
    match direct.result {
        Ok(usage) => {
            clear_rate_limit_state();
            let windows = windows_from_usage(&usage);
            if !windows.is_empty() {
                apply_success(&mut report, windows, now);
                if direct.credential_reread {
                    record_incident(
                        RecoveryTrigger::Unauthorized,
                        "recovered_credential_reread",
                        IncidentRecovery {
                            credential_reread: true,
                            credential_changed: direct.credential_changed,
                            ..IncidentRecovery::default()
                        },
                    );
                }
                return report;
            }
            let trigger = if direct.credential_reread {
                RecoveryTrigger::Unauthorized
            } else {
                RecoveryTrigger::NoQuotaWindows
            };
            let presentation = apply_failure_with_cache(&mut report, trigger.diagnostic(), now);
            record_incident(
                trigger,
                presentation.result_label(),
                IncidentRecovery {
                    credential_reread: direct.credential_reread,
                    credential_changed: direct.credential_changed,
                    last_good_used: presentation.last_good_used(),
                    ..IncidentRecovery::default()
                },
            );
            report
        }
        Err(DirectAttemptError::MissingCredential) => recover_auth_in_background(
            home,
            report,
            RecoveryTrigger::MissingCredential,
            false,
            None,
            direct.credential_baseline,
        ),
        Err(DirectAttemptError::Fetch(UsageFetchError::RateLimited { retry_after_ms })) => {
            clear_auth_recovery();
            AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
            let cooldown_ms = set_rate_limit_cooldown(now, retry_after_ms);
            let presentation = apply_failure_with_cache(
                &mut report,
                format!(
                    "Claude usage rate limited (HTTP 429); retry in about {}s",
                    cooldown_ms.div_ceil(1000)
                ),
                now,
            );
            record_incident(
                RecoveryTrigger::RateLimited,
                presentation.result_label(),
                IncidentRecovery {
                    credential_reread: direct.credential_reread,
                    credential_changed: direct.credential_changed,
                    retry_after_ms: Some(retry_after_ms),
                    cooldown_ms: Some(cooldown_ms),
                    last_good_used: presentation.last_good_used(),
                    ..IncidentRecovery::default()
                },
            );
            report
        }
        Err(DirectAttemptError::Fetch(error)) => {
            let trigger = RecoveryTrigger::from_fetch_error(&error);
            if matches!(error, UsageFetchError::Unauthorized) {
                clear_rate_limit_state();
                recover_auth_in_background(
                    home,
                    report,
                    trigger,
                    direct.credential_reread,
                    direct.credential_changed,
                    direct.credential_baseline,
                )
            } else {
                let presentation = apply_failure_with_cache(&mut report, error.diagnostic(), now);
                record_incident(
                    trigger,
                    presentation.result_label(),
                    IncidentRecovery {
                        credential_reread: direct.credential_reread,
                        credential_changed: direct.credential_changed,
                        last_good_used: presentation.last_good_used(),
                        ..IncidentRecovery::default()
                    },
                );
                report
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RecoveryTrigger {
    Unauthorized,
    RateLimited,
    MissingCredential,
    NoQuotaWindows,
    Transport,
    InvalidPayload,
    HttpError,
}

impl RecoveryTrigger {
    fn from_fetch_error(error: &UsageFetchError) -> Self {
        match error {
            UsageFetchError::Unauthorized => Self::Unauthorized,
            UsageFetchError::RateLimited { .. } => Self::RateLimited,
            UsageFetchError::Other(detail) if detail.contains("invalid payload") => {
                Self::InvalidPayload
            }
            UsageFetchError::Other(detail) if detail.contains("request failed") => Self::Transport,
            UsageFetchError::Other(_) => Self::HttpError,
        }
    }

    fn log_fields(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Unauthorized => ("auth", "HTTP_401", "oauth_usage"),
            Self::RateLimited => ("rate_limit", "HTTP_429", "oauth_usage"),
            Self::MissingCredential => ("auth", "CREDENTIAL_UNAVAILABLE", "credential_discovery"),
            Self::NoQuotaWindows => ("payload", "NO_QUOTA_WINDOWS", "oauth_usage"),
            Self::Transport => ("transport", "TRANSPORT_ERROR", "oauth_usage"),
            Self::InvalidPayload => ("payload", "INVALID_PAYLOAD", "oauth_usage"),
            Self::HttpError => ("provider", "HTTP_ERROR", "oauth_usage"),
        }
    }

    fn diagnostic(self) -> &'static str {
        match self {
            Self::Unauthorized => "Claude authentication was rejected",
            Self::RateLimited => "Claude usage is rate limited",
            Self::MissingCredential => "Claude credential is unavailable",
            Self::NoQuotaWindows => "Claude usage API returned no supported quota windows",
            Self::Transport => "Claude usage request failed",
            Self::InvalidPayload => "Claude usage API returned an invalid payload",
            Self::HttpError => "Claude usage API returned an error",
        }
    }
}

struct DirectUsageAttempt {
    result: Result<Value, DirectAttemptError>,
    credential_reread: bool,
    credential_changed: Option<bool>,
    credential_baseline: CredentialBaseline,
}

enum DirectAttemptError {
    MissingCredential,
    Fetch(UsageFetchError),
}

fn read_usage_with_reloaded_credential(home: &Path) -> DirectUsageAttempt {
    read_usage_with_reloaded_credential_with(home, read_access_token, fetch_usage)
}

fn read_usage_with_reloaded_credential_with<R, F>(
    home: &Path,
    mut read_credential: R,
    mut fetch: F,
) -> DirectUsageAttempt
where
    R: FnMut(&Path) -> Option<String>,
    F: FnMut(&str) -> Result<Value, UsageFetchError>,
{
    let Some(access_token) = read_credential(home) else {
        return DirectUsageAttempt {
            result: Err(DirectAttemptError::MissingCredential),
            credential_reread: false,
            credential_changed: None,
            credential_baseline: CredentialBaseline::Missing,
        };
    };
    let initial_baseline = credential_baseline(Some(&access_token));
    match fetch(&access_token) {
        Ok(usage) => DirectUsageAttempt {
            result: Ok(usage),
            credential_reread: false,
            credential_changed: None,
            credential_baseline: initial_baseline,
        },
        Err(UsageFetchError::Unauthorized) => {
            let reloaded = read_credential(home);
            let changed = reloaded
                .as_ref()
                .is_some_and(|token| token != &access_token);
            let result = if changed {
                fetch(reloaded.as_deref().expect("changed credential must exist"))
                    .map_err(DirectAttemptError::Fetch)
            } else {
                Err(DirectAttemptError::Fetch(UsageFetchError::Unauthorized))
            };
            let baseline = reloaded
                .as_deref()
                .map_or(CredentialBaseline::Missing, |token| {
                    credential_baseline(Some(token))
                });
            DirectUsageAttempt {
                result,
                credential_reread: true,
                credential_changed: Some(changed),
                credential_baseline: baseline,
            }
        }
        Err(error) => DirectUsageAttempt {
            result: Err(DirectAttemptError::Fetch(error)),
            credential_reread: false,
            credential_changed: None,
            credential_baseline: initial_baseline,
        },
    }
}

fn recover_auth_in_background(
    home: &Path,
    mut report: QuotaReport,
    trigger: RecoveryTrigger,
    credential_reread: bool,
    credential_changed: Option<bool>,
    credential_baseline: CredentialBaseline,
) -> QuotaReport {
    let now = now_ms();
    let discovery_code = matches!(trigger, RecoveryTrigger::MissingCredential)
        .then(|| credential_discovery_code(home));
    if environment_access_token().is_some() {
        let presentation = apply_failure_with_cache(
            &mut report,
            format!(
                "{}; CLAUDE_CODE_OAUTH_TOKEN cannot be refreshed by a child Claude CLI process",
                trigger.diagnostic()
            ),
            now,
        );
        record_incident(
            trigger,
            presentation.result_label(),
            IncidentRecovery {
                credential_reread,
                credential_changed,
                recovery_code: Some("CLI_ENV_CREDENTIAL_IMMUTABLE"),
                discovery_code,
                last_good_used: presentation.last_good_used(),
                ..IncidentRecovery::default()
            },
        );
        return report;
    }

    begin_auth_recovery(credential_baseline);
    let retry_after_ms = AUTH_REFRESH_RETRY_AFTER_MS.load(Ordering::Acquire);
    if retry_after_ms > now {
        let presentation = apply_failure_with_cache(
            &mut report,
            format!(
                "{}; Claude CLI credential refresh cooling down; retry in about {}s",
                trigger.diagnostic(),
                retry_after_ms.saturating_sub(now).div_ceil(1000)
            ),
            now,
        );
        record_incident(
            trigger,
            presentation.result_label(),
            IncidentRecovery {
                credential_reread,
                credential_changed,
                cli_fallback: true,
                recovery_code: Some("CLI_REFRESH_COOLDOWN"),
                discovery_code,
                last_good_used: presentation.last_good_used(),
                ..IncidentRecovery::default()
            },
        );
        return report;
    }

    let home = home.to_path_buf();
    let started = crate::provider_cli_auth::spawn_refresh_once(&AUTH_REFRESH_RUNNING, move || {
        let refresh =
            claude_cli::refresh_credential_via_startup(&home, || read_access_token(&home));
        match refresh {
            Ok(()) => {
                AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
                clear_auth_recovery();
                record_incident(
                    trigger,
                    "cli_refresh_completed",
                    IncidentRecovery {
                        credential_reread: true,
                        credential_changed: Some(true),
                        cli_fallback: true,
                        discovery_code,
                        ..IncidentRecovery::default()
                    },
                );
            }
            Err(error) => {
                AUTH_REFRESH_RETRY_AFTER_MS.store(
                    now_ms().saturating_add(AUTH_REFRESH_FAILURE_COOLDOWN_MS),
                    Ordering::Release,
                );
                let (code, _) = claude_cli::classify_refresh_error(&error);
                record_incident(
                    trigger,
                    "cli_refresh_failed",
                    IncidentRecovery {
                        credential_reread,
                        credential_changed: credential_changed.or(Some(false)),
                        cli_fallback: true,
                        recovery_code: Some(code),
                        discovery_code,
                        ..IncidentRecovery::default()
                    },
                );
            }
        }
    });

    let detail = if started {
        format!(
            "{}; Claude CLI credential refresh started in background",
            trigger.diagnostic()
        )
    } else {
        format!(
            "{}; Claude CLI credential refresh already in progress",
            trigger.diagnostic()
        )
    };
    let presentation = apply_failure_with_cache(&mut report, detail, now);
    record_incident(
        trigger,
        presentation.result_label(),
        IncidentRecovery {
            credential_reread,
            credential_changed,
            cli_fallback: true,
            recovery_code: Some(if started {
                "CLI_REFRESH_STARTED"
            } else {
                "CLI_REFRESH_IN_PROGRESS"
            }),
            discovery_code,
            last_good_used: presentation.last_good_used(),
            ..IncidentRecovery::default()
        },
    );
    report
}

fn apply_success(report: &mut QuotaReport, windows: Vec<QuotaWindow>, now: u64) {
    if let Some(provider) = report
        .providers
        .iter_mut()
        .find(|provider| provider.provider == SupportedProvider::Claude)
    {
        merge_windows(provider, windows);
        provider.diagnostic = None;
    } else {
        report.providers.push(QuotaProvider {
            provider: SupportedProvider::Claude,
            plan: None,
            account_email: None,
            diagnostic: None,
            windows,
            reset_credits: None,
            credit_status: None,
            spend_control: None,
        });
    }
    clear_rate_limit_state();
    clear_auth_recovery();
    AUTH_REFRESH_RETRY_AFTER_MS.store(0, Ordering::Release);
    cache_last_good(report, now);
}

fn set_diagnostic(report: &mut QuotaReport, detail: impl Into<String>) {
    let detail = detail.into();
    if let Some(provider) = report
        .providers
        .iter_mut()
        .find(|provider| provider.provider == SupportedProvider::Claude)
    {
        provider.diagnostic = Some(detail);
        return;
    }
    report.providers.push(QuotaProvider {
        provider: SupportedProvider::Claude,
        plan: None,
        account_email: None,
        diagnostic: Some(detail),
        windows: Vec::new(),
        reset_credits: None,
        credit_status: None,
        spend_control: None,
    });
}

fn provider_needs_enrichment(provider: &QuotaProvider) -> bool {
    let has_spend = provider
        .windows
        .iter()
        .any(|window| window.metric == "spend");
    let has_base = provider.windows.iter().any(|window| {
        !window.additional
            && matches!(
                window.kind,
                QuotaWindowKind::Session | QuotaWindowKind::Weekly
            )
            && window.remaining_percent.is_some()
    });
    !has_spend || !has_base
}

fn provider_has_usable_windows(provider: &QuotaProvider) -> bool {
    provider.windows.iter().any(|window| {
        window.remaining_percent.is_some()
            || window.used_percent.is_some()
            || window.remaining.is_some()
            || window.used.is_some()
    })
}

fn merge_windows(provider: &mut QuotaProvider, incoming: Vec<QuotaWindow>) {
    for window in incoming {
        let duplicate = if window.metric == "spend" {
            provider
                .windows
                .iter()
                .any(|existing| existing.metric == "spend")
        } else {
            provider.windows.iter().any(|existing| {
                !existing.additional
                    && existing.kind == window.kind
                    && existing.remaining_percent.is_some()
            })
        };
        if !duplicate {
            provider.windows.push(window);
        }
    }
    provider.windows.sort_by_key(window_rank);
}

fn window_rank(window: &QuotaWindow) -> u8 {
    match window.kind {
        QuotaWindowKind::Session => 0,
        QuotaWindowKind::Daily => 1,
        QuotaWindowKind::Weekly => 2,
        QuotaWindowKind::Billing => 3,
        QuotaWindowKind::Other => 4,
    }
}

fn cache_last_good(report: &QuotaReport, captured_at_ms: u64) {
    let Some(provider) = report.providers.iter().find(|provider| {
        provider.provider == SupportedProvider::Claude && provider_has_usable_windows(provider)
    }) else {
        return;
    };
    let mut provider = provider.clone();
    provider.diagnostic = None;
    let mut state = runtime_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.last_good = Some(CachedClaudeProvider {
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
    Current,
    Stale,
    Unavailable,
}

impl FailurePresentation {
    fn result_label(self) -> &'static str {
        match self {
            Self::Current => "degraded_current",
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
    if report
        .providers
        .iter()
        .find(|provider| provider.provider == SupportedProvider::Claude)
        .is_some_and(provider_has_usable_windows)
    {
        set_diagnostic(report, detail);
        return FailurePresentation::Current;
    }

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
    if !provider_has_usable_windows(&cached.provider) {
        set_diagnostic(report, detail);
        return FailurePresentation::Unavailable;
    }

    let stale = format!("{STALE_DIAGNOSTIC_PREFIX} · {detail}");
    cached.provider.diagnostic = Some(stale.clone());
    if let Some(provider) = report
        .providers
        .iter_mut()
        .find(|provider| provider.provider == SupportedProvider::Claude)
    {
        merge_windows(provider, cached.provider.windows);
        if provider.plan.is_none() {
            provider.plan = cached.provider.plan;
        }
        if provider.account_email.is_none() {
            provider.account_email = cached.provider.account_email;
        }
        provider.diagnostic = Some(stale);
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
struct IncidentRecovery {
    credential_reread: bool,
    credential_changed: Option<bool>,
    cli_fallback: bool,
    recovery_code: Option<&'static str>,
    discovery_code: Option<&'static str>,
    retry_after_ms: Option<u64>,
    cooldown_ms: Option<u64>,
    last_good_used: bool,
}

fn record_incident(trigger: RecoveryTrigger, result: &'static str, recovery: IncidentRecovery) {
    let (category, code, stage) = trigger.log_fields();
    provider_error_log::record(ProviderIncident {
        provider: "claude",
        category,
        code,
        stage,
        result,
        credential_reread: recovery.credential_reread.then_some(true),
        credential_changed: recovery.credential_changed,
        cli_fallback: recovery.cli_fallback.then_some(true),
        recovery_code: recovery.recovery_code,
        discovery_code: recovery.discovery_code,
        cli_source: None,
        retry_after_seconds: recovery.retry_after_ms.map(|value| value.div_ceil(1000)),
        cooldown_seconds: recovery.cooldown_ms.map(|value| value.div_ceil(1000)),
        last_good_used: recovery.last_good_used.then_some(true),
    });
}

fn fetch_usage(access_token: &str) -> Result<Value, UsageFetchError> {
    let token = access_token.trim();
    if token.is_empty() {
        return Err(UsageFetchError::Other(
            "Claude access token is unavailable".to_owned(),
        ));
    }
    let response = minreq::get(USAGE_URL)
        .with_header("accept", "application/json")
        .with_header("authorization", format!("Bearer {token}"))
        .with_header("anthropic-beta", "oauth-2025-04-20")
        .with_header("user-agent", "Token-Lens/2")
        .with_timeout(HTTP_TIMEOUT_SECONDS)
        .with_follow_redirects(false)
        .send()
        .map_err(|error| {
            UsageFetchError::Other(format!(
                "Claude usage request failed ({})",
                crate::http_diagnostic::transport_category(&error)
            ))
        })?;
    match response.status_code {
        200..=299 => {}
        401 => return Err(UsageFetchError::Unauthorized),
        429 => {
            return Err(UsageFetchError::RateLimited {
                retry_after_ms: retry_after_ms(&response),
            })
        }
        status => {
            return Err(UsageFetchError::Other(format!(
                "Claude usage returned HTTP {status}"
            )))
        }
    }
    response
        .json::<Value>()
        .map_err(|_| UsageFetchError::Other("Claude usage returned an invalid payload".to_owned()))
}

fn retry_after_ms(response: &minreq::Response) -> u64 {
    retry_after_header_ms(
        response.headers.get("retry-after").map(String::as_str),
        DateTime::<Utc>::from(SystemTime::now()),
    )
}

fn retry_after_header_ms(raw: Option<&str>, now: DateTime<Utc>) -> u64 {
    let Some(raw) = raw else {
        return DEFAULT_RATE_LIMIT_COOLDOWN_MS;
    };
    if let Ok(seconds) = raw.trim().parse::<u64>() {
        return seconds
            .saturating_mul(1000)
            .clamp(1_000, MAX_RATE_LIMIT_COOLDOWN_MS);
    }
    let when = DateTime::parse_from_rfc2822(raw.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(raw.trim(), "%a, %d %b %Y %H:%M:%S GMT")
                .ok()
                .map(|value| value.and_utc())
        });
    if let Some(when) = when {
        let millis = when.signed_duration_since(now).num_milliseconds();
        if millis > 0 {
            return (millis as u64).clamp(1_000, MAX_RATE_LIMIT_COOLDOWN_MS);
        }
    }
    DEFAULT_RATE_LIMIT_COOLDOWN_MS
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn windows_from_usage(usage: &Value) -> Vec<QuotaWindow> {
    let mut windows = Vec::new();
    if let Some(window) = usage_window(
        usage,
        &["five_hour", "fiveHour"],
        QuotaWindowKind::Session,
        "5h",
    ) {
        windows.push(window);
    }
    if let Some(window) = usage_window(
        usage,
        &["seven_day", "sevenDay"],
        QuotaWindowKind::Weekly,
        "Weekly",
    ) {
        windows.push(window);
    }
    if let Some(window) = usage_credits_window(usage) {
        windows.push(window);
    }
    windows
}

fn usage_window(
    usage: &Value,
    aliases: &[&str],
    kind: QuotaWindowKind,
    label: &str,
) -> Option<QuotaWindow> {
    let raw = value_from_aliases(usage, aliases)?;
    let used_percent = number_from_aliases(
        raw,
        &["usedPercent", "used_percent", "utilization", "percent"],
    )?
    .clamp(0.0, 100.0);
    Some(QuotaWindow {
        kind,
        label: label.to_owned(),
        metric: "quota",
        additional: false,
        used: None,
        limit: None,
        remaining: None,
        used_percent: Some(used_percent),
        remaining_percent: Some(100.0 - used_percent),
        remaining_label: None,
        resets_at: scalar_string(value_from_aliases(raw, &["resets_at", "resetsAt"])),
        currency: None,
        show_meter: true,
        source: SOURCE,
    })
}

fn usage_credits_window(usage: &Value) -> Option<QuotaWindow> {
    let spend = value_from_aliases(usage, &["spend"]);
    let extra = value_from_aliases(usage, &["extra_usage", "extraUsage"]);
    let enabled = spend
        .and_then(|value| value.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true)
        || extra
            .and_then(|value| value_from_aliases(value, &["is_enabled", "isEnabled"]))
            .and_then(Value::as_bool)
            == Some(true);
    if !enabled {
        return None;
    }

    let spend_used = spend
        .and_then(|value| value.get("used"))
        .and_then(spend_money);
    let spend_limit = spend
        .and_then(|value| value.get("limit"))
        .and_then(spend_money);
    let used = spend_used
        .as_ref()
        .map(|money| money.0)
        .or_else(|| extra.and_then(|value| extra_usage_money(value, "used_credits")))?;
    let limit = spend_limit
        .as_ref()
        .map(|money| money.0)
        .or_else(|| extra.and_then(|value| extra_usage_money(value, "monthly_limit")));
    let currency = spend_used
        .as_ref()
        .and_then(|money| money.1.clone())
        .or_else(|| extra.and_then(|value| scalar_string(value.get("currency"))))
        .unwrap_or_else(|| "USD".to_owned())
        .to_ascii_uppercase();
    let remaining = limit.map(|limit| (limit - used).max(0.0));
    let used_percent = limit
        .filter(|limit| *limit > 0.0)
        .map(|limit| (used / limit * 100.0).clamp(0.0, 100.0));
    let remaining_percent = used_percent.map(|value| 100.0 - value);

    Some(QuotaWindow {
        kind: QuotaWindowKind::Billing,
        label: "Usage credits".to_owned(),
        metric: "spend",
        additional: false,
        used: Some(used),
        limit,
        remaining,
        used_percent,
        remaining_percent,
        remaining_label: None,
        resets_at: None,
        currency: Some(currency),
        show_meter: limit.is_some(),
        source: SOURCE,
    })
}

fn spend_money(value: &Value) -> Option<(f64, Option<String>)> {
    let minor = number_from_aliases(value, &["amount_minor", "amountMinor"])?;
    let exponent = number_from_aliases(value, &["exponent"]).unwrap_or(2.0);
    if exponent < 0.0 || exponent > 12.0 {
        return None;
    }
    let amount = minor / 10f64.powf(exponent);
    let currency = scalar_string(value_from_aliases(value, &["currency"]))
        .map(|value| value.to_ascii_uppercase());
    Some((amount, currency))
}

fn extra_usage_money(extra: &Value, key: &str) -> Option<f64> {
    let raw = number_from_aliases(extra, &[key])?;
    let places = number_from_aliases(extra, &["decimal_places", "decimalPlaces"])
        .unwrap_or(2.0)
        .clamp(0.0, 12.0);
    Some(raw / 10f64.powf(places))
}

fn number_from_aliases(value: &Value, aliases: &[&str]) -> Option<f64> {
    let value = value_from_aliases(value, aliases)?;
    match value {
        Value::Number(number) => number.as_f64().filter(|value| value.is_finite()),
        Value::String(value) => value.parse::<f64>().ok().filter(|value| value.is_finite()),
        _ => None,
    }
}

fn value_from_aliases<'a>(value: &'a Value, aliases: &[&str]) -> Option<&'a Value> {
    aliases
        .iter()
        .find_map(|key| value.get(*key))
        .filter(|value| !value.is_null())
}

fn scalar_string(value: Option<&Value>) -> Option<String> {
    let value = match value? {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => return None,
    };
    (!value.is_empty()).then_some(value)
}

fn environment_access_token() -> Option<String> {
    env::var("CLAUDE_CODE_OAUTH_TOKEN")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_access_token(home: &Path) -> Option<String> {
    if let Some(value) = environment_access_token() {
        return Some(value);
    }
    for path in claude_credential_paths(home) {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if let Some(token) = read_token_json(&bytes) {
            return Some(token);
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(token) = read_windows_credential_token() {
        return Some(token);
    }
    None
}

fn credential_discovery_code(home: &Path) -> &'static str {
    for path in claude_credential_paths(home) {
        if !path.exists() {
            continue;
        }
        let Ok(bytes) = fs::read(path) else {
            return "FILE_READ_FAILED";
        };
        if read_token_json(&bytes).is_none() {
            return "FILE_PARSE_FAILED";
        }
        return "FILE_CHANGED_DURING_DISCOVERY";
    }
    #[cfg(target_os = "windows")]
    if let Some(bytes) = read_windows_credential_blob() {
        let parsed = decode_credential_blob(&bytes)
            .and_then(|text| read_token_json(text.as_bytes()))
            .is_some();
        return if parsed {
            "WINDOWS_CREDENTIAL_CHANGED_DURING_DISCOVERY"
        } else {
            "WINDOWS_CREDENTIAL_PARSE_FAILED"
        };
    }
    "STORE_NOT_FOUND"
}

fn claude_credential_paths(home: &Path) -> Vec<PathBuf> {
    if let Some(root) = env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return vec![root.join(".credentials.json")];
    }

    let native = home.join(".claude/.credentials.json");
    #[cfg(not(target_os = "windows"))]
    {
        vec![native]
    }
    #[cfg(target_os = "windows")]
    {
        let mut candidates = vec![native];
        candidates.extend(wsl_claude_credential_paths());
        candidates.sort_by_key(|path| {
            std::cmp::Reverse(
                fs::metadata(path)
                    .and_then(|metadata| metadata.modified())
                    .ok(),
            )
        });
        candidates
    }
}

#[cfg(target_os = "windows")]
fn wsl_claude_credential_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let root = PathBuf::from(r"\\wsl$");
    let Ok(distros) = fs::read_dir(root) else {
        return paths;
    };
    for distro in distros.flatten() {
        let name = distro.file_name();
        let name = name.to_string_lossy();
        if name.is_empty() || name.starts_with('.') || name.contains('$') {
            continue;
        }
        let home = distro.path().join("home");
        let Ok(users) = fs::read_dir(home) else {
            continue;
        };
        for user in users.flatten() {
            paths.push(user.path().join(".claude/.credentials.json"));
        }
    }
    paths
}

fn read_token_json(bytes: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(bytes).ok()?;
    let oauth = value
        .get("claudeAiOauth")
        .or_else(|| value.get("oauth"))
        .unwrap_or(&value);
    scalar_string(value_from_aliases(oauth, &["accessToken", "access_token"]))
}

#[cfg(target_os = "windows")]
fn read_windows_credential_token() -> Option<String> {
    read_windows_credential_blob().and_then(|bytes| {
        decode_credential_blob(&bytes).and_then(|text| read_token_json(text.as_bytes()))
    })
}

#[cfg(target_os = "windows")]
fn read_windows_credential_blob() -> Option<Vec<u8>> {
    use std::ffi::OsStr;
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let mut targets = vec!["Claude Code-credentials".to_owned()];
    for key in ["USER", "USERNAME"] {
        if let Ok(user) = env::var(key) {
            let user = user.trim();
            if !user.is_empty() {
                targets.push(format!("Claude Code-credentials:{user}"));
                targets.push(format!("Claude Code-credentials/{user}"));
            }
        }
    }
    for target in targets {
        let wide = OsStr::new(&target)
            .encode_wide()
            .chain(once(0))
            .collect::<Vec<_>>();
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        let ok = unsafe { CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } != 0;
        if !ok || credential.is_null() {
            continue;
        }
        let bytes = unsafe {
            let item = &*credential;
            std::slice::from_raw_parts(item.CredentialBlob, item.CredentialBlobSize as usize)
                .to_vec()
        };
        unsafe { CredFree(credential.cast()) };
        return Some(bytes);
    }
    None
}

#[cfg(target_os = "windows")]
fn decode_credential_blob(bytes: &[u8]) -> Option<String> {
    let utf8 = String::from_utf8_lossy(bytes)
        .trim_matches('\0')
        .trim()
        .to_owned();
    if utf8.starts_with('{') || utf8.contains("accessToken") {
        return Some(utf8);
    }
    if bytes.len() % 2 == 0 {
        let units = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        let utf16 = String::from_utf16_lossy(&units)
            .trim_matches('\0')
            .trim()
            .to_owned();
        if utf16.starts_with('{') || utf16.contains("accessToken") {
            return Some(utf16);
        }
    }
    (!utf8.is_empty()).then_some(utf8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "requires a local Claude OAuth access token and live usage endpoint"]
    fn live_claude_oauth_usage_smoke() {
        let home = env::var_os("HOME").map(PathBuf::from).expect("HOME");
        let token = read_access_token(&home).expect("local Claude access token");
        let usage = fetch_usage(&token).expect("read live Claude usage");
        assert!(!windows_from_usage(&usage).is_empty());
    }

    #[test]
    fn retries_401_once_when_provider_has_rotated_the_access_token() {
        let home = Path::new("/tmp/token-lens-test-home");
        let mut reads = vec!["token-a".to_owned(), "token-b".to_owned()].into_iter();
        let mut fetched = Vec::new();
        let attempt = read_usage_with_reloaded_credential_with(
            home,
            |_| reads.next(),
            |token| {
                fetched.push(token.to_owned());
                if token == "token-a" {
                    Err(UsageFetchError::Unauthorized)
                } else {
                    Ok(json!({"five_hour":{"utilization":20}}))
                }
            },
        );
        let usage = match attempt.result {
            Ok(usage) => usage,
            Err(_) => panic!("rotated provider credential should recover the request"),
        };
        assert!(attempt.credential_reread);
        assert_eq!(attempt.credential_changed, Some(true));
        assert_eq!(fetched, vec!["token-a", "token-b"]);
        assert_eq!(windows_from_usage(&usage)[0].remaining_percent, Some(80.0));
    }

    #[test]
    fn does_not_retry_401_when_provider_credential_is_unchanged() {
        let home = Path::new("/tmp/token-lens-test-home");
        let mut fetch_count = 0;
        let attempt = read_usage_with_reloaded_credential_with(
            home,
            |_| Some("same-token".to_owned()),
            |_| {
                fetch_count += 1;
                Err(UsageFetchError::Unauthorized)
            },
        );
        assert_eq!(fetch_count, 1);
        assert!(attempt.credential_reread);
        assert_eq!(attempt.credential_changed, Some(false));
        assert!(matches!(
            attempt.result,
            Err(DirectAttemptError::Fetch(UsageFetchError::Unauthorized))
        ));
    }

    #[test]
    fn classifies_429_without_retrying_the_same_refresh_cycle() {
        let home = Path::new("/tmp/token-lens-test-home");
        let mut fetch_count = 0;
        let attempt = read_usage_with_reloaded_credential_with(
            home,
            |_| Some("token-a".to_owned()),
            |_| {
                fetch_count += 1;
                Err(UsageFetchError::RateLimited {
                    retry_after_ms: 120_000,
                })
            },
        );
        assert_eq!(fetch_count, 1);
        assert!(matches!(
            attempt.result,
            Err(DirectAttemptError::Fetch(UsageFetchError::RateLimited {
                retry_after_ms: 120_000
            }))
        ));
    }

    #[test]
    fn auth_recovery_probe_requires_a_usable_credential_change() {
        let missing = CredentialBaseline::Missing;
        let token_a = credential_baseline(Some("token-a"));
        let token_b = credential_baseline(Some("token-b"));
        assert!(!credential_change_allows_probe(missing, missing));
        assert!(credential_change_allows_probe(missing, token_a));
        assert!(!credential_change_allows_probe(token_a, token_a));
        assert!(credential_change_allows_probe(token_a, token_b));
        assert!(!credential_change_allows_probe(token_a, missing));
    }

    #[test]
    fn auth_recovery_waits_without_reprobing_an_unchanged_credential() {
        let token_a = credential_baseline(Some("token-a"));
        let token_b = credential_baseline(Some("token-b"));
        assert!(auth_recovery_wait_detail(token_a, token_a, true, 0, 1_000).is_some());
        assert!(auth_recovery_wait_detail(token_a, token_a, false, 301_000, 1_000).is_some());
        assert!(auth_recovery_wait_detail(token_a, token_b, true, 301_000, 1_000).is_none());
        assert!(auth_recovery_wait_detail(
            CredentialBaseline::Missing,
            CredentialBaseline::Missing,
            false,
            301_000,
            1_000,
        )
        .is_some());
    }

    #[test]
    fn repeated_429_backoff_does_not_collapse_to_tiny_retry_after_values() {
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
            effective_rate_limit_cooldown_ms(9, 1_000, 60 * 60 * 1000),
            60 * 60 * 1000
        );
        assert_eq!(
            effective_rate_limit_cooldown_ms(1, 60 * 60 * 1000, 0),
            60 * 60 * 1000
        );
        assert_eq!(
            effective_rate_limit_cooldown_ms(2, 1_000, 60 * 60 * 1000),
            60 * 60 * 1000
        );
    }

    #[test]
    fn retry_after_seconds_and_http_date_are_bounded_and_parsed() {
        let now = DateTime::parse_from_rfc3339("2026-09-07T13:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(retry_after_header_ms(Some("120"), now), 120_000);
        assert_eq!(
            retry_after_header_ms(Some("Mon, 07 Sep 2026 13:02:00 GMT"), now),
            120_000
        );
        assert_eq!(
            retry_after_header_ms(None, now),
            DEFAULT_RATE_LIMIT_COOLDOWN_MS
        );
    }

    #[test]
    fn stale_windows_are_not_reused_past_their_reset_time() {
        let mut window = QuotaWindow {
            kind: QuotaWindowKind::Session,
            label: "5h".to_owned(),
            metric: "quota",
            additional: false,
            used: None,
            limit: None,
            remaining: None,
            used_percent: Some(20.0),
            remaining_percent: Some(80.0),
            remaining_label: None,
            resets_at: Some("2026-09-07T13:00:00Z".to_owned()),
            currency: None,
            show_meter: true,
            source: SOURCE,
        };
        let now = DateTime::parse_from_rfc3339("2026-09-07T13:00:01Z")
            .unwrap()
            .timestamp_millis() as u64;
        assert!(!window_not_expired(&window, now));
        window.resets_at = Some("2026-09-07T13:30:00Z".to_owned());
        assert!(window_not_expired(&window, now));
    }

    #[test]
    fn parses_usage_credits_from_self_describing_spend_shape() {
        let usage = json!({
            "spend": {
                "enabled": true,
                "used": { "amount_minor": 23500, "currency": "usd", "exponent": 2 },
                "limit": { "amount_minor": 200000, "currency": "usd", "exponent": 2 }
            }
        });
        let [window] = windows_from_usage(&usage)
            .try_into()
            .expect("one billing window");
        assert_eq!(window.metric, "spend");
        assert_eq!(window.label, "Usage credits");
        assert_eq!(window.used, Some(235.0));
        assert_eq!(window.limit, Some(2000.0));
        assert_eq!(window.remaining, Some(1765.0));
        assert_eq!(window.currency.as_deref(), Some("USD"));
        assert_eq!(window.remaining_percent, Some(88.25));
    }

    #[test]
    fn parses_extra_usage_alias_without_inventing_a_disabled_credit_window() {
        let enabled = json!({
            "extra_usage": {
                "is_enabled": true,
                "used_credits": 23500,
                "monthly_limit": 200000,
                "decimal_places": 2,
                "currency": "USD"
            }
        });
        let [window] = windows_from_usage(&enabled)
            .try_into()
            .expect("one billing window");
        assert_eq!(window.used, Some(235.0));
        assert_eq!(window.limit, Some(2000.0));

        let disabled =
            json!({"extra_usage":{"is_enabled":false,"used_credits":0,"monthly_limit":200000}});
        assert!(windows_from_usage(&disabled).is_empty());
    }

    #[test]
    fn fills_missing_base_quota_but_does_not_replace_tokscale_windows() {
        let usage = json!({
            "five_hour": { "utilization": 25, "resets_at": "2026-09-08T01:00:00Z" },
            "seven_day": { "utilization": 40, "resets_at": "2026-09-12T01:00:00Z" }
        });
        let windows = windows_from_usage(&usage);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].remaining_percent, Some(75.0));
        assert_eq!(windows[1].remaining_percent, Some(60.0));
    }

    #[test]
    fn credential_parser_reads_only_access_token_from_current_file_shapes() {
        let nested = br#"{"claudeAiOauth":{"accessToken":"token-a","refreshToken":"DO_NOT_USE"}}"#;
        let root = br#"{"accessToken":"token-b","refreshToken":"DO_NOT_USE"}"#;
        assert_eq!(read_token_json(nested).as_deref(), Some("token-a"));
        assert_eq!(read_token_json(root).as_deref(), Some("token-b"));
    }
}
