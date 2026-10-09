use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use rusqlite::OpenFlags;
use std::path::PathBuf;

/// Cursor IDE: composer/aichat data in `User/workspaceStorage/<hash>/state.vscdb`
/// (ItemTable key/value). Only Cursor-specific keys are read, so plain VS Code
/// workspaces in the same storage tree are ignored.
pub struct Cursor {
    pub storage_dirs: Vec<PathBuf>,
}

impl Cursor {
    pub fn new(storage_dirs: Vec<PathBuf>) -> Self {
        Self { storage_dirs }
    }

    pub(crate) fn dbs(&self) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        for dir in &self.storage_dirs {
            if !dir.is_dir() {
                continue;
            }
            for entry in walkdir::WalkDir::new(dir)
                .max_depth(2)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file()
                    && entry.file_name().to_string_lossy() == "state.vscdb"
                {
                    let key = entry
                        .path()
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "ws".into());
                    out.push((key, entry.path().to_path_buf()));
                }
            }
        }
        out
    }

    pub(crate) fn open(db: &PathBuf) -> Option<rusqlite::Connection> {
        rusqlite::Connection::open_with_flags(
            db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()
    }

    pub(crate) fn fingerprint(db: &PathBuf) -> u64 {
        db.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}

impl Adapter for Cursor {
    fn name(&self) -> &'static str {
        "cursor"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(self
            .dbs()
            .into_iter()
            .map(|(key, db)| DiscoveredSession {
                key,
                fingerprint: Cursor::fingerprint(&db),
                meta_hint: None,
            })
            .collect())
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let db = self
            .dbs()
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, db)| db)
            .ok_or_else(|| anyhow::anyhow!("cursor workspace {key} not found"))?;
        let conn = Cursor::open(&db).ok_or_else(|| anyhow::anyhow!("cannot open {}", db.display()))?;
        let mut stmt = conn.prepare(
            "SELECT key, value FROM ItemTable
             WHERE key LIKE '%composer%' OR key LIKE '%aichat%'",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut msgs: Vec<Msg> = Vec::new();
        let mut idx = 0usize;
        for (_k, v) in rows.flatten() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v) {
                super::scan::find_message_arrays(&v, &mut msgs, key, &mut idx);
            }
        }
        msgs.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
        let title = msgs
            .iter()
            .find(|m| m.role == "user")
            .map(|m| m.text.chars().take(60).collect())
            .unwrap_or_else(|| key.to_string());
        let created = msgs.first().map(|m| m.ts_ms).unwrap_or(0);
        let updated = msgs.last().map(|m| m.ts_ms).unwrap_or(created);
        Ok((
            SessionMeta {
                source: "cursor".into(),
                id: key.to_string(),
                project: "default".into(),
                title,
                created_ms: created,
                updated_ms: updated,
            },
            msgs,
        ))
    }
}
