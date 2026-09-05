mod codex_business;
mod commands;
mod domain;
mod floating_bubble;
mod session_detail;
mod session_metadata;
mod settings;
mod tokscale;
mod tray;

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
            let settings_store = SettingsStore::load(config_dir)?;
            let initial_settings = settings_store.get()?;
            app.manage(settings_store);
            app.manage(FloatingBubbleController::default());
            tray::initialize(app.handle(), &initial_settings)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let should_hide = window
                    .state::<SettingsStore>()
                    .get()
                    .map(|settings| settings.show_tray_icon)
                    .unwrap_or(false);
                if should_hide {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_contract_version,
            commands::get_usage_report,
            commands::get_usage_since_report,
            commands::get_session_detail,
            commands::get_quota_report,
            commands::get_tokscale_status,
            commands::get_session_metadata,
            commands::get_settings,
            commands::update_settings,
            commands::update_tray_summary,
            commands::get_floating_bubble_state,
            commands::collapse_floating_bubble_if_idle,
            commands::expand_floating_bubble,
            commands::peek_floating_bubble,
            commands::move_floating_bubble
        ])
        .run(tauri::generate_context!())
        .expect("error while running Token Lens");
}
