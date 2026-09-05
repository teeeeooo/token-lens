mod commands;
mod domain;
mod floating_bubble;
mod settings;
mod tokscale;

use floating_bubble::FloatingBubbleController;
use settings::SettingsStore;
use tauri::Manager;
use tokscale::TokscaleAdapter;

#[tauri::command]
fn app_contract_version() -> &'static str {
    "v2"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tokscale = TokscaleAdapter::discover().expect("failed to initialize tokScale adapter");
    tauri::Builder::default()
        .manage(tokscale)
        .setup(|app| {
            let config_dir = app.path().app_config_dir().map_err(|error| {
                format!("failed to resolve Token Lens config directory: {error}")
            })?;
            app.manage(SettingsStore::load(config_dir)?);
            app.manage(FloatingBubbleController::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_contract_version,
            commands::get_usage_report,
            commands::get_usage_since_report,
            commands::get_quota_report,
            commands::get_tokscale_status,
            commands::get_settings,
            commands::update_settings,
            commands::get_floating_bubble_state,
            commands::collapse_floating_bubble_if_idle,
            commands::expand_floating_bubble,
            commands::peek_floating_bubble,
            commands::move_floating_bubble
        ])
        .run(tauri::generate_context!())
        .expect("error while running Token Lens");
}
