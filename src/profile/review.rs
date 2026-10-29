//! Approval flow: the pipeline *proposes* changes to your profile; nothing
//! reaches `traits.json` / `SKILL.md` until you accept it.

use super::traits::{ProfileDoc, Trait};
use crate::store::{write_atomic, Store};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Add,
    Update,
    Retire,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub id: String,
    /// The trait as it would be after the change (absent for `retire`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Trait>,
    /// The trait as it is today (absent for `add`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Trait>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Proposal {
    pub created_ms: i64,
    pub changes: Vec<Change>,
    /// `source/session -> content hash` absorbed once the proposal is resolved.
    #[serde(default)]
    pub processed: BTreeMap<String, String>,
    #[serde(default)]
    pub failed_batches: Vec<String>,
}

/// Content hashes of chats the profile has already looked at.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub processed: BTreeMap<String, String>,
}

fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &std::path::Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl State {
    pub fn load(store: &Store) -> Self {
        load_json(&store.state_path())
    }
    pub fn save(&self, store: &Store) -> Result<()> {
        write_atomic(&store.state_path(), serde_json::to_string_pretty(self)?)
    }
}

impl Proposal {
    pub fn load(store: &Store) -> Result<Option<Self>> {
        let p = store.proposal_path();
        if !p.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
        Ok(Some(serde_json::from_str(&raw).context("parsing proposal.json")?))
    }
    pub fn save(&self, store: &Store) -> Result<()> {
        std::fs::create_dir_all(store.profile_dir())?;
        write_atomic(&store.proposal_path(), serde_json::to_string_pretty(self)?)
    }
}

pub fn load_rejected(store: &Store) -> Vec<String> {
    load_json(&store.rejected_path())
}

fn save_rejected(store: &Store, ids: &[String]) -> Result<()> {
    write_atomic(&store.rejected_path(), serde_json::to_string_pretty(ids)?)
}

/// Is `new` different enough from `old` to bother the user? Evidence-only
/// churn is deliberately ignored.
fn material(old: &Trait, new: &Trait) -> bool {
    old.statement != new.statement
        || old.category != new.category
        || (old.confidence - new.confidence).abs() >= 0.15
}

/// Deterministic diff between today's profile and the reducer's output.
///
/// Existing traits the reducer did not mention are *kept* — only ids listed in
/// `retired` are proposed for removal, so a forgetful model can't erase you.
pub fn diff(existing: &[Trait], merged: Vec<Trait>, retired: &[String], rejected: &[String]) -> Vec<Change> {
    let mut out = Vec::new();
    for m in merged {
        match existing.iter().find(|p| p.id == m.id) {
            None if rejected.contains(&m.id) => {}
            None => out.push(Change { kind: ChangeKind::Add, id: m.id.clone(), new: Some(m), previous: None }),
            Some(p) if material(p, &m) && !rejected.contains(&m.id) => out.push(Change {
                kind: ChangeKind::Update,
                id: m.id.clone(),
                new: Some(m),
                previous: Some(p.clone()),
            }),
            Some(_) => {}
        }
    }
    for id in retired {
        if let Some(p) = existing.iter().find(|p| &p.id == id) {
            if !out.iter().any(|c| &c.id == id) {
                out.push(Change { kind: ChangeKind::Retire, id: id.clone(), new: None, previous: Some(p.clone()) });
            }
        }
    }
    out
}

pub fn apply(doc: &mut ProfileDoc, changes: &[Change], now: i64) {
    for c in changes {
        match (c.kind, &c.new) {
            (ChangeKind::Add, Some(t)) => {
                if !doc.traits.iter().any(|x| x.id == c.id) {
                    doc.traits.push(t.clone());
                }
            }
            (ChangeKind::Update, Some(t)) => {
                if let Some(slot) = doc.traits.iter_mut().find(|x| x.id == c.id) {
                    let first = slot.first_seen_ms;
                    *slot = t.clone();
                    slot.first_seen_ms = first;
                } else {
                    doc.traits.push(t.clone());
                }
            }
            (ChangeKind::Retire, _) => doc.traits.retain(|x| x.id != c.id),
            _ => {}
        }
    }
    doc.normalize(now);
}

pub fn describe(c: &Change) -> String {
    let t = |t: &Option<Trait>| t.as_ref().map(|t| t.statement.clone()).unwrap_or_default();
    match c.kind {
        ChangeKind::Add => {
            let n = c.new.as_ref();
            format!(
                "  + {}  [{}]  {}\n      \"{}\"",
                c.id,
                n.map(|n| n.category.as_str()).unwrap_or("?"),
                t(&c.new),
                n.and_then(|n| n.evidence.first()).map(|e| e.quote.as_str()).unwrap_or("")
            )
        }
        ChangeKind::Update => format!(
            "  ~ {}\n      was: {} ({:.2})\n      now: {} ({:.2})",
            c.id,
            t(&c.previous),
            c.previous.as_ref().map(|p| p.confidence).unwrap_or(0.0),
            t(&c.new),
            c.new.as_ref().map(|n| n.confidence).unwrap_or(0.0)
        ),
        ChangeKind::Retire => format!("  - {}  {}", c.id, t(&c.previous)),
    }
}

/// Print the pending proposal. Returns false when nothing is pending.
pub fn review(store: &Store) -> Result<bool> {
    let Some(p) = Proposal::load(store)? else {
        println!("nothing to review");
        return Ok(false);
    };
    println!("{} pending change(s):\n", p.changes.len());
    for c in &p.changes {
        println!("{}", describe(c));
    }
    if !p.failed_batches.is_empty() {
        println!("\n{} batch(es) failed and will be retried next run", p.failed_batches.len());
    }
    println!("\naiyou accept [ids...]   apply (all if no ids)\naiyou reject [ids...]   decline (won't be proposed again)");
    Ok(true)
}

/// Accept or reject changes (all when `ids` is empty). Once nothing is left
/// pending, the analysed chats are marked as absorbed.
pub fn resolve(store: &Store, ids: &[String], accept: bool) -> Result<(usize, usize)> {
    let Some(mut p) = Proposal::load(store)? else {
        bail!("nothing pending — run `aiyou profile` first");
    };
    let (selected, remaining): (Vec<Change>, Vec<Change>) = p
        .changes
        .drain(..)
        .partition(|c| ids.is_empty() || ids.contains(&c.id));
    if selected.is_empty() {
        let known: Vec<String> = remaining.iter().map(|c| c.id.clone()).collect();
        bail!("no pending change matches {:?} (pending: {})", ids, known.join(", "));
    }
    let now = chrono::Utc::now().timestamp_millis();
    if accept {
        let mut doc = if store.traits_path().exists() {
            ProfileDoc::load(&store.traits_path()).unwrap_or_default()
        } else {
            ProfileDoc::default()
        };
        apply(&mut doc, &selected, now);
        std::fs::create_dir_all(store.profile_dir())?;
        doc.save(&store.traits_path())?;
        write_atomic(&store.skill_path(), super::render_skill::render(&doc))?;
    } else {
        let mut rejected = load_rejected(store);
        for c in selected.iter().filter(|c| c.kind != ChangeKind::Retire) {
            if !rejected.contains(&c.id) {
                rejected.push(c.id.clone());
            }
        }
        save_rejected(store, &rejected)?;
    }

    let (n_sel, n_left) = (selected.len(), remaining.len());
    if remaining.is_empty() {
        let mut state = State::load(store);
        state.processed.extend(std::mem::take(&mut p.processed));
        state.save(store)?;
        let _ = std::fs::remove_file(store.proposal_path());
    } else {
        p.changes = remaining;
        p.save(store)?;
    }
    crate::git::git_commit_all(
        &store.root,
        &format!("profile: {} {n_sel} change(s)", if accept { "accepted" } else { "rejected" }),
    )?;
    Ok((n_sel, n_left))
}
