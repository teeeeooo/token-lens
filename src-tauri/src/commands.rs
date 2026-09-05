use crate::domain::{QuotaReport, TokscaleStatus, UsageGrouping, UsagePeriod, UsageReport};
use crate::tokscale::TokscaleAdapter;
use tauri::State;

#[tauri::command]
pub async fn get_usage_report(
    adapter: State<'_, TokscaleAdapter>,
    period: UsagePeriod,
    grouping: UsageGrouping,
) -> Result<UsageReport, String> {
    adapter.usage_report(period, grouping).await
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
