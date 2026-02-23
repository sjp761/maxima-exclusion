use serde::{Deserialize, Deserializer, Serialize};
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

// The serializers are for making sure that None goes to and from an empty string

fn prefix_from_string<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(s)))
    }
}

fn prefix_to_string<S>(value: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        None => serializer.serialize_str(""),
        Some(path) => serializer.serialize_str(path.to_string_lossy().as_ref()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInstallInfo {
    pub path: PathBuf,
    #[serde(
        deserialize_with = "prefix_from_string",
        serialize_with = "prefix_to_string"
    )]
    pub wine_prefix: Option<PathBuf>,
}

impl GameInstallInfo {
    pub fn new(path: PathBuf, wine_prefix: Option<PathBuf>) -> Self {
        Self { path, wine_prefix }
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn wine_prefix(&self) -> Option<PathBuf> {
        self.wine_prefix.clone()
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
