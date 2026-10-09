use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use std::path::PathBuf;

pub struct GeminiCli {
    pub dir: PathBuf,
}

impl GeminiCli {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Adapter for GeminiCli {
    fn name(&self) -> &'static str {
        "gemini-cli"
    }
    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(vec![])
    }
    fn read(&self, _key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        anyhow::bail!("not implemented")
    }
}
