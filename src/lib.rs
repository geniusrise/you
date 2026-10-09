pub mod adapters;
pub mod config;
pub mod cron;
pub mod git;
pub mod importers;
pub mod model;
pub mod profile;
pub mod render;
pub mod store;
pub mod sync;

use anyhow::Result;
use std::path::Path;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "aiyou", version, about = "Export, maintain and analyze all your AI agent chat history in one git-versioned store")]
pub struct Cli {
    /// Store root (default: $AIYOU_HOME or ~/.aiyou)
    #[arg(long, global = true)]
    pub root: Option<std::path::PathBuf>,

    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Create the store, config and git repo
    Init,
    /// Harvest chats from local harnesses (run from cron to keep updating)
    Sync {
        /// Restrict to these sources (repeatable)
        #[arg(long = "source")]
        sources: Vec<String>,
    },
    /// Import a dump from ChatGPT / Claude / Gemini (re-import newer dumps safely)
    Import {
        /// One of: chatgpt, claude-ai, gemini
        source: String,
        /// Path to dump file (.json) or directory or .zip
        path: PathBuf,
    },
    /// Run the LLM swarm over all chats to (re)build your profile + SKILL.md
    Profile {
        /// Skip if no chats changed since last run (cron-friendly)
        #[arg(long)]
        if_changed: bool,
        /// Override model from config
        #[arg(long)]
        model: Option<String>,
        /// Override worker count from config
        #[arg(long)]
        workers: Option<usize>,
    },
    /// Install the generated SKILL.md into a harness skill directory
    ProfileInstall {
        /// One of: claude, opencode, crush, codex
        #[arg(long)]
        target: Option<String>,
        /// Custom agents/skills directory
        #[arg(long)]
        agents_dir: Option<PathBuf>,
    },
    /// Show per-source counts, last sync and last profile info
    Status,
    /// Print crontab lines for scheduled sync + profile
    Cron,
}





pub fn run(root: &Path, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Init => {
            let store = store::Store::open(root)?;
            store.init()?;
            println!("initialized store at {}", root.display());
            Ok(())
        }
        Cmd::Status => {
            status(&root)
        }
        Cmd::Cron => {
            println!("{}", cron::cron_lines(root));
            Ok(())
        }
        Cmd::Sync { sources } => {
            let store = ensure_store(root)?;
            let cfg = config::Config::load(&store.config_path())?;
            let report = sync::run(&store, &cfg, if sources.is_empty() { None } else { Some(&sources) })?;
            for (s, add, upd, err) in &report.per_source {
                println!("{s}: +{add} ~{upd} !{err}");
            }
            Ok(())
        }
        Cmd::Import { source, path } => {
            let store = ensure_store(root)?;
            let cfg = config::Config::load(&store.config_path())?;
            let rep = importers::import_dump(&store, &cfg, &source, &path)?;
            println!(
                "{}: {} conversations, +{} new, ~{} updated, ={} unchanged",
                source, rep.conversations, rep.added, rep.updated, rep.unchanged
            );
            Ok(())
        }
        Cmd::Profile { if_changed, model, workers } => {
            let store = ensure_store(root)?;
            let mut cfg = config::Config::load(&store.config_path())?;
            if let Some(m) = model {
                cfg.profile.model = m;
            }
            if let Some(w) = workers {
                cfg.profile.workers = w;
            }
            profile::pipeline::run(&store, &cfg, profile::pipeline::Opts { if_changed })
        }
        Cmd::ProfileInstall { target, agents_dir } => {
            let store = ensure_store(root)?;
            profile::render_skill::install(&store, target.as_deref(), agents_dir.as_deref())
        }
    }
}

fn ensure_store(root: &Path) -> Result<store::Store> {
    let store = store::Store::open(root)?;
    if !store.config_path().exists() {
        anyhow::bail!(
            "no store at {} — run `aiyou init` first",
            root.display()
        );
    }
    Ok(store)
}

fn status(root: &Path) -> Result<()> {
    let store = store::Store::open(root)?;
    if !store.config_path().exists() {
        println!("no store at {} — run `aiyou init`", root.display());
        return Ok(());
    }
    let mut sources: std::collections::BTreeMap<String, usize> = Default::default();
    if store.chats_dir().exists() {
        for entry in walkdir::WalkDir::new(store.chats_dir())
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "md") {
                if let Some(src) = entry.path().strip_prefix(store.chats_dir()).ok().and_then(|p| p.iter().next()) {
                    *sources.entry(src.to_string_lossy().to_string()).or_default() += 1;
                }
            }
        }
    }
    println!("store: {}", root.display());
    for s in config::SOURCES {
        let n = sources.get(*s).copied().unwrap_or(0);
        println!("  {s:<14} {n:>5} sessions");
    }
    if store.traits_path().exists() {
        let meta = std::fs::metadata(&store.traits_path())?;
        if let Some(t) = chrono::DateTime::<chrono::Utc>::from_timestamp(
            meta.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64,
            0,
        ) {
            println!("last profile: {t}");
        }
    } else {
        println!("last profile: never");
    }
    if let Some(c) = git::last_commit(&store.root) {
        println!("git: {c}");
    }
    Ok(())
}
