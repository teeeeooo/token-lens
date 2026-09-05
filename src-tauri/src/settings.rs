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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub floating_bubble_enabled: bool,
    pub floating_bubble_trigger: FloatingBubbleTrigger,
    pub floating_bubble_content: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            floating_bubble_enabled: false,
            floating_bubble_trigger: FloatingBubbleTrigger::Click,
            floating_bubble_content: "icon".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub floating_bubble_enabled: Option<bool>,
    pub floating_bubble_trigger: Option<FloatingBubbleTrigger>,
    pub floating_bubble_content: Option<String>,
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

    pub fn update(&self, patch: SettingsPatch) -> Result<AppSettings, String> {
        let mut value = self
            .value
            .lock()
            .map_err(|_| "settings state lock was poisoned".to_owned())?;
        if let Some(enabled) = patch.floating_bubble_enabled {
            value.floating_bubble_enabled = enabled;
        }
        if let Some(trigger) = patch.floating_bubble_trigger {
            value.floating_bubble_trigger = trigger;
        }
        if let Some(content) = patch.floating_bubble_content {
            value.floating_bubble_content = content;
        }
        *value = normalize_settings(value.clone());
        write_settings(&self.path, &value)?;
        Ok(value.clone())
    }
}

fn normalize_settings(mut value: AppSettings) -> AppSettings {
    if value.floating_bubble_content != "icon" {
        value.floating_bubble_content = "icon".to_owned();
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
    fn defaults_keep_floating_bubble_disabled_and_icon_only() {
        let settings = AppSettings::default();
        assert!(!settings.floating_bubble_enabled);
        assert_eq!(
            settings.floating_bubble_trigger,
            FloatingBubbleTrigger::Click
        );
        assert_eq!(settings.floating_bubble_content, "icon");
    }

    #[test]
    fn normalization_rejects_unimplemented_bubble_content() {
        let normalized = normalize_settings(AppSettings {
            floating_bubble_content: "tokens".to_owned(),
            ..AppSettings::default()
        });
        assert_eq!(normalized.floating_bubble_content, "icon");
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
                ..SettingsPatch::default()
            })
            .expect("second settings update should replace the same file");
        let reloaded = SettingsStore::load(dir.clone()).expect("settings should reload");
        let value = reloaded.get().expect("settings should be readable");
        assert!(value.floating_bubble_enabled);
        assert_eq!(value.floating_bubble_trigger, FloatingBubbleTrigger::Hover);
        let _ = fs::remove_dir_all(dir);
    }
}
