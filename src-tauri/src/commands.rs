use crate::antigravity_quota;
use crate::appearance;
use crate::claude_quota;
use crate::codex_business;
use crate::domain::{
    HistoryReport, QuotaProvider, QuotaReport, SessionDetailReport, SessionMetadataRef,
    SessionMetadataReport, SupportedProvider, TokscaleStatus, UsageGrouping, UsagePeriod,
    UsageReport,
};
use crate::floating_bubble::{
    self, BubbleDragOffset, FloatingBubbleController, FloatingBubblePayload,
};
use crate::gemini_quota;
use crate::session_detail;
use crate::session_metadata;
use crate::settings::{AppSettings, SettingsPatch, SettingsStore};
use crate::startup_timing::StartupTiming;
use crate::tokscale::TokscaleAdapter;
use crate::tray::{self, TraySummary};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State, WebviewWindow};

#[derive(Default)]
pub(crate) struct QuotaSnapshotCache {
    operation: tokio::sync::Mutex<()>,
    report: Mutex<Option<QuotaReport>>,
}

impl QuotaSnapshotCache {
    fn load(&self) -> Option<QuotaReport> {
        self.report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn store(&self, report: QuotaReport) {
        *self
            .report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(report);
    }
}

fn provider_auth_recovery_pending(provider: &QuotaProvider) -> bool {
    provider.freshness.recovery_state != crate::domain::RecoveryState::Idle
}

fn report_provider_recovery_pending(report: &QuotaReport, provider: SupportedProvider) -> bool {
    report
        .providers
        .iter()
        .find(|item| item.provider == provider)
        .is_some_and(provider_auth_recovery_pending)
}

fn prepare_provider_recovery(report: &mut QuotaReport, provider: SupportedProvider) {
    if let Some(item) = report
        .providers
        .iter_mut()
        .find(|item| item.provider == provider)
    {
        // This row is a previous collection, not fresh tokScale input. Only an
        // actual successful probe may promote it to ready again.
        if !item.windows.is_empty() {
            item.freshness.status = crate::domain::QuotaStatus::Stale;
        }
    }
}

fn merge_provider_result(
    report: &mut QuotaReport,
    enriched: QuotaReport,
    provider: SupportedProvider,
) {
    let Some(provider_result) = enriched
        .providers
        .into_iter()
        .find(|item| item.provider == provider)
    else {
        return;
    };
    if let Some(existing) = report
        .providers
        .iter_mut()
        .find(|item| item.provider == provider)
    {
        *existing = provider_result;
    } else {
        report.providers.push(provider_result);
    }
}

#[tauri::command]
pub async fn get_usage_report(
    adapter: State<'_, TokscaleAdapter>,
    period: UsagePeriod,
    grouping: UsageGrouping,
) -> Result<UsageReport, String> {
    adapter.usage_report(period, grouping).await
}

#[tauri::command]
pub async fn get_usage_since_report(
    adapter: State<'_, TokscaleAdapter>,
    since: String,
    grouping: UsageGrouping,
) -> Result<UsageReport, String> {
    adapter.usage_since_report(&since, grouping).await
}

#[tauri::command]
pub async fn get_dashboard_history(
    adapter: State<'_, TokscaleAdapter>,
    since: String,
) -> Result<HistoryReport, String> {
    adapter.history_report(&since).await
}

#[tauri::command]
pub async fn get_session_detail(
    app: AppHandle,
    client: String,
    session_id: String,
    start_time_ms: Option<i64>,
    session_cost: f64,
) -> Result<SessionDetailReport, String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|error| format!("failed to resolve home directory: {error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        session_detail::read(&home, &client, &session_id, start_time_ms, session_cost)
    })
    .await
    .map_err(|error| format!("session detail task failed: {error}"))
}

fn quota_base(result: Result<QuotaReport, String>) -> QuotaReport {
    result.unwrap_or_else(|_| QuotaReport {
        generated_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        providers: [
            SupportedProvider::Codex,
            SupportedProvider::Claude,
            SupportedProvider::Gemini,
            SupportedProvider::Antigravity,
        ]
        .into_iter()
        .map(|provider| QuotaProvider {
            provider,
            plan: None,
            account_email: None,
            diagnostic: Some("Base quota collection unavailable".to_owned()),
            windows: Vec::new(),
            reset_credits: None,
            credit_status: None,
            spend_control: None,
            freshness: Default::default(),
        })
        .collect(),
        source: "tokscale",
    })
}

#[tauri::command]
pub async fn get_quota_report(
    app: AppHandle,
    adapter: State<'_, TokscaleAdapter>,
    quota_cache: State<'_, QuotaSnapshotCache>,
) -> Result<QuotaReport, String> {
    let _operation = quota_cache.operation.lock().await;
    let home = app.path().home_dir().ok();
    let expected_workspace_id = home
        .as_deref()
        .and_then(codex_business::selected_workspace_id);
    // Base failure must not suppress independent provider adapters. Never reuse an
    // old base here: it may belong to the account selected before this refresh.
    let report = quota_base(adapter.quota_report().await);
    app.state::<StartupTiming>()
        .record_internal("quota-tokscale-ready");
    let report = match home {
        Some(home) => {
            let base = report;
            let (codex, claude, gemini, antigravity) = tokio::join!(
                async {
                    let enriched = codex_business::enrich_quota_report(
                        &home,
                        expected_workspace_id,
                        base.clone(),
                    )
                    .await;
                    app.state::<StartupTiming>()
                        .record_internal("quota-codex-ready");
                    enriched
                },
                async {
                    let enriched = claude_quota::enrich_quota_report(&home, base.clone()).await;
                    app.state::<StartupTiming>()
                        .record_internal("quota-claude-ready");
                    enriched
                },
                async {
                    let enriched = gemini_quota::enrich_quota_report(&home, base.clone()).await;
                    app.state::<StartupTiming>()
                        .record_internal("quota-gemini-ready");
                    enriched
                },
                async {
                    let enriched =
                        antigravity_quota::enrich_quota_report(&home, base.clone()).await;
                    app.state::<StartupTiming>()
                        .record_internal("quota-antigravity-ready");
                    enriched
                },
            );
            let mut merged = base;
            merge_provider_result(&mut merged, codex, SupportedProvider::Codex);
            merge_provider_result(&mut merged, claude, SupportedProvider::Claude);
            merge_provider_result(&mut merged, gemini, SupportedProvider::Gemini);
            merge_provider_result(&mut merged, antigravity, SupportedProvider::Antigravity);
            app.state::<StartupTiming>()
                .record_internal("quota-enrichment-ready");
            merged
        }
        None => report,
    };
    quota_cache.store(report.clone());
    Ok(report)
}

#[tauri::command]
pub async fn get_quota_recovery_report(
    app: AppHandle,
    quota_cache: State<'_, QuotaSnapshotCache>,
) -> Result<QuotaReport, String> {
    let _operation = quota_cache.operation.lock().await;
    let mut report = quota_cache
        .load()
        .ok_or_else(|| "quota recovery requested before a full quota report".to_owned())?;
    let Some(home) = app.path().home_dir().ok() else {
        return Ok(report);
    };

    let mut attempted = false;
    if report_provider_recovery_pending(&report, SupportedProvider::Claude) {
        attempted = true;
        prepare_provider_recovery(&mut report, SupportedProvider::Claude);
        report = claude_quota::enrich_quota_report(&home, report).await;
    }
    if report_provider_recovery_pending(&report, SupportedProvider::Gemini) {
        attempted = true;
        prepare_provider_recovery(&mut report, SupportedProvider::Gemini);
        report = gemini_quota::enrich_quota_report(&home, report).await;
    }
    if attempted {
        report.generated_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
    }

    quota_cache.store(report.clone());
    Ok(report)
}

#[tauri::command]
pub async fn get_tokscale_status(
    adapter: State<'_, TokscaleAdapter>,
) -> Result<TokscaleStatus, String> {
    Ok(adapter.status().await)
}

#[tauri::command]
pub async fn get_session_metadata(
    app: AppHandle,
    sessions: Vec<SessionMetadataRef>,
) -> Result<SessionMetadataReport, String> {
    let home = app
        .path()
        .home_dir()
        .map_err(|error| format!("failed to resolve home directory: {error}"))?;
    tokio::task::spawn_blocking(move || session_metadata::collect(&home, sessions))
        .await
        .map_err(|error| format!("session metadata task failed: {error}"))?
}

#[tauri::command]
pub fn get_settings(settings: State<'_, SettingsStore>) -> Result<AppSettings, String> {
    settings.get()
}

#[tauri::command]
pub fn record_startup_timing(
    timing: State<'_, StartupTiming>,
    phase: String,
) -> Result<f64, String> {
    timing.record_renderer(&phase)
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let updated = settings.update(patch)?;
    tray::apply_settings(&app, &updated)?;
    if !updated.floating_bubble_enabled {
        floating_bubble::expand_if_disabled(&window, &settings, &bubble)?;
    }
    let bubble_collapsed = floating_bubble::current_state(&settings, &bubble)?.collapsed;
    appearance::apply_to_window(&window, &updated, bubble_collapsed)?;
    Ok(updated)
}

#[tauri::command]
pub fn open_provider_error_log_directory(app: AppHandle) -> Result<(), String> {
    let directory = app
        .path()
        .app_log_dir()
        .map_err(|_| "failed to resolve Token Lens error log directory".to_owned())?;
    fs::create_dir_all(&directory)
        .map_err(|_| "failed to prepare Token Lens error log directory".to_owned())?;
    open_directory_in_shell(&directory)
}

fn open_directory_in_shell(directory: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");

    command
        .arg(directory)
        .spawn()
        .map(|_| ())
        .map_err(|_| "failed to open Token Lens error log directory".to_owned())
}

#[tauri::command]
pub fn update_tray_summary(app: AppHandle, summary: TraySummary) -> Result<(), String> {
    tray::update_summary(&app, &summary)
}

#[tauri::command]
pub fn get_floating_bubble_state(
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
) -> Result<FloatingBubblePayload, String> {
    floating_bubble::current_state(&settings, &bubble)
}
#[tauri::command]
pub fn collapse_floating_bubble_if_idle(
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
) -> Result<FloatingBubblePayload, String> {
    floating_bubble::collapse_if_idle(&window, &settings, &bubble)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MinimizeTarget {
    Bubble,
    Tray,
    Os,
}

fn minimize_target(settings: &AppSettings) -> MinimizeTarget {
    if settings.floating_bubble_enabled {
        MinimizeTarget::Bubble
    } else if settings.show_tray_icon {
        MinimizeTarget::Tray
    } else {
        MinimizeTarget::Os
    }
}

#[tauri::command]
pub fn minimize_main_window(
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
) -> Result<FloatingBubblePayload, String> {
    match minimize_target(&settings.get()?) {
        MinimizeTarget::Bubble => return floating_bubble::collapse(&window, &settings, &bubble),
        MinimizeTarget::Tray => window
            .hide()
            .map_err(|error| format!("failed to hide Token Lens window: {error}"))?,
        MinimizeTarget::Os => window
            .minimize()
            .map_err(|error| format!("failed to minimize Token Lens window: {error}"))?,
    }
    floating_bubble::current_state(&settings, &bubble)
}

#[tauri::command]
pub fn set_floating_bubble_width(
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
    width: f64,
) -> Result<FloatingBubblePayload, String> {
    floating_bubble::set_collapsed_width(&window, &settings, &bubble, width)
}

#[tauri::command]
pub fn expand_floating_bubble(
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
) -> Result<FloatingBubblePayload, String> {
    floating_bubble::expand(&window, &settings, &bubble, true)
}

#[tauri::command]
pub fn peek_floating_bubble(
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
) -> Result<FloatingBubblePayload, String> {
    floating_bubble::expand(&window, &settings, &bubble, false)
}
#[tauri::command]
pub fn move_floating_bubble(
    window: WebviewWindow,
    settings: State<'_, SettingsStore>,
    bubble: State<'_, FloatingBubbleController>,
    offset: BubbleDragOffset,
) -> Result<FloatingBubblePayload, String> {
    floating_bubble::move_to_cursor(&window, &settings, &bubble, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota_provider(provider: SupportedProvider, diagnostic: &str) -> QuotaProvider {
        QuotaProvider {
            provider,
            plan: None,
            account_email: None,
            diagnostic: Some(diagnostic.to_owned()),
            windows: Vec::new(),
            reset_credits: None,
            credit_status: None,
            spend_control: None,
            freshness: Default::default(),
        }
    }

    #[test]
    fn failed_base_keeps_independent_provider_merge_and_sanitizes_error() {
        let mut base = quota_base(Err("private payload sentinel".to_owned()));
        let enriched = QuotaReport {
            generated_at_ms: 2,
            providers: vec![quota_provider(
                SupportedProvider::Gemini,
                "independent result",
            )],
            source: "test",
        };
        merge_provider_result(&mut base, enriched, SupportedProvider::Gemini);
        assert_eq!(base.providers.len(), 4);
        assert_eq!(
            base.providers[2].diagnostic.as_deref(),
            Some("independent result")
        );
        assert_eq!(base.providers[0].windows.len(), 0);
        assert!(!serde_json::to_string(&base).unwrap().contains("sentinel"));
    }

    #[test]
    fn parallel_quota_merge_replaces_only_the_target_provider() {
        let mut base = QuotaReport {
            generated_at_ms: 1,
            providers: vec![
                quota_provider(SupportedProvider::Codex, "base-codex"),
                quota_provider(SupportedProvider::Claude, "base-claude"),
            ],
            source: "test",
        };
        let enriched = QuotaReport {
            generated_at_ms: 2,
            providers: vec![
                quota_provider(SupportedProvider::Codex, "wrong-codex"),
                quota_provider(SupportedProvider::Claude, "parallel-claude"),
            ],
            source: "test",
        };
        merge_provider_result(&mut base, enriched, SupportedProvider::Claude);
        assert_eq!(base.generated_at_ms, 1);
        assert_eq!(base.providers[0].diagnostic.as_deref(), Some("base-codex"));
        assert_eq!(
            base.providers[1].diagnostic.as_deref(),
            Some("parallel-claude")
        );
    }

    #[test]
    fn parallel_quota_merge_appends_a_provider_missing_from_tokscale_base() {
        let mut base = QuotaReport {
            generated_at_ms: 1,
            providers: vec![quota_provider(SupportedProvider::Codex, "base-codex")],
            source: "test",
        };
        let enriched = QuotaReport {
            generated_at_ms: 1,
            providers: vec![
                quota_provider(SupportedProvider::Codex, "base-codex"),
                quota_provider(SupportedProvider::Gemini, "parallel-gemini"),
            ],
            source: "test",
        };
        merge_provider_result(&mut base, enriched, SupportedProvider::Gemini);
        assert_eq!(base.providers.len(), 2);
        assert_eq!(base.providers[1].provider, SupportedProvider::Gemini);
        assert_eq!(
            base.providers[1].diagnostic.as_deref(),
            Some("parallel-gemini")
        );
    }

    #[test]
    fn recovery_input_is_stale_without_refreshing_its_capture_time() {
        let mut provider = quota_provider(SupportedProvider::Claude, "description");
        provider.windows.push(crate::domain::QuotaWindow {
            kind: crate::domain::QuotaWindowKind::Session,
            label: "5h".to_owned(),
            metric: "quota",
            additional: false,
            used: None,
            limit: None,
            remaining: None,
            used_percent: None,
            remaining_percent: Some(50.0),
            remaining_label: None,
            resets_at: None,
            currency: None,
            show_meter: true,
            source: "test",
        });
        provider.record_success(100);
        provider.freshness.recovery_state = crate::domain::RecoveryState::Pending;
        let mut report = QuotaReport {
            generated_at_ms: 100,
            providers: vec![provider],
            source: "test",
        };
        prepare_provider_recovery(&mut report, SupportedProvider::Claude);
        assert_eq!(
            report.providers[0].freshness.status,
            crate::domain::QuotaStatus::Stale
        );
        assert_eq!(report.providers[0].freshness.last_success_at_ms, Some(100));
        assert_eq!(report.providers[0].freshness.last_attempt_at_ms, Some(100));
        assert!(provider_auth_recovery_pending(&report.providers[0]));
    }

    #[test]
    fn provider_only_merge_preserves_other_provider_capture_times() {
        let mut codex = quota_provider(SupportedProvider::Codex, "base");
        codex.freshness.last_success_at_ms = Some(100);
        let mut gemini = quota_provider(SupportedProvider::Gemini, "recovered");
        gemini.freshness.last_success_at_ms = Some(200);
        let mut report = QuotaReport {
            generated_at_ms: 999,
            providers: vec![codex],
            source: "test",
        };
        merge_provider_result(
            &mut report,
            QuotaReport {
                generated_at_ms: 1000,
                providers: vec![gemini],
                source: "test",
            },
            SupportedProvider::Gemini,
        );
        assert_eq!(report.providers[0].freshness.last_success_at_ms, Some(100));
        assert_eq!(report.providers[1].freshness.last_success_at_ms, Some(200));
    }

    #[test]
    fn quota_recovery_detection_is_independent_of_diagnostic_language() {
        let mut provider = quota_provider(SupportedProvider::Claude, "다른 문구");
        assert!(!provider_auth_recovery_pending(&provider));
        provider.freshness.recovery_state = crate::domain::RecoveryState::Pending;
        assert!(provider_auth_recovery_pending(&provider));
        provider.diagnostic = None;
        provider.freshness.recovery_state = crate::domain::RecoveryState::Cooldown;
        assert!(provider_auth_recovery_pending(&provider));
    }

    #[test]
    fn minimize_policy_prefers_bubble_then_tray_then_os() {
        let defaults = AppSettings::default();
        assert_eq!(minimize_target(&defaults), MinimizeTarget::Bubble);

        let tray = AppSettings {
            floating_bubble_enabled: false,
            show_tray_icon: true,
            ..AppSettings::default()
        };
        assert_eq!(minimize_target(&tray), MinimizeTarget::Tray);

        let os = AppSettings {
            floating_bubble_enabled: false,
            show_tray_icon: false,
            ..AppSettings::default()
        };
        assert_eq!(minimize_target(&os), MinimizeTarget::Os);
    }
}
