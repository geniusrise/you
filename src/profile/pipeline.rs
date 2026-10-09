use anyhow::Result;
use crate::{config::Config, store::Store};

pub struct Opts {
    pub if_changed: bool,
}

pub fn run(_store: &Store, _cfg: &Config, _opts: Opts) -> Result<()> {
    anyhow::bail!("profile pipeline not implemented yet")
}
