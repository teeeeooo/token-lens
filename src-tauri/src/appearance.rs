use crate::settings::AppSettings;
#[cfg(target_os = "windows")]
use crate::settings::WindowsBackdrop;
use tauri::WebviewWindow;

pub fn apply_to_window(
    window: &WebviewWindow,
    settings: &AppSettings,
    bubble_collapsed: bool,
) -> Result<(), String> {
    window
        .set_zoom(settings.zoom_factor)
        .map_err(|error| format!("failed to apply Token Lens zoom: {error}"))?;
    apply_backdrop(window, settings, bubble_collapsed);
    Ok(())
}

pub fn apply_backdrop(window: &WebviewWindow, settings: &AppSettings, bubble_collapsed: bool) {
    #[cfg(target_os = "windows")]
    {
        use tauri::window::{Effect, EffectsBuilder};
        let enabled = settings.windows_backdrop == WindowsBackdrop::Acrylic && !bubble_collapsed;
        let effects = enabled.then(|| EffectsBuilder::new().effect(Effect::Acrylic).build());
        let _ = window.set_effects(effects);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, settings.windows_backdrop, bubble_collapsed);
    }
}
