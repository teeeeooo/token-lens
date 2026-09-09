mod antigravity_local;
mod antigravity_quota;
mod appearance;
mod background_process;
mod claude_cli;
mod claude_quota;
mod codex_business;
mod commands;
mod domain;
mod floating_bubble;
mod gemini_cli;
mod gemini_quota;
mod google_code_assist;
mod http_diagnostic;
mod portable_sidecar;
mod provider_cli_auth;
mod provider_error_log;
mod session_detail;
mod session_metadata;
mod settings;
mod startup_timing;
mod tokscale;
mod tray;
mod window_state;

use floating_bubble::FloatingBubbleController;
use settings::SettingsStore;
use startup_timing::StartupTiming;
use tauri::Manager;
use tokscale::TokscaleAdapter;

#[tauri::command]
fn app_contract_version() -> &'static str {
    "v2"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_timing = StartupTiming::new();
    startup_timing.record_internal("tokscale-discovery-start");
    let tokscale = TokscaleAdapter::discover().expect("failed to initialize tokScale adapter");
    if tokscale.source() == "embedded-portable" {
        startup_timing.record_internal("portable-sidecar-ready");
    }
    startup_timing.record_internal("tokscale-discovery-ready");
    tauri::Builder::default()
        .manage(tokscale)
        .manage(startup_timing)
        .manage(commands::QuotaSnapshotCache::default())
        .setup(|app| {
            let config_dir = app.path().app_config_dir().map_err(|error| {
                format!("failed to resolve Token Lens config directory: {error}")
            })?;
            if let Ok(log_dir) = app.path().app_log_dir() {
                app.state::<StartupTiming>().initialize(log_dir.clone());
                provider_error_log::initialize(log_dir);
            }
            let settings_store = SettingsStore::load(config_dir)?;
            let initial_settings = settings_store.get()?;
            app.manage(settings_store);
            app.manage(window_state::WindowBoundsController::default());
            app.manage(FloatingBubbleController::default());
            if let Some(window) = app.get_webview_window("main") {
                window_state::restore_initial(&window, &initial_settings)?;
                appearance::apply_to_window(&window, &initial_settings, false)?;
            }
            tray::initialize(app.handle(), &initial_settings)?;
            app.state::<StartupTiming>()
                .record_internal("tauri-setup-ready");
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                    window_state::schedule_persist(webview);
                }
            }
            tauri::WindowEvent::CloseRequested { .. } => {
                if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                    let _ = window_state::persist_now(&webview);
                }
                let _ = window.app_handle().state::<StartupTiming>().persist();
                window.app_handle().exit(0);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            app_contract_version,
            commands::get_usage_report,
            commands::get_usage_since_report,
            commands::get_dashboard_history,
            commands::get_session_detail,
            commands::get_quota_report,
            commands::get_quota_recovery_report,
            commands::get_tokscale_status,
            commands::get_session_metadata,
            commands::get_settings,
            commands::record_startup_timing,
            commands::update_settings,
            commands::open_provider_error_log_directory,
            commands::update_tray_summary,
            commands::get_floating_bubble_state,
            commands::collapse_floating_bubble_if_idle,
            commands::minimize_main_window,
            commands::set_floating_bubble_width,
            commands::expand_floating_bubble,
            commands::peek_floating_bubble,
            commands::move_floating_bubble
        ])
        .run(tauri::generate_context!())
        .expect("error while running Token Lens");
}
