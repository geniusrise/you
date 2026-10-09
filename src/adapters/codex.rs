use super::{Adapter, DiscoveredSession};
use anyhow::Result as _;
use crate::model::{Msg, SessionMeta};
use anyhow::{Context, Result};
use chrono::DateTime;
use std::path::PathBuf;

pub struct Codex {
    pub dir: PathBuf,
}

fn iso_to_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

/// Generic file-walking discovery shared by jsonl-based adapters.
pub(crate) fn walk_jsonl(
    dir: &std::path::Path,
    source: &'static str,
    project_from: fn(&std::path::Path) -> String,
) -> Vec<DiscoveredSession> {
    if !dir.is_dir() {
        return vec![];
    }
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "jsonl") {
            let md = entry.metadata().ok();
            let mtime = md
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
            let _ = source;
            out.push(DiscoveredSession {
                key: entry.path().file_stem().unwrap_or_default().to_string_lossy().to_string(),
                fingerprint: mtime ^ (size << 1),
                meta_hint: Some((project_from(entry.path()), String::new())),
            });
        }
    }
    out
}

/// Find the jsonl file whose stem == key under dir.
pub(crate) fn find_by_stem(dir: &std::path::Path, key: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| {
            e.file_type().is_file()
                && e.path().extension().is_some_and(|x| x == "jsonl")
                && e.path().file_stem().is_some_and(|s| s == key)
        })
        .map(|e| e.path().to_path_buf())
}

fn content_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                let t = p["text"].as_str()?;
                Some(t.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

impl Codex {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Adapter for Codex {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        Ok(walk_jsonl(&self.dir, "codex", |p| {
            // path: <dir>/YYYY/MM/DD/rollout-...jsonl — no project in path;
            // project comes from session_meta at read time.
            let _ = p;
            "default".to_string()
        }))
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let path = find_by_stem(&self.dir, key)
            .with_context(|| format!("codex session {key} not found"))?;
        let raw = std::fs::read_to_string(&path)?;
        let mut msgs: Vec<Msg> = Vec::new();
        let mut project = "default".to_string();
        let mut first_ts: Option<i64> = None;
        let mut last_ts = 0i64;
        let mut idx = 0u64;

        for line in raw.lines() {
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let ts = v["timestamp"].as_str().and_then(iso_to_ms).unwrap_or(last_ts);
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = last_ts.max(ts);
            let ty = v["type"].as_str().unwrap_or_default();
            let payload = &v["payload"];
            match ty {
                "session_meta" => {
                    let cwd = payload["cwd"].as_str().unwrap_or("default");
                    project = cwd.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("default").to_string();
                }
                "response_item" => {
                    let ptype = payload["type"].as_str().unwrap_or_default();
                    match ptype {
                        "message" => {
                            let role = payload["role"].as_str().unwrap_or("assistant");
                            let text = content_text(&payload["content"]);
                            if !text.is_empty() {
                                msgs.push(Msg {
                                    id: format!("{key}:{idx}"),
                                    role: role.to_string(),
                                    ts_ms: ts,
                                    text: text.chars().take(8000).collect(),
                                    tool: None,
                                });
                            }
                        }
                        "function_call" => {
                            let name = payload["name"].as_str().unwrap_or("tool").to_string();
                            let args = payload["arguments"].as_str().unwrap_or_default().to_string();
                            msgs.push(Msg {
                                id: format!("{key}:{idx}:tool"),
                                role: "tool".into(),
                                ts_ms: ts,
                                text: args.chars().take(2000).collect(),
                                tool: Some(name),
                            });
                        }
                        "function_call_output" => {
                            let out = payload["output"].as_str().unwrap_or_default().to_string();
                            msgs.push(Msg {
                                id: format!("{key}:{idx}:out"),
                                role: "tool".into(),
                                ts_ms: ts,
                                text: out.chars().take(2000).collect(),
                                tool: None,
                            });
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            idx += 1;
        }

        let title = msgs
            .iter()
            .find(|m| m.role == "user")
            .map(|m| m.text.chars().take(60).collect())
            .unwrap_or_else(|| key.to_string());
        Ok((
            SessionMeta {
                source: "codex".into(),
                id: key.to_string(),
                project,
                title,
                created_ms: first_ts.unwrap_or(0),
                updated_ms: last_ts,
            },
            msgs,
        ))
    }
}
