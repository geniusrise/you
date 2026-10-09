use crate::config::Config;
use crate::model::{stable_msg_id, Msg, SessionMeta};
use crate::store::Store;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub mod chatgpt;
pub mod claude_ai;
pub mod gemini_takeout;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DumpEntry {
    pub source: String,
    pub file: String,
    pub file_sha256: String,
    pub imported_at_ms: i64,
    pub conversations: usize,
    pub messages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportManifest {
    pub entries: Vec<DumpEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImportReport {
    pub conversations: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
}

fn sha256_file(path: &Path) -> Result<String> {
    let data = std::fs::read(path)?;
    Ok(hex::encode(Sha256::digest(&data)))
}

fn sha256_str(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn hash8(s: &str) -> String {
    sha256_str(s).chars().take(8).collect()
}

fn load_manifest(store: &Store) -> ImportManifest {
    std::fs::read_to_string(store.imports_dir().join("manifest.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_manifest(store: &Store, m: &ImportManifest) -> Result<()> {
    std::fs::create_dir_all(store.imports_dir())?;
    std::fs::write(
        store.imports_dir().join("manifest.json"),
        serde_json::to_string_pretty(m)?,
    )?;
    Ok(())
}

/// Resolve a dump path: extract zips to a temp dir (kept alive for the call),
/// return the effective root to search for dump files plus the exact input
/// file when the input was a file (so re-scanning can't pick a sibling).
fn resolve_path(path: &Path) -> Result<(PathBuf, Option<PathBuf>, Option<tempfile::TempDir>)> {
    if path.is_dir() {
        return Ok((path.to_path_buf(), None, None));
    }
    if path.extension().is_some_and(|e| e == "zip") {
        let tmp = tempfile::tempdir()?;
        let file = std::fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        archive.extract(tmp.path())?;
        return Ok((tmp.path().to_path_buf(), None, Some(tmp)));
    }
    if path.is_file() {
        return Ok((
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            Some(path.to_path_buf()),
            None,
        ));
    }
    bail!("path {} does not exist", path.display())
}

/// Conversation identity per spec: (source, external_id) else slug(title)+hash8(first user msg).
pub fn conversation_id(external_id: Option<&str>, title: &str, first_user_msg: &str) -> String {
    let raw = match external_id {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => format!("{}-{}", crate::render::slug(title), hash8(first_user_msg)),
    };
    raw.replace(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'), "-")
}

/// Upsert messages by id into an existing session; re-sort; re-write.
/// Returns (existed_before, changed).
fn upsert_session(
    store: &Store,
    cfg: &Config,
    meta: &SessionMeta,
    new_msgs: Vec<Msg>,
) -> Result<(bool, bool)> {
    let existing_path = store.find_session(&meta.source, &meta.id);
    let (mut meta_out, mut msgs) = match &existing_path {
        Some(p) => store.read_session_jsonl_at(p)?,
        None => (meta.clone(), Vec::new()),
    };
    meta_out.updated_ms = meta_out.updated_ms.max(meta.updated_ms);
    if meta_out.title.is_empty() {
        meta_out.title = meta.title.clone();
    }
    let before = msgs.len();
    let mut changed = existing_path.is_none();
    for m in new_msgs {
        match msgs.iter().position(|x| x.id == m.id) {
            Some(i) => {
                if msgs[i].text != m.text {
                    msgs[i] = m;
                    changed = true;
                }
            }
            None => {
                msgs.push(m);
                changed = true;
            }
        }
    }
    if msgs.len() != before || changed {
        store.write_session(&meta_out, &mut msgs, cfg)?;
    }
    Ok((existing_path.is_some(), changed))
}

/// Entry point for `aiyou import <source> <path>`.
pub fn import_dump(store: &Store, cfg: &Config, source: &str, path: &Path) -> Result<ImportReport> {
    let (root, exact_file, _tmp_keepalive) = resolve_path(path)?;
    let source = source.to_string();

    let mut sessions: Vec<(SessionMeta, Vec<Msg>)> = match source.as_str() {
        "chatgpt" => chatgpt::parse_dump(&root, exact_file.as_deref(), &source)?,
        "claude-ai" => claude_ai::parse_dump(&root, exact_file.as_deref(), &source)?,
        "gemini" => gemini_takeout::parse_dump(&root, exact_file.as_deref(), &source)?,
        other => bail!("unknown import source {other} (expected chatgpt|claude-ai|gemini)"),
    };
    sessions.retain(|(m, msgs)| !msgs.is_empty());

    // identical-dump short-circuit
    let dump_file = match exact_file {
        Some(f) => f,
        None => find_dump_file(&root, &source)?,
    };
    let file_hash = sha256_file(&dump_file)?;
    let mut manifest = load_manifest(store);
    if manifest
        .entries
        .iter()
        .any(|e| e.source == source && e.file_sha256 == file_hash)
    {
        return Ok(ImportReport {
            conversations: sessions.len(),
            added: 0,
            updated: 0,
            unchanged: sessions.len(),
        });
    }

    let mut report = ImportReport { conversations: sessions.len(), ..Default::default() };
    let mut total_msgs = 0usize;
    let mut any_change = false;
    for (meta, mut msgs) in sessions {
        // assign ids
        for m in msgs.iter_mut() {
            if m.id.is_empty() {
                m.id = stable_msg_id(&m.role, &m.text, m.ts_ms);
            }
        }
        msgs.sort_by(|a, b| (a.ts_ms, &a.id).cmp(&(b.ts_ms, &b.id)));
        total_msgs += msgs.len();
        let (existed, changed) = upsert_session(store, cfg, &meta, msgs)?;
        if !existed {
            report.added += 1;
            any_change = true;
        } else if changed {
            report.updated += 1;
            any_change = true;
        } else {
            report.unchanged += 1;
        }
    }

    manifest.entries.push(DumpEntry {
        source: source.clone(),
        file: dump_file.display().to_string(),
        file_sha256: file_hash,
        imported_at_ms: chrono::Utc::now().timestamp_millis(),
        conversations: report.conversations,
        messages: total_msgs,
    });
    save_manifest(store, &manifest)?;
    if any_change {
        crate::git::git_commit_all(
            &store.root,
            &format!("import: {source} +{} ~{} ={}", report.added, report.updated, report.unchanged),
        )?;
    }
    Ok(report)
}

pub(crate) fn dump_file(root: &Path, exact: Option<&Path>, source: &str) -> Result<PathBuf> {
    match exact {
        Some(f) if f.is_file() => Ok(f.to_path_buf()),
        _ => find_dump_file(root, source),
    }
}

fn find_dump_file(root: &Path, source: &str) -> Result<PathBuf> {
    let names: &[&str] = match source {
        "chatgpt" => &["conversations.json", "chat.json"],
        "claude-ai" => &["conversations.json", "chats.json"],
        "gemini" => &["MyChat.json", "MyChats.json", "MyActivities.json", "MyActivity.json"],
        _ => &["conversations.json"],
    };
    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if names.contains(&name.as_str()) {
            return Ok(entry.path().to_path_buf());
        }
    }
    bail!("no dump file found under {} for source {source}", root.display())
}
