use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,

    #[serde(default)]
    pub llm: LlmConfig,

    #[serde(default)]
    pub scanner: ScannerConfig,

    #[serde(default)]
    pub preview: PreviewConfig,

    #[serde(default)]
    pub trash: TrashConfig,

    #[serde(default)]
    pub thumbnails: ThumbnailConfig,

    #[serde(default)]
    pub schedule: ScheduleConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashConfig {
    #[serde(default = "default_trash_path")]
    pub path: PathBuf,

    #[serde(default = "default_max_age_days")]
    pub max_age_days: u32,

    #[serde(default = "default_max_size_bytes")]
    pub max_size_bytes: u64,
}

fn default_trash_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(".local/share"))
        .join("clepho/.trash")
}

fn default_max_age_days() -> u32 {
    30
}

fn default_max_size_bytes() -> u64 {
    1024 * 1024 * 1024 // 1GB
}

impl Default for TrashConfig {
    fn default() -> Self {
        Self {
            path: default_trash_path(),
            max_age_days: default_max_age_days(),
            max_size_bytes: default_max_size_bytes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailConfig {
    #[serde(default = "default_thumb_cache_path")]
    pub path: PathBuf,

    #[serde(default = "default_thumb_cache_size")]
    pub size: u32,
}

fn default_thumb_cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("clepho/thumbnails")
}

fn default_thumb_cache_size() -> u32 {
    256
}

impl Default for ThumbnailConfig {
    fn default() -> Self {
        Self {
            path: default_thumb_cache_path(),
            size: default_thumb_cache_size(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Whether to check for overdue schedules on startup.
    #[serde(default = "default_check_overdue_on_startup")]
    pub check_overdue_on_startup: bool,

    /// Default start hour for hours of operation (0-23).
    #[serde(default)]
    pub default_hours_start: Option<u8>,

    /// Default end hour for hours of operation (0-23).
    #[serde(default)]
    pub default_hours_end: Option<u8>,
}

fn default_check_overdue_on_startup() -> bool {
    true
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            check_overdue_on_startup: default_check_overdue_on_startup(),
            default_hours_start: None,
            default_hours_end: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProviderType {
    #[default]
    LmStudio,
    OpenAI,
    Anthropic,
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: LlmProviderType,

    #[serde(default = "default_llm_endpoint")]
    pub endpoint: String,

    #[serde(default = "default_llm_model")]
    pub model: String,

    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannerConfig {
    #[serde(default = "default_image_extensions")]
    pub image_extensions: Vec<String>,

    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageProtocol {
    #[default]
    Auto,
    Sixel,
    Kitty,
    ITerm2,
    Halfblocks,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewConfig {
    #[serde(default = "default_preview_enabled")]
    pub image_preview: bool,

    #[serde(default)]
    pub protocol: ImageProtocol,

    #[serde(default = "default_thumbnail_size")]
    pub thumbnail_size: u32,

    /// External viewer application for right-click open (e.g., "feh", "eog", "gimp")
    /// If not set, uses system default (xdg-open on Linux, open on macOS)
    #[serde(default)]
    pub external_viewer: Option<String>,
}

fn default_preview_enabled() -> bool {
    true
}

fn default_thumbnail_size() -> u32 {
    1024
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            image_preview: default_preview_enabled(),
            protocol: ImageProtocol::default(),
            thumbnail_size: default_thumbnail_size(),
            external_viewer: None,
        }
    }
}

fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("clepho")
        .join("clepho.db")
}

fn default_llm_endpoint() -> String {
    "http://127.0.0.1:1234/v1".to_string()
}

fn default_llm_model() -> String {
    "gemma-3-4b".to_string()
}

fn default_image_extensions() -> Vec<String> {
    vec![
        "jpg".to_string(),
        "jpeg".to_string(),
        "png".to_string(),
        "gif".to_string(),
        "webp".to_string(),
        "heic".to_string(),
        "heif".to_string(),
        "raw".to_string(),
        "cr2".to_string(),
        "nef".to_string(),
        "arw".to_string(),
    ]
}

fn default_similarity_threshold() -> u32 {
    50 // Hamming distance threshold for perceptual hash similarity (~20% of 256 bits)
       // Higher values catch more edited versions (borders, contrast) but may have false positives
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            image_extensions: default_image_extensions(),
            similarity_threshold: default_similarity_threshold(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            llm: LlmConfig::default(),
            scanner: ScannerConfig::default(),
            preview: PreviewConfig::default(),
            trash: TrashConfig::default(),
            thumbnails: ThumbnailConfig::default(),
            schedule: ScheduleConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Create default config
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path();

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;

        Ok(())
    }

    fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("clepho")
            .join("config.toml")
    }
}
