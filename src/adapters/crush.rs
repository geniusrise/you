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
}

impl Adapter for Crush {
    fn name(&self) -> &'static str {
        "crush"
    }
    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(vec![])
    }
    fn read(&self, _key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        anyhow::bail!("not implemented")
    }
}
