use crate::floating_bubble::{self, FloatingBubbleController, FloatingBubblePayload};
use crate::settings::{AppSettings, SettingsStore};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const TRAY_ID: &str = "token-lens-v2";
const EVENT_NAME: &str = "token-lens://tray-action";

const MENU_REFRESH: &str = "tray.refresh";
const MENU_SETTINGS: &str = "tray.settings";
const MENU_QUIT: &str = "tray.quit";
const MENU_VIEW_PREFIX: &str = "tray.view.";

const RETAINED_VIEWS: [(&str, &str); 5] = [
    ("home", "Home"),
    ("tool", "Tools"),
    ("model", "Models"),
    ("session", "Sessions"),
    ("limits", "Limits"),
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraySummary {
    pub today_tokens: u64,
    pub today_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayActionPayload<'a> {
    action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bubble: Option<FloatingBubblePayload>,
}

pub fn initialize(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let open_view_items = RETAINED_VIEWS
        .iter()
        .map(|(id, label)| {
            MenuItem::with_id(
                app,
                format!("{MENU_VIEW_PREFIX}{id}"),
                *label,
                true,
                None::<&str>,
            )
            .map_err(|error| format!("failed to build tray view item: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let open_view_refs = open_view_items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect::<Vec<_>>();
    let open_view = Submenu::with_items(app, "Open View", true, &open_view_refs)
        .map_err(|error| format!("failed to build tray view menu: {error}"))?;

    let refresh = MenuItem::with_id(app, MENU_REFRESH, "Refresh Now", true, None::<&str>)
        .map_err(|error| format!("failed to build tray refresh item: {error}"))?;
    let settings_item = MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, None::<&str>)
        .map_err(|error| format!("failed to build tray settings item: {error}"))?;
    let version = MenuItem::new(
        app,
        format!("Version {}", app.package_info().version),
        false,
        None::<&str>,
    )
    .map_err(|error| format!("failed to build tray version item: {error}"))?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit Token Lens", true, None::<&str>)
        .map_err(|error| format!("failed to build tray quit item: {error}"))?;
    let sep1 = PredefinedMenuItem::separator(app)
        .map_err(|error| format!("failed to build tray separator: {error}"))?;
    let sep2 = PredefinedMenuItem::separator(app)
        .map_err(|error| format!("failed to build tray separator: {error}"))?;

    let menu = Menu::with_items(
        app,
        &[
            &refresh,
            &open_view,
            &sep1,
            &version,
            &settings_item,
            &sep2,
            &quit,
        ],
    )
    .map_err(|error| format!("failed to build tray menu: {error}"))?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Token Lens")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            let activate = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            );
            if activate {
                if let Ok(bubble) = focus_main_window(tray.app_handle()) {
                    let _ = emit_action(tray.app_handle(), "focus", None, Some(bubble));
                }
            }
        });
    #[cfg(target_os = "macos")]
    {
        let icon =
            tauri::image::Image::from_bytes(include_bytes!("../icons/tray-token-monitor.png"))
                .map_err(|error| format!("failed to decode macOS tray icon: {error}"))?;
        builder = builder.icon(icon).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder
        .build(app)
        .map_err(|error| format!("failed to create Token Lens tray icon: {error}"))?;
    tray.set_visible(settings.show_tray_icon)
        .map_err(|error| format!("failed to set tray visibility: {error}"))?;
    Ok(())
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();
    if id == MENU_QUIT {
        app.exit(0);
        return;
    }
    if id == MENU_REFRESH {
        let _ = emit_action(app, "refresh", None, None);
        return;
    }
    if id == MENU_SETTINGS {
        let bubble = focus_main_window(app).ok();
        let _ = emit_action(app, "openSettings", None, bubble);
        return;
    }
    if let Some(view) = id.strip_prefix(MENU_VIEW_PREFIX) {
        if RETAINED_VIEWS.iter().any(|(id, _)| *id == view) {
            let bubble = focus_main_window(app).ok();
            let _ = emit_action(app, "openView", Some(view), bubble);
        }
    }
}

fn emit_action(
    app: &AppHandle,
    action: &str,
    view: Option<&str>,
    bubble: Option<FloatingBubblePayload>,
) -> Result<(), String> {
    app.emit_to(
        "main",
        EVENT_NAME,
        TrayActionPayload {
            action,
            view,
            bubble,
        },
    )
    .map_err(|error| format!("failed to emit tray action: {error}"))
}

pub fn focus_main_window(app: &AppHandle) -> Result<FloatingBubblePayload, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main Token Lens window is unavailable".to_owned())?;
    let settings = app.state::<SettingsStore>();
    let bubble = app.state::<FloatingBubbleController>();
    if floating_bubble::current_state(&settings, &bubble)?.collapsed {
        return floating_bubble::expand(&window, &settings, &bubble, true);
    }
    window
        .show()
        .map_err(|error| format!("failed to show Token Lens window: {error}"))?;
    if window
        .is_minimized()
        .map_err(|error| format!("failed to read Token Lens window state: {error}"))?
    {
        window
            .unminimize()
            .map_err(|error| format!("failed to restore Token Lens window: {error}"))?;
    }
    window
        .set_focus()
        .map_err(|error| format!("failed to focus Token Lens window: {error}"))?;
    floating_bubble::current_state(&settings, &bubble)
}

pub fn apply_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "Token Lens tray icon is unavailable".to_owned())?;
    tray.set_visible(settings.show_tray_icon)
        .map_err(|error| format!("failed to set tray visibility: {error}"))
}

pub fn update_summary(app: &AppHandle, summary: &TraySummary) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "Token Lens tray icon is unavailable".to_owned())?;
    let tokens = compact_tokens(summary.today_tokens);
    let tooltip = format!(
        "Token Lens — Today {tokens} tokens · ${:.2}",
        summary.today_cost_usd.max(0.0)
    );
    tray.set_tooltip(Some(tooltip))
        .map_err(|error| format!("failed to update tray tooltip: {error}"))?;
    #[cfg(not(target_os = "windows"))]
    tray.set_title(Some(tokens))
        .map_err(|error| format!("failed to update tray title: {error}"))?;
    Ok(())
}

fn compact_tokens(value: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1_000_000_000, "B"), (1_000_000, "M"), (1_000, "K")];
    for (scale, suffix) in UNITS {
        if value >= scale {
            let scaled = value as f64 / scale as f64;
            let mut number = if scaled >= 100.0 {
                format!("{scaled:.0}")
            } else if scaled >= 10.0 {
                format!("{scaled:.1}")
            } else {
                format!("{scaled:.2}")
            };
            if number.contains('.') {
                while number.ends_with('0') {
                    number.pop();
                }
                if number.ends_with('.') {
                    number.pop();
                }
            }
            return format!("{number}{suffix}");
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_focus_payload_carries_expanded_bubble_state_to_renderer() {
        let payload = TrayActionPayload {
            action: "focus",
            view: None,
            bubble: Some(FloatingBubblePayload {
                enabled: true,
                collapsed: false,
                side: None,
            }),
        };
        let json = serde_json::to_value(payload).expect("serialize tray payload");
        assert_eq!(json["action"], "focus");
        assert_eq!(json["bubble"]["collapsed"], false);
        assert!(json.get("view").is_none());
    }

    #[test]
    fn compact_token_labels_stay_small_enough_for_a_menu_bar() {
        assert_eq!(compact_tokens(999), "999");
        assert_eq!(compact_tokens(1_000), "1K");
        assert_eq!(compact_tokens(10_000), "10K");
        assert_eq!(compact_tokens(1_250), "1.25K");
        assert_eq!(compact_tokens(12_500), "12.5K");
        assert_eq!(compact_tokens(125_000), "125K");
        assert_eq!(compact_tokens(1_250_000), "1.25M");
    }
}
