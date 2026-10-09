use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use std::path::PathBuf;

pub struct ClaudeCode {
    pub dir: PathBuf,
}

impl ClaudeCode {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Adapter for ClaudeCode {
    fn name(&self) -> &'static str {
        "claude-code"
    }
    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(vec![])
    }
    fn read(&self, _key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        anyhow::bail!("not implemented")
    }
}
