use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SETTINGS_FILE_NAME: &str = "settings-v2.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FloatingBubbleTrigger {
    #[default]
    Click,
    Hover,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreset {
    #[default]
    Default,
    Obsidian,
    Porcelain,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowsBackdrop {
    Off,
    #[default]
    Acrylic,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub window_bounds: Option<WindowBounds>,
    pub show_tray_icon: bool,
    pub floating_bubble_enabled: bool,
    pub floating_bubble_trigger: FloatingBubbleTrigger,
    pub floating_bubble_content: String,
    pub floating_bubble_scale: f64,
    pub theme_preset: ThemePreset,
    pub zoom_factor: f64,
    pub show_compact_total_tokens: bool,
    pub windows_backdrop: WindowsBackdrop,
    pub language: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            window_bounds: None,
            show_tray_icon: true,
            floating_bubble_enabled: true,
            floating_bubble_trigger: FloatingBubbleTrigger::Click,
            floating_bubble_content: "limitsAllSessions".to_owned(),
            floating_bubble_scale: 1.0,
            theme_preset: ThemePreset::Default,
            zoom_factor: 1.0,
            show_compact_total_tokens: false,
            windows_backdrop: WindowsBackdrop::Acrylic,
            language: "auto".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub show_tray_icon: Option<bool>,
    pub floating_bubble_enabled: Option<bool>,
    pub floating_bubble_trigger: Option<FloatingBubbleTrigger>,
    pub floating_bubble_content: Option<String>,
    pub floating_bubble_scale: Option<f64>,
    pub theme_preset: Option<ThemePreset>,
    pub zoom_factor: Option<f64>,
    pub show_compact_total_tokens: Option<bool>,
    pub windows_backdrop: Option<WindowsBackdrop>,
    pub language: Option<String>,
}

pub struct SettingsStore {
    path: PathBuf,
    value: Mutex<AppSettings>,
}

impl SettingsStore {
    pub fn load(config_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&config_dir)
            .map_err(|error| format!("failed to create Token Lens config directory: {error}"))?;
        let path = config_dir.join(SETTINGS_FILE_NAME);
        let value = read_settings(&path).unwrap_or_default();
        Ok(Self {
            path,
            value: Mutex::new(normalize_settings(value)),
        })
    }
    pub fn get(&self) -> Result<AppSettings, String> {
        self.value
            .lock()
            .map(|value| value.clone())
            .map_err(|_| "settings state lock was poisoned".to_owned())
    }

    pub fn update_window_bounds(&self, bounds: WindowBounds) -> Result<(), String> {
        let mut value = self
            .value
            .lock()
            .map_err(|_| "settings state lock was poisoned".to_owned())?;
        if value.window_bounds == Some(bounds) {
            return Ok(());
        }
        value.window_bounds = Some(bounds);
        write_settings(&self.path, &value)
    }

    pub fn update(&self, patch: SettingsPatch) -> Result<AppSettings, String> {
        let mut value = self
            .value
            .lock()
            .map_err(|_| "settings state lock was poisoned".to_owned())?;
        if let Some(show) = patch.show_tray_icon {
            value.show_tray_icon = show;
        }
        if let Some(enabled) = patch.floating_bubble_enabled {
            value.floating_bubble_enabled = enabled;
        }
        if let Some(trigger) = patch.floating_bubble_trigger {
            value.floating_bubble_trigger = trigger;
        }
        if let Some(content) = patch.floating_bubble_content {
            value.floating_bubble_content = content;
        }
        if let Some(scale) = patch.floating_bubble_scale {
            value.floating_bubble_scale = scale;
        }
        if let Some(theme) = patch.theme_preset {
            value.theme_preset = theme;
        }
        if let Some(zoom) = patch.zoom_factor {
            value.zoom_factor = zoom;
        }
        if let Some(compact) = patch.show_compact_total_tokens {
            value.show_compact_total_tokens = compact;
        }
        if let Some(backdrop) = patch.windows_backdrop {
            value.windows_backdrop = backdrop;
        }
        if let Some(language) = patch.language {
            value.language = language;
        }
        *value = normalize_settings(value.clone());
        write_settings(&self.path, &value)?;
        Ok(value.clone())
    }
}

fn normalize_settings(mut value: AppSettings) -> AppSettings {
    const BUBBLE_CONTENT_VALUES: [&str; 6] = [
        "limitsAllSessions",
        "icon",
        "barsSession",
        "barsWeekly",
        "barsAllSessions",
        "bars",
    ];
    if !BUBBLE_CONTENT_VALUES.contains(&value.floating_bubble_content.as_str()) {
        value.floating_bubble_content = "limitsAllSessions".to_owned();
    }
    if !value.floating_bubble_scale.is_finite() {
        value.floating_bubble_scale = 1.0;
    }
    value.floating_bubble_scale =
        (value.floating_bubble_scale.clamp(0.7, 1.5) * 10.0).round() / 10.0;
    if !value.zoom_factor.is_finite() {
        value.zoom_factor = 1.0;
    }
    value.zoom_factor = (value.zoom_factor.clamp(0.7, 1.6) * 10.0).round() / 10.0;
    const LANGUAGES: [&str; 6] = ["auto", "en", "ko", "ja", "zh-CN", "zh-TW"];
    if !LANGUAGES.contains(&value.language.as_str()) {
        value.language = "auto".to_owned();
    }
    value
}
fn read_settings(path: &Path) -> Option<AppSettings> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_settings(path: &Path, value: &AppSettings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize Token Lens settings: {error}"))?;
    // Direct truncate/write is intentionally cross-platform. std::fs::rename cannot
    // replace an existing destination on Windows, which made repeated settings saves fail.
    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("failed to write Token Lens settings: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_retained_shell_and_appearance_policy() {
        let settings = AppSettings::default();
        assert_eq!(settings.window_bounds, None);
        assert!(settings.show_tray_icon);
        assert!(settings.floating_bubble_enabled);
        assert_eq!(
            settings.floating_bubble_trigger,
            FloatingBubbleTrigger::Click
        );
        assert_eq!(settings.floating_bubble_content, "limitsAllSessions");
        assert_eq!(settings.floating_bubble_scale, 1.0);
        assert_eq!(settings.theme_preset, ThemePreset::Default);
        assert_eq!(settings.zoom_factor, 1.0);
        assert!(!settings.show_compact_total_tokens);
        assert_eq!(settings.windows_backdrop, WindowsBackdrop::Acrylic);
        assert_eq!(settings.language, "auto");
    }

    #[test]
    fn normalization_keeps_supported_bubble_modes_and_rejects_retired_content() {
        let bars = normalize_settings(AppSettings {
            floating_bubble_content: "barsWeekly".to_owned(),
            ..AppSettings::default()
        });
        assert_eq!(bars.floating_bubble_content, "barsWeekly");
        let retired = normalize_settings(AppSettings {
            floating_bubble_content: "tokens".to_owned(),
            ..AppSettings::default()
        });
        assert_eq!(retired.floating_bubble_content, "limitsAllSessions");
    }

    #[test]
    fn bubble_scale_is_clamped_to_user_range_and_step() {
        let high = normalize_settings(AppSettings {
            floating_bubble_scale: 4.0,
            ..AppSettings::default()
        });
        let stepped = normalize_settings(AppSettings {
            floating_bubble_scale: 1.24,
            ..AppSettings::default()
        });
        assert_eq!(high.floating_bubble_scale, 1.5);
        assert_eq!(stepped.floating_bubble_scale, 1.2);
    }

    #[test]
    fn zoom_is_clamped_to_retained_v1_range_and_step() {
        let high = normalize_settings(AppSettings {
            zoom_factor: 4.0,
            ..AppSettings::default()
        });
        let stepped = normalize_settings(AppSettings {
            zoom_factor: 1.24,
            ..AppSettings::default()
        });
        assert_eq!(high.zoom_factor, 1.6);
        assert_eq!(stepped.zoom_factor, 1.2);
    }

    #[test]
    fn older_v2_settings_gain_appearance_defaults() {
        let settings: AppSettings = serde_json::from_str(
            r#"{"showTrayIcon":false,"floatingBubbleEnabled":true,"floatingBubbleTrigger":"hover","floatingBubbleContent":"icon"}"#,
        )
        .expect("older v2 settings should deserialize");
        assert_eq!(settings.floating_bubble_scale, 1.0);
        assert_eq!(settings.theme_preset, ThemePreset::Default);
        assert_eq!(settings.zoom_factor, 1.0);
        assert!(!settings.show_compact_total_tokens);
        assert_eq!(settings.windows_backdrop, WindowsBackdrop::Acrylic);
        assert_eq!(settings.language, "auto");
    }

    #[test]
    fn language_is_allowlisted_and_defaults_to_system_auto() {
        let korean = normalize_settings(AppSettings {
            language: "ko".to_owned(),
            ..AppSettings::default()
        });
        assert_eq!(korean.language, "ko");
        let invalid = normalize_settings(AppSettings {
            language: "xx-test".to_owned(),
            ..AppSettings::default()
        });
        assert_eq!(invalid.language, "auto");
    }

    #[test]
    fn settings_store_persists_repeated_updates() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "token-lens-settings-{}-{unique}",
            std::process::id()
        ));
        let store = SettingsStore::load(dir.clone()).expect("settings store should load");
        store
            .update(SettingsPatch {
                floating_bubble_enabled: Some(true),
                ..SettingsPatch::default()
            })
            .expect("first settings update should persist");
        store
            .update(SettingsPatch {
                floating_bubble_trigger: Some(FloatingBubbleTrigger::Hover),
                floating_bubble_scale: Some(1.3),
                ..SettingsPatch::default()
            })
            .expect("second settings update should replace the same file");
        store
            .update_window_bounds(WindowBounds {
                x: 120,
                y: 80,
                width: 420.0,
                height: 760.0,
            })
            .expect("window bounds should persist alongside user settings");
        let reloaded = SettingsStore::load(dir.clone()).expect("settings should reload");
        let value = reloaded.get().expect("settings should be readable");
        assert!(value.floating_bubble_enabled);
        assert_eq!(value.floating_bubble_trigger, FloatingBubbleTrigger::Hover);
        assert_eq!(value.floating_bubble_scale, 1.3);
        assert_eq!(
            value.window_bounds,
            Some(WindowBounds {
                x: 120,
                y: 80,
                width: 420.0,
                height: 760.0,
            })
        );
        let _ = fs::remove_dir_all(dir);
    }
}
