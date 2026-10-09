use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use rusqlite::OpenFlags;
use std::path::PathBuf;

/// Block's Goose agent: sessions + messages in a SQLite db
/// (default ~/.local/share/goose/sessions/sessions.db).
pub struct Goose {
    pub db: PathBuf,
}

impl Goose {
    pub fn new(db: PathBuf) -> Self {
        Self { db }
    }

    fn open(&self) -> Option<rusqlite::Connection> {
        if !self.db.is_file() {
            return None;
        }
        rusqlite::Connection::open_with_flags(
            &self.db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()
    }
}

/// Goose timestamps are seconds; normalize to millis.
fn to_ms(t: i64) -> i64 {
    if t > 0 && t < 10_000_000_000 {
        t * 1000
    } else {
        t
    }
}

fn parts_text(content_json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(content_json)
        .ok()
        .and_then(|v| {
            v.as_array().map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        })
        .unwrap_or_default()
}

impl Adapter for Goose {
    fn name(&self) -> &'static str {
        "goose"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        let Some(conn) = self.open() else {
            return Ok(vec![]);
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT s.id, COUNT(m.id), COALESCE(MAX(m.created_timestamp), 0)
             FROM sessions s LEFT JOIN messages m ON m.session_id = s.id
             GROUP BY s.id",
        ) else {
            return Ok(vec![]);
        };
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows.flatten() {
            let (id, count, updated) = row;
            out.push(DiscoveredSession {
                key: id,
                fingerprint: (updated as u64) ^ ((count as u64) << 40),
                meta_hint: None,
            });
        }
        Ok(out)
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let conn = self
            .open()
            .ok_or_else(|| anyhow::anyhow!("goose db not available"))?;
        let (name, working_dir, created, updated): (String, String, i64, i64) = conn.query_row(
            "SELECT COALESCE(NULLIF(name,''), id), working_dir,
                    CAST(strftime('%s', created_at) AS INTEGER), CAST(strftime('%s', updated_at) AS INTEGER)
             FROM sessions WHERE id = ?1",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let project = working_dir
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("default")
            .to_string();

        let mut stmt = conn.prepare(
            "SELECT COALESCE(message_id, id), role, content_json, created_timestamp
             FROM messages WHERE session_id = ?1 ORDER BY created_timestamp, id",
        )?;
        let rows = stmt.query_map([key], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut msgs = Vec::new();
        for (id, role, content_json, ts) in rows.flatten() {
            let text = parts_text(&content_json);
            if text.trim().is_empty() {
                continue;
            }
            msgs.push(Msg {
                id: format!("{key}:{id}"),
                role,
                ts_ms: to_ms(ts),
                text: text.chars().take(8000).collect(),
                tool: None,
            });
        }
        Ok((
            SessionMeta {
                source: "goose".into(),
                id: key.to_string(),
                project,
                title: name.chars().take(80).collect(),
                created_ms: to_ms(created),
                updated_ms: to_ms(updated),
            },
            msgs,
        ))
    }
}
