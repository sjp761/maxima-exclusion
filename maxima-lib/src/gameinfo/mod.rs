use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::util::native::maxima_dir;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GameVersionError {
    #[error(transparent)]
    Native(#[from] crate::util::native::NativeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("game version info not found for `{0}`")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstallInfo {
    pub path: String,
    pub wine_prefix: String,
}

impl GameInstallInfo {
    pub fn new(path: String) -> Self {
        Self {
            path,
            wine_prefix: String::new(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn wine_prefix(&self) -> &str {
        &self.wine_prefix
    }

    pub fn install_path_pathbuf(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    pub fn wine_prefix_pathbuf(&self) -> PathBuf {
        PathBuf::from(&self.wine_prefix)
    }

    // TODO: Maybe we can just query the slug by the filename of the path? Look into this later
    pub fn save_to_json(&self, slug: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let mut path = maxima_dir();
            path.as_mut().unwrap().push("gameinfo");
            if let Ok(_) = std::fs::create_dir_all(&path.as_ref().unwrap()) {
                path.as_mut().unwrap().push(format!("{}.json", slug));
                fs::write(path.unwrap(), json).unwrap();
            }
        }
    }
}
pub fn load_game_version_from_json(slug: &str) -> Result<GameInstallInfo, GameVersionError> {
    let mut path = maxima_dir();
    path.as_mut().unwrap().push("gameinfo");
    path.as_mut().unwrap().push(format!("{}.json", slug));
    let json = fs::read_to_string(path.unwrap())?;
    let game_install_info: GameInstallInfo = serde_json::from_str(&json)?;
    Ok(game_install_info)
}
