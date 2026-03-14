use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub stt: SttConfig,
    pub tts: TtsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub listen: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SttConfig {
    pub model_path: PathBuf,
    pub language: String,
    pub use_gpu: bool,
    pub flash_attn: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    pub model: String,
    pub voices_dir: Option<PathBuf>,
    pub default_voice: String,
    pub synthesis_timeout_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            stt: SttConfig::default(),
            tts: TtsConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: "0.0.0.0:9200".to_string(),
        }
    }
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("/models/ggml-large-v3-turbo.bin"),
            language: "auto".to_string(),
            use_gpu: true,
            flash_attn: true,
        }
    }
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            model: "custom-voice".to_string(),
            voices_dir: None,
            default_voice: "ryan".to_string(),
            synthesis_timeout_ms: 90_000,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
