use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use anyhow::Result;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub default_batch_size: Option<usize>,
    pub default_album_id: Option<String>,
    pub default_album_name: Option<String>,
    pub preprocess_videos: Option<bool>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            default_batch_size: Some(1),
            default_album_id: None,
            default_album_name: None,
            preprocess_videos: Some(true),
        }
    }
}

#[derive(Clone, Copy)]
enum ConfigKey {
    DefaultBatchSize,
    DefaultAlbumId,
    DefaultAlbumName,
    PreprocessVideos,
}

impl ConfigKey {
    fn as_str(&self) -> &str {
        match self {
            ConfigKey::DefaultBatchSize => "default_batch_size",
            ConfigKey::DefaultAlbumId => "default_album_id",
            ConfigKey::DefaultAlbumName => "default_album_name",
            ConfigKey::PreprocessVideos => "preprocess_videos",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "default_batch_size" => Some(ConfigKey::DefaultBatchSize),
            "default_album_id" => Some(ConfigKey::DefaultAlbumId),
            "default_album_name" => Some(ConfigKey::DefaultAlbumName),
            "preprocess_videos" => Some(ConfigKey::PreprocessVideos),
            _ => None,
        }
    }

    fn get(&self, config: &Config) -> String {
        match self {
            ConfigKey::DefaultBatchSize => config.default_batch_size.map(|v| v.to_string()).unwrap_or_else(|| "1".to_string()),
            ConfigKey::DefaultAlbumId => config.default_album_id.clone().unwrap_or_else(|| "none".to_string()),
            ConfigKey::DefaultAlbumName => config.default_album_name.clone().unwrap_or_else(|| "none".to_string()),
            ConfigKey::PreprocessVideos => config.preprocess_videos.map(|v| v.to_string()).unwrap_or_else(|| "true".to_string()),
        }
    }

    fn set(&self, config: &mut Config, value: &str) -> Result<()> {
        match self {
            ConfigKey::DefaultBatchSize => {
                config.default_batch_size = Some(value.parse()?);
            }
            ConfigKey::DefaultAlbumId => {
                config.default_album_id = if value == "none" { None } else { Some(value.to_string()) };
            }
            ConfigKey::DefaultAlbumName => {
                config.default_album_name = if value == "none" { None } else { Some(value.to_string()) };
            }
            ConfigKey::PreprocessVideos => {
                config.preprocess_videos = Some(value.parse()?);
            }
        }
        Ok(())
    }

    fn default(&self) -> String {
        match self {
            ConfigKey::DefaultBatchSize => "1".to_string(),
            ConfigKey::DefaultAlbumId => "none".to_string(),
            ConfigKey::DefaultAlbumName => "none".to_string(),
            ConfigKey::PreprocessVideos => "true".to_string(),
        }
    }

    const fn all() -> &'static [Self] {
        &[
            ConfigKey::DefaultBatchSize,
            ConfigKey::DefaultAlbumId,
            ConfigKey::DefaultAlbumName,
            ConfigKey::PreprocessVideos,
        ]
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();
        if config_path.exists() {
            let content = fs::read_to_string(config_path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    pub fn get_value(&self, key: &str) -> String {
        if let Some(k) = ConfigKey::from_str(key) {
            k.get(self)
        } else {
            "unknown key".to_string()
        }
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<()> {
        if let Some(k) = ConfigKey::from_str(key) {
            k.set(self, value)
        } else {
            Err(anyhow::anyhow!("Unknown key: {}", key))
        }
    }

    pub fn print_all(&self) {
        println!("Key                    Value     | Default");
        println!("─────────────────────────────────────────");
        for key in ConfigKey::all() {
            let current = key.get(self);
            let default = key.default();
            println!("{:<22} {:<9} | \x1b[3m{}\x1b[0m", key.as_str(), current, default);
        }
    }

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bunkr_uploader.toml")
    }
}
