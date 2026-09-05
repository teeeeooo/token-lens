use crate::domain::{
    QuotaReport, SessionMetadataRef, SessionMetadataReport, TokscaleStatus, UsageGrouping,
    UsagePeriod, UsageReport,
};
use crate::floating_bubble::{
    self, BubbleDragOffset, FloatingBubbleController, FloatingBubblePayload,
};
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
pub async fn get_quota_report(adapter: State<'_, TokscaleAdapter>) -> Result<QuotaReport, String> {
    adapter.quota_report().await
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
    floating_bubble::collapse(&window, &settings, &bubble)
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
