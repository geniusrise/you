use crate::adapters::Adapter;
use anyhow::Result;
use crate::store::Store;
use crate::config::Config;

pub struct SyncReport { pub per_source: Vec<(String, usize, usize, usize)> }

pub fn run(_store: &Store, _cfg: &Config, _only: Option<&[String]>) -> Result<SyncReport> { unimplemented!() }
pub fn run_with_adapters(_s: &Store, _c: &Config, _a: Vec<Box<dyn Adapter>>) -> Result<SyncReport> { unimplemented!() }
