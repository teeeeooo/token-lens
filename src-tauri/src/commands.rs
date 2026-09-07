use crate::antigravity_quota;
use crate::appearance;
use crate::claude_quota;
use crate::codex_business;
use crate::domain::{
    HistoryReport, QuotaReport, SessionDetailReport, SessionMetadataRef, SessionMetadataReport,
    TokscaleStatus, UsageGrouping, UsagePeriod, UsageReport,
};
use crate::floating_bubble::{
    self, BubbleDragOffset, FloatingBubbleController, FloatingBubblePayload,
};
use crate::gemini_quota;
use crate::session_detail;
use crate::session_metadata;
use crate::settings::{AppSettings, SettingsPatch, SettingsStore};
use crate::tokscale::TokscaleAdapter;
use crate::tray::{self, TraySummary};
use tauri::{AppHandle, Manager, State, WebviewWindow};

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

#[tauri::command]
pub async fn get_quota_report(
    app: AppHandle,
    adapter: State<'_, TokscaleAdapter>,
) -> Result<QuotaReport, String> {
    let home = app.path().home_dir().ok();
    let expected_workspace_id = home
        .as_deref()
        .and_then(codex_business::selected_workspace_id);
    let report = adapter.quota_report().await?;
    Ok(match home {
        Some(home) => {
            let report =
                codex_business::enrich_quota_report(&home, expected_workspace_id, report).await;
            let report = claude_quota::enrich_quota_report(&home, report).await;
            let report = gemini_quota::enrich_quota_report(&home, report).await;
            antigravity_quota::enrich_quota_report(&home, report).await
        }
        None => report,
    })
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
