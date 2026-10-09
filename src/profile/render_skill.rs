use super::traits::ProfileDoc;
use crate::store::Store;
use anyhow::{Context, Result};

/// Deterministic SKILL.md rendering from the profile doc (no LLM — stable diffs).
pub fn render(doc: &ProfileDoc) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("name: user-persona\n");
    out.push_str("description: Personality, preferences and working style of the user, distilled from their AI chat history. Use when personalizing responses, tone, and workflow choices.\n");
    out.push_str("---\n\n");
    out.push_str("# User Persona\n\n");
    out.push_str(&format!(
        "Distilled from {} traits across the user's AI chat history (updated {}).\n\n",
        doc.traits.len(),
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(doc.updated_at_ms)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".into())
    ));

    for cat in super::traits::CATEGORIES {
        let traits: Vec<_> = doc.traits.iter().filter(|t| t.category == *cat).collect();
        if traits.is_empty() {
            continue;
        }
        out.push_str(&format!("## {}\n\n", capitalize(cat)));
        for t in traits {
            out.push_str(&format!("- **{}** ({:.2})", t.statement, t.confidence));
            if let Some(ev) = t.evidence.first() {
                out.push_str(&format!(" — \"{}\"", truncate(&ev.quote, 120)));
            }
            out.push('\n');
        }
        out.push('\n');
    }

    // Working-with-me heuristics from workflow + preferences
    let actionable: Vec<_> = doc
        .traits
        .iter()
        .filter(|t| t.category == "workflow" || t.category == "preferences")
        .take(12)
        .collect();
    if !actionable.is_empty() {
        out.push_str("## Working with me\n\n");
        for t in actionable {
            out.push_str(&format!("- {}\n", t.statement));
        }
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Install the generated SKILL.md into a harness skill directory.
pub fn install(store: &Store, target: Option<&str>, agents_dir: Option<&std::path::Path>) -> Result<()> {
    let skill = store.skill_path();
    if !skill.exists() {
        anyhow::bail!("no {} — run `aiyou profile` first", skill.display());
    }
    let home = std::env::var("HOME").context("$HOME not set")?;
    let dest_dir: std::path::PathBuf = match (agents_dir, target) {
        (Some(d), _) => d.to_path_buf(),
        (None, Some(t)) => match t {
            "claude" => [home.as_str(), ".claude", "skills"].iter().collect(),
            "opencode" => [home.as_str(), ".config", "opencode", "skill"].iter().collect(),
            "crush" => [home.as_str(), ".local", "share", "crush", "skills"].iter().collect(),
            "codex" => [home.as_str(), ".codex", "skills"].iter().collect(),
            other => anyhow::bail!("unknown target {other} (expected claude|opencode|crush|codex)"),
        },
        (None, None) => anyhow::bail!("specify --target claude|opencode|crush|codex or --agents-dir <path>"),
    };
    let dest = dest_dir.join("user-persona");
    std::fs::create_dir_all(&dest)?;
    std::fs::copy(&skill, dest.join("SKILL.md"))?;
    println!("installed {} -> {}/SKILL.md", skill.display(), dest.display());
    Ok(())
}
