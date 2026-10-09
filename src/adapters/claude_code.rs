use super::{Adapter, DiscoveredSession};
use crate::model::{stable_msg_id, Msg, SessionMeta};
use anyhow::{Context, Result};
use chrono::DateTime;
use std::path::PathBuf;

pub struct ClaudeCode {
    pub dir: PathBuf,
}

fn iso_to_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp_millis())
}

fn content_to_text_and_tools(
    content: &serde_json::Value,
    out: &mut Vec<(String, String, Option<String>)>, // (role, text, tool)
    role: &str,
) {
    match content {
        serde_json::Value::String(s) => out.push((role.to_string(), s.clone(), None)),
        serde_json::Value::Array(parts) => {
            let mut text = String::new();
            let mut tools: Vec<(String, String)> = Vec::new(); // (name, input)
            for p in parts {
                let ptype = p["type"].as_str().unwrap_or_default();
                match ptype {
                    "text" => {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(p["text"].as_str().unwrap_or_default());
                    }
                    "tool_use" => {
                        let name = p["name"].as_str().unwrap_or("tool").to_string();
                        let input = serde_json::to_string_pretty(&p["input"]).unwrap_or_default();
                        tools.push((name, input));
                    }
                    "tool_result" => {
                        let c = &p["content"];
                        let txt = match c {
                            serde_json::Value::String(s) => s.clone(),
                            other => serde_json::to_string(other).unwrap_or_default(),
                        };
                        let txt: String = txt.chars().take(2000).collect();
                        out.push(("tool".to_string(), txt, None));
                    }
                    "thinking" => {}
                    _ => {}
                }
            }
            if !text.is_empty() {
                out.push((role.to_string(), text, None));
            }
            for (name, input) in tools {
                out.push(("tool".to_string(), input, Some(name)));
            }
        }
        _ => {}
    }
}

impl ClaudeCode {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Adapter for ClaudeCode {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        if !self.dir.is_dir() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        for entry in walkdir::WalkDir::new(&self.dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|e| e == "jsonl")
            {
                let md = entry.metadata().ok();
                let mtime = md
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
                let project = entry
                    .path()
                    .parent()
                    .map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                    .unwrap_or_default();
                out.push(DiscoveredSession {
                    key: entry.path().file_stem().unwrap_or_default().to_string_lossy().to_string(),
                    fingerprint: mtime ^ (size << 1),
                    meta_hint: Some((project, String::new())),
                });
            }
        }
        Ok(out)
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let mut path = None;
        for entry in walkdir::WalkDir::new(&self.dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|e| e == "jsonl")
                && entry.path().file_stem().is_some_and(|s| s == key)
            {
                path = Some(entry.path().to_path_buf());
                break;
            }
        }
        let path = path.with_context(|| format!("session {key} not found under {}", self.dir.display()))?;
        let project = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".into());

        let raw = std::fs::read_to_string(&path)?;
        let mut msgs: Vec<Msg> = Vec::new();
        let mut first_ts: Option<i64> = None;
        let mut last_ts: i64 = 0;
        let mut session_id = key.to_string();
        let mut first_user_text: Option<String> = None;

        for line in raw.lines() {
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v["type"].as_str() == Some("last-prompt") {
                if let Some(sid) = v["sessionId"].as_str() {
                    session_id = sid.to_string();
                }
                continue;
            }
            let ty = v["type"].as_str().unwrap_or_default();
            if ty != "user" && ty != "assistant" {
                continue;
            }
            let uuid = v["uuid"].as_str().unwrap_or_default().to_string();
            let ts = v["timestamp"]
                .as_str()
                .and_then(iso_to_ms)
                .unwrap_or(last_ts);
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = last_ts.max(ts);
            let mut extracted: Vec<(String, String, Option<String>)> = Vec::new();
            content_to_text_and_tools(&v["message"]["content"], &mut extracted, ty);
            for (role, text, tool) in extracted {
                if first_user_text.is_none() && role == "user" {
                    first_user_text = Some(text.chars().take(60).collect());
                }
                let id = if uuid.is_empty() {
                    stable_msg_id(&role, &text, ts)
                } else {
                    format!("{uuid}:{}", tool.clone().unwrap_or_default())
                };
                let text = text.chars().take(8000).collect::<String>();
                msgs.push(Msg { id, role, ts_ms: ts, text, tool });
            }
        }

        let title = first_user_text.unwrap_or_else(|| key.to_string());
        Ok((
            SessionMeta {
                source: "claude-code".into(),
                id: session_id,
                project,
                title,
                created_ms: first_ts.unwrap_or(0),
                updated_ms: last_ts,
            },
            msgs,
        ))
    }
}
