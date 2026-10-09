use crate::model::{Msg, SessionMeta};

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed.chars().take(48).collect::<String>().trim_matches('-').to_string()
    }
}

pub fn redact(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?i)(sk-[a-zA-Z0-9_-]{8,}|AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{20,}|gho_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[baprs]-[a-zA-Z0-9-]{10,}|AIza[0-9A-Za-z_-]{30,}|eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})",
        ).unwrap()
    });
    re.replace_all(text, "[REDACTED]").to_string()
}

fn iso(ts_ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ts_ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| ts_ms.to_string())
}

pub fn render_markdown(meta: &SessionMeta, msgs: &[Msg]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", if meta.title.is_empty() { &meta.id } else { &meta.title }));
    out.push_str(&format!(
        "*{} · {} · {} · {}*\n\n",
        meta.source, meta.project, iso(meta.created_ms), meta.id
    ));
    for m in msgs {
        match m.role.as_str() {
            "user" => out.push_str(&format!("**user**:\n\n{}\n\n", m.text)),
            "assistant" => out.push_str(&format!("**assistant**:\n\n{}\n\n", m.text)),
            "system" => out.push_str(&format!("**system**:\n\n{}\n\n", m.text)),
            _ => out.push_str(&format!("**tool**:\n\n{}\n\n", m.text)),
        }
        if let Some(tool) = &m.tool {
            out.push_str(&format!("> tool: `{tool}`\n\n"));
        }
    }
    out
}
