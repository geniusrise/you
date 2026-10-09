use super::{conversation_id, dump_file};
use crate::model::{Msg, SessionMeta};
use anyhow::{Context, Result};
use std::path::Path;

/// Parse a Claude.ai (claude-ai) export: `conversations.json`, array of
/// conversations with `uuid`, `name`, `created_at`, `updated_at` and a
/// `chat_messages[]` array (`sender`: human|assistant, `text_message.content`,
/// `created_at`). Handles single-file and per-file layouts (we look for
/// conversations.json first).
pub fn parse_dump(root: &Path, exact: Option<&Path>, source: &str) -> Result<Vec<(SessionMeta, Vec<Msg>)>> {
    let file = dump_file(root, exact, source)?;
    let raw = std::fs::read_to_string(&file)
        .with_context(|| format!("reading {}", file.display()))?;
    let convs: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", file.display()))?;
    let Some(arr) = convs.as_array() else {
        anyhow::bail!("expected a JSON array in {}", file.display());
    };

    let mut out = Vec::new();
    for conv in arr {
        let title = conv["name"]
            .as_str()
            .or_else(|| conv["title"].as_str())
            .unwrap_or("untitled")
            .to_string();
        let uuid = conv["uuid"].as_str().or_else(|| conv["id"].as_str());
        let created = conv["created_at"].as_i64().unwrap_or(0);
        let updated = conv["updated_at"].as_i64().unwrap_or(created);

        let mut entries: Vec<(i64, String, String, String)> = Vec::new(); // ts, role, text, msg uuid
        if let Some(msgs) = conv["chat_messages"].as_array() {
            for m in msgs {
                let sender = m["sender"].as_str().unwrap_or_default();
                let role = match sender {
                    "human" => "user",
                    "assistant" => "assistant",
                    _ => continue,
                };
                let ts = m["created_at"].as_i64().unwrap_or(0);
                let text = m["text_message"]["content"]
                    .as_str()
                    .or_else(|| m["content"].as_array().and_then(|c| c.first()).and_then(|c| c["text"].as_str()))
                    .or_else(|| m["content"].as_array().and_then(|c| c.first()).and_then(|c| c.as_str()))
                    .unwrap_or_default();
                if text.trim().is_empty() {
                    continue;
                }
                let id = m["uuid"].as_str().or_else(|| m["id"].as_str()).unwrap_or_default().to_string();
                entries.push((ts, role.to_string(), text.to_string(), id));
            }
        }
        entries.sort_by_key(|(ts, _, _, _)| *ts);
        if entries.is_empty() {
            continue;
        }

        let first_user = entries
            .iter()
            .find(|(_, r, _, _)| r == "user")
            .map(|(_, _, t, _)| t.clone())
            .unwrap_or_default();
        let id = conversation_id(uuid, &title, &first_user);
        let msgs: Vec<Msg> = entries
            .into_iter()
            .map(|(ts, role, text, mid)| Msg {
                id: if mid.is_empty() {
                    format!("{id}:{}", crate::model::stable_msg_id(&role, &text, ts))
                } else {
                    format!("{id}:{mid}")
                },
                role,
                ts_ms: ts * 1000,
                text: text.chars().take(8000).collect(),
                tool: None,
            })
            .collect();

        out.push((
            SessionMeta {
                source: source.to_string(),
                id,
                project: "default".into(),
                title: title.chars().take(80).collect(),
                created_ms: created * 1000,
                updated_ms: updated.max(entries_last_ts(&msgs)) * 1000,
            },
            msgs,
        ));
    }
    Ok(out)
}

fn entries_last_ts(msgs: &[Msg]) -> i64 {
    msgs.last().map(|m| m.ts_ms / 1000).unwrap_or(0)
}
