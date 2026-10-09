use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SOURCES: &[&str] = &[
    "claude-code",
    "opencode",
    "codex",
    "crush",
    "gemini-cli",
    "pi",
    "chatgpt",
    "claude-ai",
    "gemini-import",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcesCfg {
    #[serde(default = "default_enabled")]
    pub enabled: Vec<String>,
    pub claude_dir: Option<std::path::PathBuf>,
    pub opencode_db: Option<std::path::PathBuf>,
    pub opencode_storage: Option<std::path::PathBuf>,
    pub codex_dir: Option<std::path::PathBuf>,
    pub gemini_dir: Option<std::path::PathBuf>,
    pub crush_dirs: Option<Vec<std::path::PathBuf>>,
    pub pi_dir: Option<std::path::PathBuf>,
}

impl Default for SourcesCfg {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            claude_dir: None,
            opencode_db: None,
            opencode_storage: None,
            codex_dir: None,
            gemini_dir: None,
            crush_dirs: None,
            pi_dir: None,
        }
    }
}

fn default_enabled() -> Vec<String> {
    SOURCES.iter().map(|s| s.to_string()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileCfg {
    pub provider_base_url: String,
    pub api_key_env: String,
    pub model: String,
    pub workers: usize,
    pub map_context_chars: usize,
    #[serde(default)]
    pub exclude_sources: Vec<String>,
}

impl Default for ProfileCfg {
    fn default() -> Self {
        Self {
            provider_base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            model: "gpt-5.2".into(),
            workers: 4,
            map_context_chars: 120_000,
            exclude_sources: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub sources: SourcesCfg,
    #[serde(default = "yes")]
    pub redact_markdown: bool,
    #[serde(default)]
    pub redact_all: bool,
    #[serde(default)]
    pub profile: ProfileCfg,
}

fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sources: SourcesCfg::default(),
            redact_markdown: true,
            redact_all: false,
            profile: ProfileCfg::default(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&raw).with_context(|| format!("parsing config {}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let raw = toml::to_string_pretty(self).context("serializing config")?;
        std::fs::write(path, raw).with_context(|| format!("writing config {}", path.display()))
    }
}
