use super::traits::ProfileDoc;
use crate::store::Store;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const SKILL_DIR: &str = "me";
const LEGACY_SKILL_DIR: &str = "user-persona";
const BLOCK_BEGIN: &str = "<!-- aiyou:begin -->";
const BLOCK_END: &str = "<!-- aiyou:end -->";

fn heading(cat: &str) -> &'static str {
    match cat {
        "personality" => "Who I am",
        "communication" => "How to talk to me",
        "preferences" => "What I prefer",
        "workflow" => "How I like work done",
        "tech-stack" => "My tools and stack",
        "values" => "What I value",
        "goals" => "What I'm working toward",
        _ => "Other",
    }
}

/// Deterministic SKILL.md rendering from the profile doc (no LLM — stable diffs).
pub fn render(doc: &ProfileDoc) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("name: me\n");
    out.push_str("description: Who the user is and how they want to be worked with — personality, taste, communication style and working habits, distilled from their real AI chat history. Use to decide tone, level of detail, and how to approach and carry out tasks.\n");
    out.push_str("---\n\n");
    out.push_str("# Me\n\n");
    out.push_str(&format!(
        "{} traits distilled from my own conversations (updated {}). Treat them as defaults: follow them unless I say otherwise in the current conversation.\n\n",
        doc.traits.len(),
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(doc.updated_at_ms)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".into())
    ));

    for cat in super::traits::CATEGORIES {
        let mut traits: Vec<_> = doc.traits.iter().filter(|t| t.category == *cat).collect();
        if traits.is_empty() {
            continue;
        }
        traits.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        out.push_str(&format!("## {}\n\n", heading(cat)));
        for t in traits {
            out.push_str(&format!("- {}", t.statement));
            if t.confidence < 0.6 {
                out.push_str(" _(tentative)_");
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// The skill without its frontmatter — for pasting into AGENTS.md-style files.
fn body(skill: &str) -> String {
    match skill.strip_prefix("---\n").and_then(|s| s.split_once("\n---\n")) {
        Some((_, rest)) => rest.trim_start().to_string(),
        None => skill.to_string(),
    }
}

/// Insert or replace the managed block in `existing`, leaving all other text untouched.
pub fn upsert_block(existing: &str, content: &str) -> String {
    let block = format!("{BLOCK_BEGIN}\n{}\n{BLOCK_END}\n", content.trim_end());
    match (existing.find(BLOCK_BEGIN), existing.find(BLOCK_END)) {
        (Some(s), Some(e)) if s < e => {
            let end = e + BLOCK_END.len();
            let tail = existing[end..].strip_prefix('\n').unwrap_or(&existing[end..]);
            format!("{}{}{}", &existing[..s], block, tail)
        }
        _ if existing.trim().is_empty() => block,
        _ => format!("{}\n\n{}", existing.trim_end(), block),
    }
}

fn home() -> Result<String> {
    std::env::var("HOME").context("$HOME not set")
}

/// Install the generated skill into a harness: a skill directory, a managed
/// block inside a context file, or stdout.
pub fn install(
    store: &Store,
    target: Option<&str>,
    agents_dir: Option<&Path>,
    file: Option<&Path>,
    print: bool,
) -> Result<()> {
    let skill_path = store.skill_path();
    if !skill_path.exists() {
        anyhow::bail!(
            "no {} — run `aiyou profile` (and `aiyou accept`) first",
            skill_path.display()
        );
    }
    let skill = std::fs::read_to_string(&skill_path)?;
    if print {
        print!("{skill}");
        return Ok(());
    }

    let file_target: Option<PathBuf> = match (file, target) {
        (Some(f), _) => Some(f.to_path_buf()),
        (None, Some("claude-md")) => Some([home()?.as_str(), ".claude", "CLAUDE.md"].iter().collect()),
        (None, Some("gemini")) => Some([home()?.as_str(), ".gemini", "GEMINI.md"].iter().collect()),
        (None, Some("codex-md")) => Some([home()?.as_str(), ".codex", "AGENTS.md"].iter().collect()),
        (None, Some("agents-md")) => Some(PathBuf::from("AGENTS.md")),
        _ => None,
    };
    if let Some(path) = file_target {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        crate::store::write_atomic(&path, upsert_block(&existing, &body(&skill)))?;
        println!("updated aiyou block in {}", path.display());
        return Ok(());
    }

    let dest_dir: PathBuf = match (agents_dir, target) {
        (Some(d), _) => d.to_path_buf(),
        (None, Some(t)) => match t {
            "claude" => [home()?.as_str(), ".claude", "skills"].iter().collect(),
            "opencode" => [home()?.as_str(), ".config", "opencode", "skill"].iter().collect(),
            "crush" => [home()?.as_str(), ".local", "share", "crush", "skills"].iter().collect(),
            "codex" => [home()?.as_str(), ".codex", "skills"].iter().collect(),
            other => anyhow::bail!(
                "unknown target {other} (skills: claude|opencode|crush|codex; context files: claude-md|gemini|codex-md|agents-md)"
            ),
        },
        (None, None) => anyhow::bail!(
            "specify --target <claude|opencode|crush|codex|claude-md|gemini|codex-md|agents-md>, --agents-dir <dir>, --file <path> or --print"
        ),
    };
    let dest = dest_dir.join(SKILL_DIR);
    std::fs::create_dir_all(&dest)?;
    crate::store::write_atomic(&dest.join("SKILL.md"), &skill)?;

    // v0.2 installed this as `user-persona`; drop our own stale copy so agents don't see two
    let legacy = dest_dir.join(LEGACY_SKILL_DIR).join("SKILL.md");
    if std::fs::read_to_string(&legacy).is_ok_and(|s| s.contains("distilled from their AI chat history")) {
        let _ = std::fs::remove_dir_all(dest_dir.join(LEGACY_SKILL_DIR));
    }
    println!("installed {} -> {}/SKILL.md", skill_path.display(), dest.display());
    Ok(())
}
