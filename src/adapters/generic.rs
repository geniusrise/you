use super::scan::{parse_scanned, scan};
use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use std::path::PathBuf;

/// Generic scan-based adapter for harnesses with simple session files
/// (JSON with a messages array, or JSONL with one message per line).
/// Instantiated for: continue, amp, zed.
pub struct GenericScan {
    pub source: &'static str,
    pub dirs: Vec<PathBuf>,
}

impl GenericScan {
    pub fn new(source: &'static str, dirs: Vec<PathBuf>) -> Self {
        Self { source, dirs }
    }
}

impl Adapter for GenericScan {
    fn name(&self) -> &'static str {
        self.source
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(scan(&self.dirs, |p| {
            p.extension().is_some_and(|e| e == "json" || e == "jsonl")
        })
        .into_iter()
        .map(|s| DiscoveredSession {
            key: s.key,
            fingerprint: s.fingerprint,
            meta_hint: Some((s.project, String::new())),
        })
        .collect())
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let f = super::scan::scan(&self.dirs, |p| {
            p.extension().is_some_and(|e| e == "json" || e == "jsonl")
        })
        .into_iter()
        .find(|s| s.key == key)
        .ok_or_else(|| anyhow::anyhow!("{} session {key} not found", self.source))?;
        let (mut meta, msgs) = parse_scanned(&f.path, self.source, key, &f.project)?;
        if meta.title.is_empty() || meta.title == key {
            meta.title = msgs
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.text.chars().take(60).collect())
                .unwrap_or_else(|| key.to_string());
        }
        Ok((meta, msgs))
    }
}
