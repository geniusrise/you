use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use std::path::PathBuf;

pub struct Codex {
    pub dir: PathBuf,
}

impl Codex {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Adapter for Codex {
    fn name(&self) -> &'static str {
        "codex"
    }
    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(vec![])
    }
    fn read(&self, _key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        anyhow::bail!("not implemented")
    }
}
