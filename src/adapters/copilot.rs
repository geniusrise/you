use super::scan::{find_message_arrays, parse_scanned, scan, ScannedFile};
use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use rusqlite::OpenFlags;
use std::path::PathBuf;

/// GitHub Copilot: (a) CLI/agent workspace sessions
/// (`copilot.cli.workspaceSessions.*.json` under VS Code/VSCodium
/// workspaceStorage) and (b) VS Code Copilot Chat sessions stored in
/// `state.vscdb` under `chat.ChatSessionStore.*` keys.
pub struct Copilot {
    pub storage_dirs: Vec<PathBuf>,
}

impl Copilot {
    pub fn new(storage_dirs: Vec<PathBuf>) -> Self {
        Self { storage_dirs }
    }

    fn scanned(&self) -> Vec<ScannedFile> {
        scan(&self.storage_dirs, |p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            name.starts_with("copilot.cli.workspaceSessions.") && name.ends_with(".json")
        })
    }

    /// Extract ChatSessionStore payloads from one state.vscdb.
    fn chat_store_sessions(&self, db: (String, PathBuf)) -> Option<Vec<Msg>> {
        let conn = rusqlite::Connection::open_with_flags(
            &db.1,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        let mut stmt = conn
            .prepare("SELECT key, value FROM ItemTable WHERE key LIKE 'chat.ChatSessionStore.%'")
            .ok()?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .ok()?;
        let mut msgs = Vec::new();
        let mut idx = 0usize;
        for (_k, v) in rows.flatten() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&v) {
                find_message_arrays(&v, &mut msgs, &db.0, &mut idx);
            }
        }
        Some(msgs)
    }
}

impl Adapter for Copilot {
    fn name(&self) -> &'static str {
        "copilot"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        let mut out: Vec<DiscoveredSession> = self
            .scanned()
            .into_iter()
            .map(|s| DiscoveredSession {
                key: s.key,
                fingerprint: s.fingerprint,
                meta_hint: Some((s.project, String::new())),
            })
            .collect();
        // ChatSessionStore-based sessions from state.vscdb, keyed `vscdb-<ws>`
        for (ws, db) in super::cursor::Cursor::new(self.storage_dirs.clone()).dbs() {
            let key = format!("vscdb-{ws}");
            let fp = super::cursor::Cursor::fingerprint(&db);
            out.push(DiscoveredSession { key, fingerprint: fp, meta_hint: None });
        }
        Ok(out)
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        if let Some(key) = key.strip_prefix("vscdb-") {
            let db = super::cursor::Cursor::new(self.storage_dirs.clone())
                .dbs()
                .into_iter()
                .find(|(k, _)| k == key)
                .map(|(_, db)| db)
                .ok_or_else(|| anyhow::anyhow!("copilot workspace {key} not found"))?;
            let msgs = self
                .chat_store_sessions((key.to_string(), db))
                .ok_or_else(|| anyhow::anyhow!("cannot open state.vscdb for {key}"))?;
            let title = msgs
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.text.chars().take(60).collect())
                .unwrap_or_else(|| key.to_string());
            let created = msgs.first().map(|m| m.ts_ms).unwrap_or(0);
            let updated = msgs.last().map(|m| m.ts_ms).unwrap_or(created);
            return Ok((
                SessionMeta {
                    source: "copilot".into(),
                    id: format!("vscdb-{key}"),
                    project: "default".into(),
                    title,
                    created_ms: created,
                    updated_ms: updated,
                },
                msgs,
            ));
        }
        let f = self
            .scanned()
            .into_iter()
            .find(|s| s.key == key)
            .ok_or_else(|| anyhow::anyhow!("copilot session {key} not found"))?;
        let (mut meta, msgs) = parse_scanned(&f.path, "copilot", key, "default")?;
        if f.project != "workspaceStorage" {
            meta.project = f.project;
        }
        Ok((meta, msgs))
    }
}
