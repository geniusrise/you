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
#[command(name = "aiyou", version, about = "Teach every AI agent who you are: extract your personality and working style from your chats into a me skill, kept current with your approval")]
pub struct Cli {
    /// Store root (default: $AIYOU_HOME or ~/.aiyou)
    #[arg(long, global = true)]
    pub root: Option<std::path::PathBuf>,

    /// Print shell completions for the given shell and exit
    #[arg(long, global = true)]
    pub shell: Option<clap_complete::Shell>,

    #[command(subcommand)]
    pub cmd: Option<Cmd>,
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
    /// Run the LLM swarm over new chats and propose changes to your profile (review before they apply)
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
        /// Apply proposed changes immediately instead of waiting for review
        #[arg(long)]
        yes: bool,
    },
    /// Show the changes the last `profile` run proposes to your profile
    Review,
    /// Accept proposed profile changes (all, or the given ids)
    Accept {
        ids: Vec<String>,
    },
    /// Decline proposed profile changes (all, or the given ids); they won't be proposed again
    Reject {
        ids: Vec<String>,
    },
    /// Install your `me` skill into an agent (skill dir, context-file block, or stdout)
    #[command(alias = "profile-install")]
    Install {
        /// claude|opencode|crush|codex (skill dirs) or claude-md|gemini|codex-md|agents-md (context files)
        #[arg(long)]
        target: Option<String>,
        /// Custom agents/skills directory
        #[arg(long)]
        agents_dir: Option<PathBuf>,
        /// Insert/refresh a managed block in this context file (AGENTS.md, CLAUDE.md, ...)
        #[arg(long)]
        file: Option<PathBuf>,
        /// Print the skill to stdout
        #[arg(long)]
        print: bool,
    },
    /// Diagnose sources, config and the profile endpoint
    Doctor,
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
        Cmd::Status => status(root),
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
        Cmd::Profile { if_changed, model, workers, yes } => {
            let store = ensure_store(root)?;
            let mut cfg = config::Config::load(&store.config_path())?;
            if let Some(m) = model {
                cfg.profile.model = m;
            }
            if let Some(w) = workers {
                cfg.profile.workers = w;
            }
            profile::pipeline::run(&store, &cfg, profile::pipeline::Opts { if_changed, yes })
        }
        Cmd::Review => {
            let store = ensure_store(root)?;
            profile::review::review(&store)?;
            Ok(())
        }
        Cmd::Accept { ids } => resolve_cmd(root, &ids, true),
        Cmd::Reject { ids } => resolve_cmd(root, &ids, false),
        Cmd::Install { target, agents_dir, file, print } => {
            let store = ensure_store(root)?;
            profile::render_skill::install(&store, target.as_deref(), agents_dir.as_deref(), file.as_deref(), print)
        }
        Cmd::Doctor => doctor(root),
    }
}

fn resolve_cmd(root: &Path, ids: &[String], accept: bool) -> Result<()> {
    let store = ensure_store(root)?;
    let (n, left) = profile::review::resolve(&store, ids, accept)?;
    println!("{} {n} change(s){}", if accept { "accepted" } else { "rejected" }, if left > 0 { format!("; {left} still pending") } else { String::new() });
    if accept {
        println!("skill updated: {} — `aiyou install` to push it to your agents", store.skill_path().display());
    }
    Ok(())
}

fn doctor(root: &Path) -> Result<()> {
    let store = store::Store::open(root)?;
    if !store.config_path().exists() {
        println!("store     MISSING  {} — run `aiyou init`", root.display());
        return Ok(());
    }
    let cfg = config::Config::load(&store.config_path())?;
    println!("store     ok       {}", root.display());
    println!("git       {}", if std::process::Command::new("git").arg("--version").output().is_ok() { "ok" } else { "MISSING — history won't be versioned" });
    println!(
        "llm       {}  {} via ${}",
        if std::env::var(&cfg.profile.api_key_env).is_ok() { "ok      " } else { "NO KEY  " },
        cfg.profile.model,
        cfg.profile.api_key_env
    );
    println!("\nsources:");
    let mut adapters = sync::registry(&store, &cfg);
    if let Some(enabled) = &cfg.sources.enabled {
        adapters.retain(|a| enabled.iter().any(|s| s == a.name()));
    }
    for a in &adapters {
        match a.discover() {
            Ok(d) if d.is_empty() => println!("  {:<14} none found", a.name()),
            Ok(d) => println!("  {:<14} {} session(s)", a.name(), d.len()),
            Err(e) => println!("  {:<14} ERROR {e:#}", a.name()),
        }
    }
    match profile::review::Proposal::load(&store) {
        Ok(Some(p)) => println!("\nprofile   {} change(s) awaiting review (`aiyou review`)", p.changes.len()),
        Ok(None) => println!("\nprofile   no pending changes"),
        Err(e) => println!("\nprofile   proposal.json unreadable: {e:#}"),
    }
    Ok(())
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
        let meta = std::fs::metadata(store.traits_path())?;
        if let Some(t) = chrono::DateTime::<chrono::Utc>::from_timestamp(
            meta.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64,
            0,
        ) {
            println!("last profile: {t}");
        }
    } else {
        println!("last profile: never");
    }
    if let Ok(Some(p)) = profile::review::Proposal::load(&store) {
        println!("pending: {} profile change(s) — run `aiyou review`", p.changes.len());
    }
    if let Some(c) = git::last_commit(&store.root) {
        println!("git: {c}");
    }
    Ok(())
}
