use super::scan::{parse_scanned, scan, ScannedFile};
use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use std::path::PathBuf;

pub struct Pi {
    pub dir: PathBuf,
}

impl Pi {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn scanned(&self) -> Vec<ScannedFile> {
        scan(&[self.dir.clone()], |p| {
            p.extension().is_some_and(|e| e == "json" || e == "jsonl")
        })
    }
}

impl Adapter for Pi {
    fn name(&self) -> &'static str {
        "pi"
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
            .ok_or_else(|| anyhow::anyhow!("pi session {key} not found"))?;
        let mut r = parse_scanned(&f.path, "pi", key, &f.project)?;
        if f.project != "sessions" {
            r.0.project = f.project;
        }
        Ok(r)
    }
}
