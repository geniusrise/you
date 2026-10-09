use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::Result;
use chrono::DateTime;
use std::path::PathBuf;

/// Google Antigravity CLI: per-agent "brain" conversations stored as one
/// JSON file per message under
/// `<dir>/brain/<uuid>/.system_generated/messages/*.json`.
/// Internal agent-to-agent traffic (hideFromUser) is filtered out.
pub struct Antigravity {
    pub dir: PathBuf,
}

fn iso_to_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

struct MsgFile {
    path: PathBuf,
    brain: String,
}

impl Antigravity {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn scan(&self) -> Vec<MsgFile> {
        if !self.dir.is_dir() {
            return vec![];
        }
        let mut out = Vec::new();
        for entry in walkdir::WalkDir::new(&self.dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if !entry.file_type().is_file() || p.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let is_msg = p
                .components()
                .any(|c| c.as_os_str().to_string_lossy() == "messages")
                && p.to_string_lossy().contains(".system_generated");
            if !is_msg {
                continue;
            }
            // brain dir = the first path component after `brain/`
            let brain = p
                .components()
                .collect::<Vec<_>>()
                .windows(2)
                .find(|w| w[0].as_os_str().to_string_lossy() == "brain")
                .map(|w| w[1].as_os_str().to_string_lossy().to_string())
                .unwrap_or_default();
            if brain.is_empty() {
                continue;
            }
            out.push(MsgFile { path: p.to_path_buf(), brain });
        }
        out.sort_by_key(|m| m.path.clone());
        out
    }

    fn brains(&self) -> Vec<(String, u64)> {
        let mut map: std::collections::BTreeMap<String, u64> = Default::default();
        for m in self.scan() {
            let fp = m
                .path
                .metadata()
                .ok()
                .and_then(|md| md.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let e = map.entry(m.brain).or_insert(0);
            *e = (*e).max(fp);
        }
        map.into_iter().collect()
    }
}

impl Adapter for Antigravity {
    fn name(&self) -> &'static str {
        "antigravity"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(self
            .brains()
            .into_iter()
            .map(|(brain, fp)| DiscoveredSession {
                key: brain,
                fingerprint: fp,
                meta_hint: None,
            })
            .collect())
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let files: Vec<MsgFile> = self.scan().into_iter().filter(|m| m.brain == key).collect();
        if files.is_empty() {
            anyhow::bail!("antigravity brain {key} not found");
        }
        let mut msgs = Vec::new();
        let mut first_ts: Option<i64> = None;
        let mut last_ts = 0i64;
        let mut idx = 0u64;
        for f in files {
            let Ok(raw) = std::fs::read_to_string(&f.path) else { continue };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };
            if v["hideFromUser"].as_bool().unwrap_or(false) {
                continue;
            }
            let text = v["content"]
                .as_str()
                .or_else(|| v["message"].as_str())
                .unwrap_or_default()
                .to_string();
            if text.trim().is_empty() {
                continue;
            }
            let sender = v["sender"].as_str().unwrap_or_default();
            let role = if sender.contains("user") { "user" } else { "assistant" };
            let ts = v["timestamp"].as_str().and_then(iso_to_ms).unwrap_or(last_ts);
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = last_ts.max(ts);
            let id = v["id"].as_str().map(|s| s.to_string()).unwrap_or_else(|| format!("{key}:{idx}"));
            let tool = if sender.contains("/task-") {
                Some("task".to_string())
            } else {
                None
            };
            msgs.push(Msg {
                id,
                role: role.to_string(),
                ts_ms: ts,
                text: text.chars().take(8000).collect(),
                tool,
            });
            idx += 1;
        }
        msgs.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
        let title = msgs
            .iter()
            .find(|m| m.role == "user")
            .map(|m| m.text.chars().take(60).collect())
            .unwrap_or_else(|| key.to_string());
        Ok((
            SessionMeta {
                source: "antigravity".into(),
                id: key.to_string(),
                project: "default".into(),
                title,
                created_ms: first_ts.unwrap_or(0),
                updated_ms: last_ts,
            },
            msgs,
        ))
    }
}
