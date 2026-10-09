use crate::model::SessionMeta;
use crate::store::Store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CATEGORIES: &[&str] = &[
    "personality",
    "communication",
    "preferences",
    "workflow",
    "tech-stack",
    "values",
    "goals",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    pub source: String,
    pub session: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trait {
    pub id: String,
    pub statement: String,
    pub category: String,
    pub confidence: f32,
    pub evidence: Vec<Evidence>,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileDoc {
    pub updated_at_ms: i64,
    pub traits: Vec<Trait>,
}

impl ProfileDoc {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Post-filter and cap per spec: drop confidence < 0.3, cap 100 traits
    /// sorted by confidence weighted by recency.
    pub fn normalize(&mut self, now_ms: i64) {
        const DAY_MS: i64 = 86_400_000;
        self.traits.retain(|t| t.confidence >= 0.3 && CATEGORIES.contains(&t.category.as_str()));
        self.traits.sort_by(|a, b| {
            let rec = |t: &Trait| 1.0 + ((t.last_seen_ms.max(t.first_seen_ms).max(0)) as f64 / DAY_MS as f64 / 365.0);
            let sa = (a.confidence as f64) * rec(a);
            let sb = (b.confidence as f64) * rec(b);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        self.traits.truncate(100);
        self.updated_at_ms = now_ms;
    }
}

pub fn trait_id(statement: &str) -> String {
    crate::render::slug(statement)
}

/// The `.md` path + parsed meta used for selecting incremental inputs.
#[derive(Clone)]
pub struct ChatRef {
    pub path: std::path::PathBuf,
    pub meta: SessionMeta,
}

pub fn select_chats(store: &Store, exclude_sources: &[String]) -> Result<Vec<ChatRef>> {
    let all = store.list_sessions()?;
    Ok(all
        .into_iter()
        .filter(|(_, m)| !exclude_sources.contains(&m.source))
        .map(|(path, meta)| ChatRef { path, meta })
        .collect())
}

/// Batch chats into groups of ~`target_chars` total characters.
pub fn batch_chats(chats: Vec<ChatRef>, target_chars: usize) -> Vec<Vec<ChatRef>> {
    let mut batches: Vec<Vec<ChatRef>> = Vec::new();
    let mut current: Vec<ChatRef> = Vec::new();
    let mut size = 0usize;
    for chat in chats {
        let len = chat.path.metadata().map(|m| m.len() as usize).unwrap_or(0);
        if !current.is_empty() && size + len > target_chars.max(len) {
            batches.push(std::mem::take(&mut current));
            size = 0;
        }
        size += len;
        current.push(chat);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}
