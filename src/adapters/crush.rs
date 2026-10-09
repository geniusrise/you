use super::scan::{parse_scanned, scan, ScannedFile};
use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use std::path::PathBuf;

pub struct Crush {
    pub dirs: Vec<PathBuf>,
}

impl Crush {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { dirs }
    }

    fn scanned(&self) -> Vec<ScannedFile> {
        scan(&self.dirs, |p| {
            p.extension().is_some_and(|e| e == "json" || e == "jsonl")
        })
    }
}

impl Adapter for Crush {
    fn name(&self) -> &'static str {
        "crush"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(self
            .scanned()
            .into_iter()
            .map(|s| DiscoveredSession {
                key: s.key,
                fingerprint: s.fingerprint,
                meta_hint: Some((s.project, String::new())),
            })
            .collect())
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let f = self
            .scanned()
            .into_iter()
            .find(|s| s.key == key)
            .ok_or_else(|| anyhow::anyhow!("crush session {key} not found"))?;
        let mut r = parse_scanned(&f.path, "crush", key, &f.project)?;
        // sessions may carry their own project dir; scanned project (parent dir name) wins if it isn't "sessions"
        if f.project != "sessions" {
            r.0.project = f.project;
        }
        Ok(r)
    }
}
