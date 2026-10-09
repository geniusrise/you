use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use std::path::PathBuf;

pub struct OpenCode {
    pub db: PathBuf,
    pub storage: PathBuf,
}

impl OpenCode {
    pub fn new(db: PathBuf, storage: PathBuf) -> Self {
        Self { db, storage }
    }
}

impl Adapter for OpenCode {
    fn name(&self) -> &'static str {
        "opencode"
    }
    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(vec![])
    }
    fn read(&self, _key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        anyhow::bail!("not implemented")
    }
}
