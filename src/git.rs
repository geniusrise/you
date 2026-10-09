use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

static GIT_WARNED: AtomicBool = AtomicBool::new(false);

fn git(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=aiyou@local", "-c", "user.name=aiyou"])
        .args(args)
        .output()
        .context("running git (is git installed?)")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(out)
}

fn warn_git_missing(e: anyhow::Error) {
    if !GIT_WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("[warn] git unavailable ({e}); continuing without version control");
    }
}

pub fn git_init(dir: &Path) -> Result<()> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    match git(dir, &["init"]) {
        Ok(_) => Ok(()),
        Err(e) => {
            warn_git_missing(e);
            Ok(())
        }
    }
}

/// Returns true if a commit was made, false if the tree was clean.
pub fn git_commit_all(dir: &Path, msg: &str) -> Result<bool> {
    let status = match Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=aiyou@local", "-c", "user.name=aiyou"])
        .args(["status", "--porcelain"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        Ok(o) => anyhow::bail!("git status failed: {}", String::from_utf8_lossy(&o.stderr)),
        Err(e) => {
            warn_git_missing(e.into());
            return Ok(false);
        }
    };
    if status.stdout.is_empty() {
        return Ok(false);
    }
    match git(dir, &["add", "-A"]).and_then(|_| git(dir, &["commit", "-q", "-m", msg])) {
        Ok(_) => Ok(true),
        Err(e) => {
            warn_git_missing(e);
            Ok(false)
        }
    }
}

pub fn last_commit(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["log", "-1", "--format=%h %ad %s", "--date=short"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
