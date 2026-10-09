use anyhow::Result;
use std::path::Path;
use crate::store::Store;

pub fn install(_store: &Store, _target: Option<&str>, _agents_dir: Option<&Path>) -> Result<()> {
    anyhow::bail!("skill install not implemented yet")
}
