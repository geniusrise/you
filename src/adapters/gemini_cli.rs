use super::codex::{find_by_stem, walk_jsonl};
use super::{Adapter, DiscoveredSession};
use crate::model::{Msg, SessionMeta};
use anyhow::{Context, Result};
use chrono::DateTime;
use std::path::{Path, PathBuf};

pub struct GeminiCli {
    pub dir: PathBuf,
}

fn iso_to_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

fn parts_text(message: &serde_json::Value) -> String {
    message["parts"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

impl GeminiCli {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Adapter for GeminiCli {
    fn name(&self) -> &'static str {
        "gemini-cli"
    }

    fn discover(&self) -> Result<Vec<DiscoveredSession>> {
        // sessions live under <dir>/tmp/<hash>/chats/*.jsonl and <dir>/history/**/*.jsonl
        let mut out = Vec::new();
        for sub in ["tmp", "history"] {
            let base = self.dir.join(sub);
            if !base.is_dir() {
                continue;
            }
            let sessions = walk_jsonl(&base, "gemini-cli", |p| {
                // project root marker dirs like .project_root sit next to chats;
                // use the parent-of-parent (hash dir or history/<name>) as project
                let mut proj = p.to_path_buf();
                for _ in 0..2 {
                    proj.pop();
                }
                proj.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "default".into())
            });
            out.extend(sessions);
        }
        Ok(out)
    }

    fn read(&self, key: &str) -> Result<(SessionMeta, Vec<Msg>)> {
        let mut path = None;
        for sub in ["tmp", "history"] {
            if let Some(p) = find_by_stem(&self.dir.join(sub), key) {
                path = Some(p);
                break;
            }
        }
        let path =
            path.with_context(|| format!("gemini-cli session {key} not found"))?;
        let mut project = "default".to_string();
        {
            let mut proj = path.clone();
            proj.pop(); // file
            proj.pop(); // chats or history/<name>
            if self.dir.join("history").is_ancestor_of(&proj) {
                // history/<name> — project is <name>
                if let Some(n) = proj.file_name() {
                    project = n.to_string_lossy().to_string();
                }
            } else if let Some(n) = proj.file_name() {
                // tmp/<hash> — check sibling .project_root for real name
                let marker = proj.join(".project_root");
                if let Ok(target) = std::fs::read_to_string(&marker) {
                    let t = target.trim().trim_end_matches('/');
                    if let Some(name) = t.rsplit('/').next() {
                        if !name.is_empty() {
                            project = name.to_string();
                        }
                    }
                } else {
                    project = n.to_string_lossy().to_string();
                }
            }
        }

        let raw = std::fs::read_to_string(&path)?;
        let mut msgs: Vec<Msg> = Vec::new();
        let mut first_ts: Option<i64> = None;
        let mut last_ts = 0i64;
        let mut idx = 0u64;

        for line in raw.lines() {
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let ty = v["type"].as_str().unwrap_or_default();
            let role = match ty {
                "user" => "user",
                "model" => "assistant",
                _ => continue,
            };
            let ts = v["timestamp"].as_str().and_then(iso_to_ms).unwrap_or(last_ts);
            if first_ts.is_none() {
                first_ts = Some(ts);
            }
            last_ts = last_ts.max(ts);
            let text = parts_text(&v["message"]);
            if !text.is_empty() {
                msgs.push(Msg {
                    id: format!("{key}:{idx}"),
                    role: role.to_string(),
                    ts_ms: ts,
                    text: text.chars().take(8000).collect(),
                    tool: None,
                });
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
                source: "gemini-cli".into(),
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

trait PathExt {
    fn is_ancestor_of(&self, other: &Path) -> bool;
}

impl PathExt for Path {
    fn is_ancestor_of(&self, other: &Path) -> bool {
        other.starts_with(self)
    }
}
