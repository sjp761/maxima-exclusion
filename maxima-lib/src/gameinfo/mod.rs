use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::util::native::maxima_dir;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GameInfoError {
    #[error(transparent)]
    Native(#[from] crate::util::native::NativeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("game info not found for `{0}`")]
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

    pub fn save_to_json(&self, slug: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let path = maxima_dir().unwrap().join("gameinfo");
            if std::fs::create_dir_all(&path).is_ok() {
                let gameinfo_path = path.join(format!("{}.json", slug));
                fs::write(gameinfo_path, json).unwrap();
            }
        }
    }
}

pub fn load_game_info_from_json(slug: &str) -> Result<GameInstallInfo, GameInfoError> {
    let path = maxima_dir()?.join("gameinfo").join(format!("{}.json", slug));
    let json = fs::read_to_string(path)?;
    let game_install_info: GameInstallInfo = serde_json::from_str(&json)?;
    Ok(game_install_info)
}
