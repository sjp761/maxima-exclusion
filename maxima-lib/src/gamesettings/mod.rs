use std::collections::HashMap;

use crate::util::native::maxima_dir;
use log::info;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize, Clone)]
pub struct GameSettings {
    pub cloud_saves: bool,
    pub installed: bool,
    pub launch_args: String,
    pub exe_override: String,
}

impl GameSettings {
    pub fn new() -> Self {
        Self {
            cloud_saves: true,
            installed: false,
            launch_args: String::new(),
            exe_override: String::new(),
        }
    }

    /// Public accessors for fields so consumers can read settings.
    pub fn cloud_saves(&self) -> bool {
        self.cloud_saves
    }

    pub fn launch_args(&self) -> &str {
        &self.launch_args
    }

    pub fn exe_override(&self) -> &str {
        &self.exe_override
    }

    /// Update mutable fields from UI-provided values while preserving any internal-only fields like `wine_prefix`.
    pub fn update_from(&mut self, cloud_saves: bool, launch_args: String, exe_override: String) {
        self.cloud_saves = cloud_saves;
        self.launch_args = launch_args;
        self.exe_override = exe_override;
    }
}

pub fn get_game_settings(slug: &str) -> GameSettings {
    let path = match maxima_dir() {
        Ok(dir) => dir.join("settings").join(format!("{}.json", slug)),
        Err(_) => return GameSettings::new(),
    };

    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return GameSettings::new(),
    };

    let game_settings = serde_json::from_str(&content).unwrap_or_else(|_| GameSettings::new());
    game_settings
}

pub fn save_game_settings(slug: &str, settings: &GameSettings) {
    if settings.installed == false {
        info!("Skipping save for {} as game is not installed.", slug);
        return;
    }
    info!("Saving settings for {}...", slug);
    if let Ok(dir) = maxima_dir() {
        let settings_dir = dir.join("settings");
        // Ensure the settings directory exists
        if let Err(err) = std::fs::create_dir_all(&settings_dir) {
            info!("Failed to create settings dir {:?}: {}", settings_dir, err);
            return;
        }

        let path = settings_dir.join(format!("{}.json", slug));
        if let Ok(content) = serde_json::to_string_pretty(settings) {
            match std::fs::write(&path, content) {
                Ok(()) => info!("Saved settings to {:?}", path),
                Err(err) => info!("Failed to write settings for {}: {}", slug, err),
            }
        } else {
            info!("Failed to serialize settings for {}", slug);
        }
    } else {
        info!(
            "Failed to get maxima directory, cannot save settings for {}",
            slug
        );
    }
}

#[derive(Clone)]
pub struct GameSettingsManager {
    settings: HashMap<String, GameSettings>,
}

impl GameSettingsManager {
    pub fn new() -> Self {
        Self {
            settings: HashMap::new(),
        }
    }

    pub fn get(&self, slug: &str) -> GameSettings {
        self.settings
            .get(slug)
            .cloned()
            .unwrap_or_else(|| GameSettings::new())
    }

    pub fn save(&mut self, slug: &str, settings: GameSettings) {
        save_game_settings(slug, &settings);
        self.settings.insert(slug.to_string(), settings.clone());
    }
}
