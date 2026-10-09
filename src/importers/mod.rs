use anyhow::Result;
use std::path::Path;
use crate::{config::Config, store::Store};

pub struct ImportReport { pub conversations: usize, pub added: usize, pub updated: usize, pub unchanged: usize }

pub fn import_dump(_store: &Store, _cfg: &Config, source: &str, path: &Path) -> Result<ImportReport> { anyhow::bail!("import for {source} from {} not implemented yet", path.display()) }
