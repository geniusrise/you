use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct Store {
    pub root: PathBuf,
}

pub fn default_root() -> PathBuf {
    if let Ok(p) = std::env::var("AIYOU_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".aiyou")
}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        Ok(Self { root: root.to_path_buf() })
    }

    pub fn init(&self) -> Result<()> {
        std::fs::create_dir_all(self.chats_dir())?;
        std::fs::create_dir_all(self.profile_dir())?;
        std::fs::create_dir_all(self.imports_dir())?;
        std::fs::create_dir_all(self.runs_dir())?;
        if !self.config_path().exists() {
            crate::config::Config::default().save(&self.config_path())?;
        }
        crate::git::git_init(&self.root)?;
        crate::git::git_commit_all(&self.root, "init: aiyou store")?;
        Ok(())
    }

    pub fn chats_dir(&self) -> PathBuf {
        self.root.join("chats")
    }

    pub fn profile_dir(&self) -> PathBuf {
        self.root.join("profile")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.profile_dir().join("runs")
    }

    pub fn imports_dir(&self) -> PathBuf {
        self.root.join("imports")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("aiyou.toml")
    }

    pub fn fingerprints_path(&self) -> PathBuf {
        self.root.join(".fingerprints.json")
    }

    pub fn traits_path(&self) -> PathBuf {
        self.profile_dir().join("traits.json")
    }

    pub fn skill_path(&self) -> PathBuf {
        self.profile_dir().join("SKILL.md")
    }

    pub fn session_path(&self, source: &str, id: &str, ext: &str) -> PathBuf {
        self.chats_dir().join(source).join(format!("{id}.{ext}"))
    }
}
