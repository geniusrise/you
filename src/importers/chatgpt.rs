use super::conversation_id;
use crate::model::{Msg, SessionMeta};
use anyhow::{Context, Result};
use std::path::Path;

/// Parse a ChatGPT export (`conversations.json`): array of conversations with
/// a `mapping` tree of messages, linearized by create_time.
pub fn parse_dump(root: &Path, exact: Option<&Path>, source: &str) -> Result<Vec<(SessionMeta, Vec<Msg>)>> {
    let file = super::dump_file(root, exact, source)?;
    let raw = std::fs::read_to_string(&file)
        .with_context(|| format!("reading {}", file.display()))?;
    let convs: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {}", file.display()))?;
    let Some(arr) = convs.as_array() else {
        anyhow::bail!("expected a JSON array in {}", file.display());
    };

    let mut out = Vec::new();
    for conv in arr {
        let title = conv["title"].as_str().unwrap_or("untitled").to_string();
        let created_s = conv["create_time"].as_i64().unwrap_or(0);
        let updated_s = conv["update_time"].as_i64().unwrap_or(created_s);

        // linearize mapping tree by message create_time; node ids are stable
        // across progressive dumps, so they anchor message identity
        let mut entries: Vec<(i64, String, String, String)> = Vec::new(); // (ts, role, text, node_id)
        if let Some(mapping) = conv["mapping"].as_object() {
            for (nid, node) in mapping {
                let msg = &node["message"];
                if msg.is_null() {
                    continue;
                }
                let role = msg["author"]["role"]
                    .as_str()
                    .or_else(|| msg["role"].as_str())
                    .unwrap_or_default()
                    .to_string();
                if role.is_empty() || role == "system" {
                    continue;
                }
                let ts = msg["create_time"].as_i64().unwrap_or(0);
                let parts = msg["content"]["parts"].as_array();
                let text = match parts {
                    Some(ps) => ps
                        .iter()
                        .filter_map(|p| p.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    None => msg["content"]["text"].as_str().unwrap_or_default().to_string(),
                };
                if text.trim().is_empty() {
                    continue;
                }
                entries.push((ts, role, text, nid.clone()));
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
        let id = conversation_id(
            conv["id"].as_str().or_else(|| conv["conversation_id"].as_str()),
            &title,
            &first_user,
        );
        let msgs: Vec<Msg> = entries
            .into_iter()
            .map(|(ts, role, text, nid)| Msg {
                id: format!("{id}:{nid}"),
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
                created_ms: created_s * 1000,
                updated_ms: updated_s * 1000,
            },
            msgs,
        ));
    }
    Ok(out)
}
