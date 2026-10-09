use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use rusqlite::OpenFlags;
use std::path::PathBuf;

pub struct OpenCode {
    pub db: PathBuf,
    pub storage: PathBuf,
}

impl OpenCode {
    pub fn new(db: PathBuf, storage: PathBuf) -> Self {
        Self { db, storage }
    }

    fn open_db(&self) -> Option<rusqlite::Connection> {
        if !self.db.is_file() {
            return None;
        }
        rusqlite::Connection::open_with_flags(
            &self.db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()
    }

    fn db_sessions(&self) -> Vec<DiscoveredSession> {
        let Some(conn) = self.open_db() else {
            return vec![];
        };
        let mut stmt = match conn.prepare(
            "SELECT s.id, MAX(m.time_updated), COUNT(m.id) FROM session s
             LEFT JOIN message m ON m.session_id = s.id
             GROUP BY s.id",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, i64>(2)?,
            ))
        });
        let mut out = Vec::new();
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                let (id, updated, count) = row;
                let fp = (updated as u64) ^ ((count as u64) << 40);
                out.push(DiscoveredSession { key: id, fingerprint: fp, meta_hint: None });
            }
        }
        out
    }

    fn db_read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let conn = self
            .open_db()
            .ok_or_else(|| anyhow::anyhow!("opencode db not available"))?;
        let (directory, title, created, updated): (String, String, i64, i64) = conn
            .query_row(
                "SELECT directory, title,
                        COALESCE(time_created, 0), COALESCE(time_updated, 0)
                 FROM session WHERE id = ?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|e| anyhow::anyhow!("session {key}: {e}"))?;
        let project = directory
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("default")
            .to_string();

        let mut stmt = conn.prepare(
            "SELECT p.id, p.message_id, p.time_created, p.data
             FROM part p JOIN message m ON m.id = p.message_id
             WHERE p.session_id = ?1
             ORDER BY p.time_created, p.id",
        )?;
        let rows = stmt.query_map([key], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, String>(3)?,
            ))
        })?;

        struct Pending {
            msg_id: String,
            role: String,
            ts: i64,
            text: String,
            tool: Option<String>,
        }
        let mut by_msg: Vec<Pending> = Vec::new();
        let mut msg_meta: Vec<(String, String, i64)> = Vec::new(); // id, role, ts
        {
            let mut stmt2 =
                conn.prepare("SELECT id, COALESCE(json_extract(data,'$.role'),''), COALESCE(time_created,0) FROM message WHERE session_id = ?1 ORDER BY time_created, id")?;
            for r in stmt2.query_map([key], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
            })?.flatten()
            {
                msg_meta.push(r);
            }
        }
        for (pid, mid, ts, data) in rows.flatten() {
            let v: serde_json::Value = match serde_json::from_str(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let _ = pid;
            let entry = by_msg.iter_mut().find(|p| p.msg_id == mid);
            let ptype = v["type"].as_str().unwrap_or_default();
            match ptype {
                "text" => {
                    let t = v["text"].as_str().unwrap_or_default().to_string();
                    if let Some(e) = entry {
                        if !e.text.is_empty() {
                            e.text.push('\n');
                        }
                        e.text.push_str(&t);
                    } else {
                        let role = msg_meta
                            .iter()
                            .find(|(id, _, _)| *id == mid)
                            .map(|(_, r, _)| r.clone())
                            .unwrap_or_default();
                        by_msg.push(Pending { msg_id: mid, role, ts, text: t, tool: None });
                    }
                }
                "tool" => {
                    let tool = v["tool"].as_str().unwrap_or("tool").to_string();
                    let state = serde_json::to_string_pretty(&v["state"]).unwrap_or_default();
                    let state: String = state.chars().take(2000).collect();
                    by_msg.push(Pending {
                        msg_id: format!("{mid}:tool"),
                        role: "tool".into(),
                        ts,
                        text: state,
                        tool: Some(tool),
                    });
                }
                _ => {}
            }
        }

        let mut msgs: Vec<Msg> = by_msg
            .into_iter()
            .map(|p| Msg {
                id: p.msg_id,
                role: if p.role.is_empty() { "assistant".into() } else { p.role },
                ts_ms: p.ts,
                text: p.text.chars().take(8000).collect(),
                tool: p.tool,
            })
            .collect();
        msgs.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
        let last = msgs.last().map(|m| m.ts_ms).unwrap_or(updated).max(updated.min(created.max(updated)));
        Ok((
            SessionMeta {
                source: "opencode".into(),
                id: key.to_string(),
                project,
                title,
                created_ms: created,
                updated_ms: last,
            },
            msgs,
        ))
    }

    fn storage_sessions(&self) -> Vec<DiscoveredSession> {
        let dir = self.storage.join("session");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return vec![];
        };
        let mut out = Vec::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                let fp = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                let key = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                out.push(DiscoveredSession { key, fingerprint: fp, meta_hint: None });
            }
        }
        out
    }

    fn storage_read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let raw = std::fs::read_to_string(self.storage.join("session").join(format!("{key}.json")))?;
        let v: serde_json::Value = serde_json::from_str(&raw)?;
        let directory = v["directory"].as_str().unwrap_or("default");
        let project = directory.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("default");
        let created = v["time"]["created"].as_i64().unwrap_or(0);
        let title = v["title"].as_str().unwrap_or(key).to_string();

        let mut msgs: Vec<Msg> = Vec::new();
        let msg_dir = self.storage.join("message");
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if !p.extension().is_some_and(|x| x == "json") {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(&p) else { continue };
                let Ok(mv) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };
                if mv["sessionID"].as_str() != Some(key) {
                    continue;
                }
                let ts = mv["time"]["created"].as_i64().unwrap_or(0);
                let role = mv["role"].as_str().unwrap_or("assistant").to_string();
                if let Some(parts) = mv["parts"].as_array() {
                    let mut text = String::new();
                    let mut tool: Option<String> = None;
                    let mut tool_text = String::new();
                    for part in parts {
                        match part["type"].as_str().unwrap_or_default() {
                            "text" => {
                                if !text.is_empty() {
                                    text.push('\n');
                                }
                                text.push_str(part["text"].as_str().unwrap_or_default());
                            }
                            "tool" => {
                                tool = Some(part["tool"].as_str().unwrap_or("tool").to_string());
                                tool_text =
                                    serde_json::to_string_pretty(&part["state"]).unwrap_or_default();
                            }
                            _ => {}
                        }
                    }
                    let id = mv["id"].as_str().unwrap_or_default().to_string();
                    if !text.is_empty() {
                        msgs.push(Msg {
                            id: id.clone(),
                            role: role.clone(),
                            ts_ms: ts,
                            text: text.chars().take(8000).collect(),
                            tool: None,
                        });
                    }
                    if let Some(t) = tool {
                        msgs.push(Msg {
                            id: format!("{id}:tool"),
                            role: "tool".into(),
                            ts_ms: ts,
                            text: tool_text.chars().take(2000).collect(),
                            tool: Some(t),
                        });
                    }
                }
            }
        }
        msgs.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
        let updated = msgs.last().map(|m| m.ts_ms).unwrap_or(created);
        Ok((
            SessionMeta {
                source: "opencode".into(),
                id: key.to_string(),
                project: project.to_string(),
                title,
                created_ms: created,
                updated_ms: updated,
            },
            msgs,
        ))
    }
}

impl Adapter for OpenCode {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        let db_list = self.db_sessions();
        if !db_list.is_empty() {
            return Ok(db_list);
        }
        Ok(self.storage_sessions())
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        if self.db.is_file() {
            return self.db_read(key);
        }
        self.storage_read(key)
    }
}
