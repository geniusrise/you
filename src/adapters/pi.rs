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
}

impl Adapter for Pi {
    fn name(&self) -> &'static str {
        "pi"
    }
    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(vec![])
    }
    fn read(&self, _key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        anyhow::bail!("not implemented")
    }
}
