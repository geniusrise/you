use super::{conversation_id, dump_file};
use crate::model::{stable_msg_id, Msg, SessionMeta};
use anyhow::{Context, Result};
use chrono::DateTime;
use std::path::Path;

fn iso_to_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

/// Parse a Gemini Takeout JSON export (best-effort): a JSON array of chats
/// with an `activity` list; each activity has a `title` like "Prompt: ..." /
/// "Response: ..." (or "Searched for ..."), a `time` RFC3339 string and
/// optionally `from_who` (user/model).
pub fn parse_dump(root: &Path, exact: Option<&Path>, source: &str) -> Result<Vec<(SessionMeta, Vec<Msg>)>> {
    let file = dump_file(root, exact, source)?;
    let raw = std::fs::read_to_string(&file)
        .with_context(|| format!("reading {}", file.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", file.display()))?;

    // Accept: top-level array of chats, top-level object with chats/activity,
    // or a bare activity array.
    let chats: Vec<serde_json::Value> = match &v {
        serde_json::Value::Array(a) => a.clone(),
        serde_json::Value::Object(_) => v["chats"]
            .as_array()
            .cloned()
            .or_else(|| v["activity"].as_array().cloned())
            .unwrap_or_default(),
        _ => vec![],
    };

    let mut out = Vec::new();
    for chat in chats {
        let title = chat["title"].as_str().unwrap_or("gemini chat").to_string();
        let Some(activities) = chat["activity"].as_array() else {
            continue;
        };
        let mut entries: Vec<(i64, String, String)> = Vec::new();
        for act in activities {
            let full_title = act["title"].as_str().unwrap_or_default();
            let (role, text) = if let Some(t) = full_title.strip_prefix("Prompt: ") {
                ("user", t)
            } else if let Some(t) = full_title.strip_prefix("Response: ") {
                ("assistant", t)
            } else if let Some(t) = full_title.strip_prefix("Searched for ") {
                ("user", t)
            } else {
                match act["from_who"].as_str() {
                    Some("model") => ("assistant", full_title),
                    _ => ("user", full_title),
                }
            };
            if text.trim().is_empty() {
                continue;
            }
            let ts = act["time"]
                .as_str()
                .and_then(iso_to_ms)
                .unwrap_or_else(|| act["time"].as_i64().unwrap_or(0));
            entries.push((ts, role.to_string(), text.to_string()));
        }
        entries.sort_by_key(|(ts, _, _)| *ts);
        if entries.is_empty() {
            continue;
        }

        let first_user = entries
            .iter()
            .find(|(_, r, _)| r == "user")
            .map(|(_, _, t)| t.clone())
            .unwrap_or_default();
        let id = conversation_id(
            chat["id"].as_str().or_else(|| chat["identifier"].as_str()),
            &title,
            &first_user,
        );
        let msgs: Vec<Msg> = entries
            .into_iter()
            .map(|(ts, role, text)| Msg {
                id: format!("{id}:{}", stable_msg_id(&role, &text, ts)),
                role,
                ts_ms: ts,
                text: text.chars().take(8000).collect(),
                tool: None,
            })
            .collect();
        let created = msgs.first().map(|m| m.ts_ms).unwrap_or(0);
        let updated = msgs.last().map(|m| m.ts_ms).unwrap_or(created);
        out.push((
            SessionMeta {
                source: source.to_string(),
                id,
                project: "default".into(),
                title: title.chars().take(80).collect(),
                created_ms: created,
                updated_ms: updated,
            },
            msgs,
        ));
    }
    Ok(out)
}
