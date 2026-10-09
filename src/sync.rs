use crate::adapters::Adapter;
use crate::config::Config;
use crate::store::Store;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub struct SyncReport {
    pub per_source: Vec<(String, usize, usize, usize)>,
}

#[derive(Default, Serialize, Deserialize)]
struct Fingerprints(BTreeMap<String, u64>);

fn load_fps(path: &Path) -> Fingerprints {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_fps(path: &Path, fps: &Fingerprints) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(fps)?)?;
    Ok(())
}

pub fn run(store: &Store, cfg: &Config, only: Option<&[String]>) -> Result<SyncReport> {
    let mut adapters = registry(store, cfg);
    adapters.retain(|a| cfg.sources.enabled.iter().any(|s| s == a.name()));
    if let Some(list) = only {
        for wanted in list {
            if !crate::config::SOURCES.contains(&wanted.as_str()) {
                anyhow::bail!("unknown source {wanted}");
            }
        }
        adapters.retain(|a| list.iter().any(|s| s == a.name()));
    }
    run_with_adapters(store, cfg, adapters)
}

pub fn run_with_adapters(
    store: &Store,
    cfg: &Config,
    adapters: Vec<Box<dyn Adapter>>,
) -> Result<SyncReport> {
    let fps_path = store.fingerprints_path();
    let mut fps = load_fps(&fps_path);
    let mut report = SyncReport { per_source: Vec::new() };
    let mut total_added = 0usize;
    let mut total_updated = 0usize;

    for adapter in &adapters {
        let name = adapter.name();
        let mut added = 0usize;
        let mut updated = 0usize;
        let mut errors = 0usize;
        let discovered = match adapter.discover() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[warn] {name}: discover failed: {e}");
                report.per_source.push((name.to_string(), 0, 0, 0));
                continue;
            }
        };
        if discovered.is_empty() {
            report.per_source.push((name.to_string(), 0, 0, 0));
            continue;
        }
        for d in discovered {
            let cache_key = format!("{name}/{}", d.key);
            if fps.0.get(&cache_key) == Some(&d.fingerprint) {
                continue;
            }
            match adapter.read(&d.key) {
                Ok((meta, mut msgs)) => {
                    let mut meta = meta;
                    if meta.source.is_empty() {
                        meta.source = name.to_string();
                    }
                    if meta.project.is_empty() {
                        meta.project = d
                            .meta_hint
                            .as_ref()
                            .map(|(p, _)| p.clone())
                            .unwrap_or_else(|| "default".into());
                    }
                    if meta.title.is_empty() {
                        meta.title = d
                            .meta_hint
                            .as_ref()
                            .map(|(_, t)| t.clone())
                            .unwrap_or_else(|| meta.id.clone());
                    }
                    let existed = store.find_session(&meta.source, &meta.id).is_some();
                    store.write_session(&meta, &mut msgs, cfg).with_context(|| {
                        format!("writing session {}/{}", meta.source, meta.id)
                    })?;
                    if existed {
                        updated += 1;
                    } else {
                        added += 1;
                    }
                    fps.0.insert(cache_key, d.fingerprint);
                }
                Err(e) => {
                    eprintln!("[warn] {name}: read {}/ failed: {e}", d.key);
                    errors += 1;
                }
            }
        }
        total_added += added;
        total_updated += updated;
        report.per_source.push((name.to_string(), added, updated, errors));
    }

    save_fps(&fps_path, &fps)?;
    if total_added + total_updated > 0 {
        crate::git::git_commit_all(
            &store.root,
            &format!("sync: +{total_added} ~{total_updated}"),
        )?;
    }
    Ok(report)
}

/// Registry of built-in adapters, resolved from config (or default paths).
pub fn registry(store: &Store, cfg: &Config) -> Vec<Box<dyn Adapter>> {
    let home = std::env::var("HOME").unwrap_or_default();
    let _ = store;
    let mut out: Vec<Box<dyn Adapter>> = Vec::new();
    let claude = cfg
        .sources
        .claude_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(&home).join(".claude/projects"));
    out.push(Box::new(crate::adapters::claude_code::ClaudeCode::new(claude)));
    let db = cfg
        .sources
        .opencode_db
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(&home).join(".local/share/opencode/opencode.db"));
    let storage = cfg
        .sources
        .opencode_storage
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(&home).join(".local/share/opencode/storage"));
    out.push(Box::new(crate::adapters::opencode::OpenCode::new(db, storage)));
    let codex = cfg
        .sources
        .codex_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(&home).join(".codex/sessions"));
    out.push(Box::new(crate::adapters::codex::Codex::new(codex)));
    let gemini = cfg
        .sources
        .gemini_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(&home).join(".gemini"));
    out.push(Box::new(crate::adapters::gemini_cli::GeminiCli::new(gemini)));
    let crush = cfg.sources.crush_dirs.clone().unwrap_or_else(|| {
        vec![
            std::path::PathBuf::from(&home).join(".local/share/crush"),
            std::path::PathBuf::from(&home).join(".config/crush"),
        ]
    });
    out.push(Box::new(crate::adapters::crush::Crush::new(crush)));
    let pi = cfg
        .sources
        .pi_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(&home).join(".pi"));
    out.push(Box::new(crate::adapters::pi::Pi::new(pi)));
    out
}
