//! Generic scan-based session parsing for less-standardized harness formats
//! (crush, pi). Walks directories for .json/.jsonl files containing a
//! top-level `messages` array and extracts best-effort canonical messages.

use crate::model::Msg;
use crate::model::SessionMeta;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct ScannedFile {
    pub path: PathBuf,
    pub key: String,
    pub fingerprint: u64,
    pub project: String,
}

pub fn scan(dirs: &[PathBuf], file_filter: fn(&Path) -> bool) -> Vec<ScannedFile> {
    let mut out = Vec::new();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            if !file_filter(p) {
                continue;
            }
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if matches!(
                name.as_str(),
                "crush.json"
                    | "crush.json.lock"
                    | "hyper.json"
                    | "projects.json"
                    | "providers.json"
                    | "config.json"
                    | "auth.json"
                    | "settings.json"
                    | "state.json"
                    | "logs.json"
                    | ".project_root"
            ) {
                continue;
            }
            let md = entry.metadata().ok();
            let mtime = md
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
            let project = p
                .parent()
                .and_then(|par| par.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "default".into());
            out.push(ScannedFile {
                path: p.to_path_buf(),
                key: p.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                fingerprint: mtime ^ (size << 1),
                project,
            });
        }
    }
    out
}

// (role, text, tool, ts, ext_id)
type Extracted = Vec<(String, String, Option<String>, i64, Option<String>)>;

fn extract_messages(
    v: &serde_json::Value,
    default_ts: i64,
) -> Extracted {
    // (role, text, tool, ts, ext_id)
    let mut out = Vec::new();
    let Some(arr) = v["messages"].as_array() else {
        return out;
    };
    for (i, m) in arr.iter().enumerate() {
        let role = m["role"].as_str().unwrap_or("user").to_string();
        let ts = m["timestamp"]
            .as_i64()
            .or_else(|| m["time"].as_i64())
            .or_else(|| m["created_at"].as_i64())
            .unwrap_or(default_ts + i as i64);
        let ext_id = m["id"].as_str().map(|s| s.to_string());
        if let Some(content) = m["content"].as_str() {
            out.push((role, content.to_string(), None, ts, ext_id.clone()));
        } else if let Some(content) = m["content"].as_array() {
            // array of strings or {text} objects
            let text = content
                .iter()
                .filter_map(|c| match c {
                    serde_json::Value::String(s) => Some(s.clone()),
                    other => other["text"].as_str().map(|s| s.to_string()),
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                out.push((role, text, None, ts, ext_id.clone()));
            }
        } else if let Some(um) = m["userMessage"].as_str() {
            out.push(("user".to_string(), um.to_string(), None, ts, ext_id.clone()));
        } else if m["message"].is_object() {
            let inner = &m["message"];
            let irole = inner["role"].as_str().unwrap_or(&role).to_string();
            let itext = inner["content"]
                .as_str()
                .or_else(|| inner["text"].as_str())
                .unwrap_or_default()
                .to_string();
            if !itext.is_empty() {
                out.push((irole, itext, None, ts, ext_id.clone()));
            }
        } else if let Some(parts) = m["parts"].as_array() {
            let mut text = String::new();
            let mut tool: Option<String> = None;
            let mut tool_text = String::new();
            for p in parts {
                match p["type"].as_str().unwrap_or("text") {
                    "text" => {
                        if let Some(t) = p["text"].as_str() {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(t);
                        }
                    }
                    "tool" => {
                        tool = Some(p["tool"].as_str().unwrap_or("tool").to_string());
                        tool_text = serde_json::to_string_pretty(&p["state"]).unwrap_or_default();
                    }
                    _ => {}
                }
            }
            if !text.is_empty() {
                out.push((role, text, None, ts, ext_id.clone()));
            }
            if let Some(t) = tool {
                out.push((
                    "tool".to_string(),
                    tool_text,
                    Some(t),
                    ts,
                    ext_id.map(|e| format!("{e}:tool")),
                ));
            }
        } else if let Some(text) = m["text"].as_str() {
            out.push((role, text.to_string(), None, ts, ext_id));
        }
    }
    out
}

fn parse_value(path: &Path) -> Result<serde_json::Value> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if path.extension().is_some_and(|e| e == "jsonl") {
        let mut found = serde_json::Value::Null;
        let mut lines_as_messages: Vec<serde_json::Value> = Vec::new();
        for line in raw.lines() {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
                if obj["messages"].is_array() {
                    return Ok(obj);
                }
                if found.is_null() {
                    found = obj.clone();
                }
                // a bare message line: has a role-ish and text-ish key
                let roleish = obj["role"].is_string() || obj["sender"].is_string();
                let textish = obj["text"].is_string()
                    || obj["content"].is_string()
                    || obj["message"].is_string()
                    || obj["userMessage"].is_string();
                if roleish || textish {
                    lines_as_messages.push(obj);
                }
            }
        }
        if !lines_as_messages.is_empty() {
            return Ok(serde_json::json!({ "messages": lines_as_messages }));
        }
        if found.is_null() {
            anyhow::bail!("no JSON objects found in {}", path.display());
        }
        Ok(found)
    } else {
        Ok(serde_json::from_str(&raw)?)
    }
}

/// Parse one scanned file into (meta, messages).
pub fn parse_scanned(
    path: &Path,
    source: &'static str,
    fallback_key: &str,
    project: &str,
) -> Result<(SessionMeta, Vec<Msg>)> {
    let v = parse_value(path)?;
    let id = v["id"]
        .as_str()
        .or_else(|| v["sessionId"].as_str())
        .or_else(|| v["session_id"].as_str())
        .unwrap_or(fallback_key)
        .to_string();
    let default_ts = v["created"]
        .as_i64()
        .or_else(|| v["createdAt"].as_i64())
        .or_else(|| v["create_time"].as_i64())
        .unwrap_or(0);
    let title = v["title"]
        .as_str()
        .or_else(|| v["name"].as_str())
        .map(|t| t.to_string())
        .unwrap_or_else(|| {
            extract_messages(&v, default_ts)
                .iter()
                .find(|(r, ..)| r == "user")
                .map(|(_, t, ..)| t.chars().take(60).collect())
                .unwrap_or_else(|| fallback_key.to_string())
        });
    let mut msgs: Vec<Msg> = extract_messages(&v, default_ts)
        .into_iter()
        .enumerate()
        .map(|(i, (role, text, tool, ts, ext_id))| Msg {
            id: ext_id.unwrap_or_else(|| format!("{fallback_key}:{i}")),
            role,
            ts_ms: ts,
            text: text.chars().take(8000).collect(),
            tool,
        })
        .collect();
    msgs.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
    let updated = msgs.last().map(|m| m.ts_ms).unwrap_or(default_ts).max(default_ts);
    Ok((
        SessionMeta {
            source: source.to_string(),
            id,
            project: project.to_string(),
            title,
            created_ms: default_ts,
            updated_ms: updated,
        },
        msgs,
    ))
}

/// Recursively find arrays of {role,text}-ish objects inside an arbitrary JSON
/// blob (used for VS Code-style state.vscdb chat payloads).
pub fn find_message_arrays(
    v: &serde_json::Value,
    out: &mut Vec<crate::model::Msg>,
    session_key: &str,
    start_idx: &mut usize,
) {
    match v {
        serde_json::Value::Array(arr) => {
            let msgish = arr
                .iter()
                .filter(|item| {
                    let roleish = item["role"].is_string() || item["speaker"].is_string();
                    let textish = item["text"].is_string()
                        || item["content"].is_string()
                        || (item["content"]["type"].is_string() && item["content"]["value"].is_string());
                    roleish && textish
                })
                .count();
            if msgish > 0 && msgish >= arr.len() / 2 {
                for item in arr {
                    let role = item["role"]
                        .as_str()
                        .or_else(|| item["speaker"].as_str())
                        .unwrap_or("assistant");
                    let text = item["text"]
                        .as_str()
                        .or_else(|| item["content"].as_str())
                        .or_else(|| item["content"]["value"].as_str())
                        .unwrap_or_default();
                    if text.trim().is_empty() {
                        continue;
                    }
                    out.push(crate::model::Msg {
                        id: format!("{session_key}:{}", *start_idx),
                        role: role.to_string(),
                        ts_ms: item["timestamp"].as_i64().or_else(|| item["createdAt"].as_i64()).unwrap_or(0),
                        text: text.chars().take(8000).collect(),
                        tool: None,
                    });
                    *start_idx += 1;
                }
                return;
            }
            for item in arr {
                find_message_arrays(item, out, session_key, start_idx);
            }
        }
        serde_json::Value::Object(map) => {
            for (_k, val) in map {
                find_message_arrays(val, out, session_key, start_idx);
            }
        }
        _ => {}
    }
}
