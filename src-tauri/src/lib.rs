mod commands;
mod domain;
mod tokscale;

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
        .invoke_handler(tauri::generate_handler![
            app_contract_version,
            commands::get_usage_report,
            commands::get_usage_since_report,
            commands::get_quota_report,
            commands::get_tokscale_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Token Lens");
}
