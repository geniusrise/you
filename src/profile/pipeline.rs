use super::client::LlmClient;
use super::review::{self, Proposal, State};
use super::traits::{batch_chats, select_chats, ChatRef, Evidence, Trait};
use crate::config::Config;
use crate::store::Store;
use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct Opts {
    pub if_changed: bool,
    /// Apply the proposal immediately instead of staging it for review.
    pub yes: bool,
}

#[derive(Serialize)]
struct RunAudit {
    ran_at_ms: i64,
    batches: usize,
    sessions_covered: usize,
    sessions_total: usize,
    changes: usize,
    failed_batches: Vec<String>,
    full_run: bool,
}

pub const MAP_SYSTEM: &str = "You extract user traits from coding/chat transcripts. Return strict JSON {\"traits\":[{\"trait\":\"...\",\"category\":\"one of personality|communication|preferences|workflow|tech-stack|values|goals\",\"evidence\":\"quote\",\"confidence\":0.0-1.0}]} about the HUMAN user only, never the assistant. Be specific and grounded in the transcript; prefer fewer, well-supported traits over many vague ones. Write each trait as an instruction an assistant could act on (e.g. \"Wants answers under five lines unless asked to elaborate\"), not as a label.";

pub const REDUCE_SYSTEM: &str = "You merge candidate user traits and an existing profile into one canonical profile. Return strict JSON {\"traits\":[{\"id\":\"slug-of-statement\",\"trait\":\"...\",\"category\":\"one of personality|communication|preferences|workflow|tech-stack|values|goals\",\"confidence\":0.0-1.0,\"evidence\":[{\"source\":\"...\",\"session\":\"...\",\"quote\":\"...\"}]}],\"retired\":[\"id\"]}. Rules: merge duplicates keeping the clearest statement (preserve the existing id when the statement is essentially the same); combine their evidence (max 5 each, no invented evidence); confidence = your merged judgment. Only traits about the HUMAN user. Do not repeat existing traits that are unchanged. List in \"retired\" the ids of existing traits that newer evidence clearly contradicts or that the user has outgrown; never retire on mere absence. Never re-propose anything listed in rejected_ids.";

fn hash_file(path: &std::path::Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    hex::encode(Sha256::digest(&bytes))
}

fn key(c: &ChatRef) -> String {
    format!("{}/{}", c.meta.source, c.meta.id)
}

pub fn run(store: &Store, cfg: &Config, opts: Opts) -> Result<()> {
    if opts.if_changed && review::Proposal::load(store)?.is_some() {
        println!("profile: a proposal is already waiting — run `aiyou review`");
        return Ok(());
    }
    let client = LlmClient::from_config(&cfg.profile, None)?;
    run_with_client(store, cfg, opts, &client)
}

pub fn run_with_client(store: &Store, cfg: &Config, opts: Opts, client: &LlmClient) -> Result<()> {
    if let Some(p) = Proposal::load(store)? {
        if opts.if_changed {
            println!("profile: a proposal is already waiting — run `aiyou review`");
            return Ok(());
        }
        anyhow::bail!(
            "{} change(s) are waiting for your decision — run `aiyou review`, then `aiyou accept` or `aiyou reject`",
            p.changes.len()
        );
    }
    let now = chrono::Utc::now().timestamp_millis();
    let existing = if store.traits_path().exists() {
        super::traits::ProfileDoc::load(&store.traits_path()).ok().unwrap_or_default()
    } else {
        super::traits::ProfileDoc::default()
    };
    let state = State::load(store);
    let rejected = review::load_rejected(store);

    // incremental: a chat is fresh when its content differs from what the
    // profile last absorbed (robust against mtime churn from re-syncs)
    let all = select_chats(store, &cfg.profile.exclude_sources)?;
    let mut hashes: BTreeMap<String, String> = BTreeMap::new();
    let fresh: Vec<ChatRef> = all
        .iter()
        .filter(|c| {
            let h = hash_file(&c.path);
            let changed = state.processed.get(&key(c)) != Some(&h);
            hashes.insert(key(c), h);
            changed
        })
        .cloned()
        .collect();
    let full_run = state.processed.is_empty();

    if fresh.is_empty() {
        println!("profile: no changes since last run");
        return Ok(());
    }

    let batches = batch_chats(fresh, cfg.profile.map_context_chars);
    println!(
        "profile: {} sessions in {} batches ({} total sessions, {} run)",
        batches.iter().map(|b| b.len()).sum::<usize>(),
        batches.len(),
        all.len(),
        if full_run { "full" } else { "incremental" }
    );

    // ---- map phase (the swarm): concurrent workers over batches ----
    let workers = cfg.profile.workers.max(1);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(workers));
    let runtime = tokio::runtime::Runtime::new()?;

    let mut failed_batches: Vec<String> = Vec::new();
    let mut candidates: Vec<serde_json::Value> = Vec::new();
    let mut covered: Vec<String> = Vec::new();

    runtime.block_on(async {
        let mut jobs = Vec::new();
        for (i, batch) in batches.into_iter().enumerate() {
            let permit_sem = semaphore.clone();
            let client = client.clone();
            jobs.push(tokio::spawn(async move {
                let _permit = permit_sem.acquire_owned().await?;
                type MapTexts = Result<(String, Vec<(String, String)>)>;
                let (combined, refs) = tokio::task::spawn_blocking(move || -> MapTexts {
                    let mut combined = String::new();
                    let mut refs = Vec::new();
                    for c in &batch {
                        let body = std::fs::read_to_string(&c.path)
                            .with_context(|| format!("reading {}", c.path.display()))?;
                        let body: String = body.chars().take(40_000).collect();
                        combined.push_str(&format!("\n----- SESSION {} ({}) -----\n", c.meta.id, c.meta.source));
                        combined.push_str(&body);
                        refs.push((c.meta.source.clone(), c.meta.id.clone()));
                    }
                    Ok((combined, refs))
                })
                .await??;
                let out = client.chat(MAP_SYSTEM, &combined).await?;
                Ok::<_, anyhow::Error>((i, out, refs))
            }));
        }
        for job in jobs {
            match job.await {
                Ok(Ok((i, out, refs))) => match LlmClient::salvage_json(&out) {
                    Ok(v) => {
                        candidates.push(attach_refs(v, &refs));
                        covered.extend(refs.iter().map(|(s, id)| format!("{s}/{id}")));
                    }
                    Err(e) => failed_batches.push(format!("batch {i}: {e}")),
                },
                Ok(Err(e)) => failed_batches.push(format!("batch: {e:#}")),
                Err(e) => failed_batches.push(format!("join: {e}")),
            }
        }
    });

    if candidates.is_empty() {
        anyhow::bail!("all map batches failed: {}", failed_batches.join("; "));
    }

    // ---- reduce phase ----
    let mut reduce_input = serde_json::json!({ "candidate_traits": candidates });
    if !existing.traits.is_empty() {
        reduce_input["existing_profile"] = serde_json::to_value(&existing.traits)?;
    }
    if !rejected.is_empty() {
        reduce_input["rejected_ids"] = serde_json::to_value(&rejected)?;
    }
    let reduce_out = runtime.block_on(client.chat(REDUCE_SYSTEM, &serde_json::to_string_pretty(&reduce_input)?))?;
    let merged = LlmClient::salvage_json(&reduce_out)?;
    let merged_traits = parse_traits(&merged["traits"], now);
    let retired: Vec<String> = merged["retired"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let changes = review::diff(&existing.traits, merged_traits, &retired, &rejected);

    // only chats from batches that succeeded are marked absorbed — failed
    // batches are retried on the next run instead of silently skipped
    let processed: BTreeMap<String, String> = covered
        .iter()
        .filter_map(|k| hashes.get(k).map(|h| (k.clone(), h.clone())))
        .collect();

    let audit = RunAudit {
        ran_at_ms: now,
        batches: candidates.len() + failed_batches.len(),
        sessions_covered: covered.len(),
        sessions_total: all.len(),
        changes: changes.len(),
        failed_batches: failed_batches.clone(),
        full_run,
    };
    std::fs::create_dir_all(store.runs_dir())?;
    crate::store::write_atomic(
        &store.runs_dir().join(format!("{now}.json")),
        serde_json::to_string_pretty(&audit)?,
    )?;

    if !failed_batches.is_empty() {
        eprintln!("[warn] {} batches failed; those chats will be retried next run", failed_batches.len());
    }

    if changes.is_empty() {
        let mut st = state;
        st.processed.extend(processed);
        st.save(store)?;
        crate::git::git_commit_all(&store.root, "profile: analyzed, no changes")?;
        println!("profile: analyzed {} sessions, your profile is already up to date", covered.len());
        return Ok(());
    }

    let proposal = Proposal { created_ms: now, changes, processed, failed_batches };
    proposal.save(store)?;
    println!("profile: {} proposed change(s) from {} sessions", proposal.changes.len(), covered.len());

    if opts.yes || cfg.profile.auto_accept {
        let (n, _) = review::resolve(store, &[], true)?;
        println!("profile: {n} change(s) applied -> {}", store.skill_path().display());
    } else {
        review::review(store)?;
    }
    Ok(())
}

fn parse_traits(raw: &serde_json::Value, now: i64) -> Vec<Trait> {
    raw.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|t| {
            let statement = t["trait"].as_str()?.to_string();
            if statement.trim().is_empty() {
                return None;
            }
            let mut category = t["category"].as_str().unwrap_or("preferences").to_string();
            if !super::traits::CATEGORIES.contains(&category.as_str()) {
                category = "preferences".into();
            }
            let evidence = t["evidence"]
                .as_array()
                .map(|evs| {
                    evs.iter()
                        .filter_map(|ev| {
                            let quote = ev["quote"].as_str().or_else(|| ev["evidence"].as_str())?;
                            Some(Evidence {
                                source: ev["source"].as_str().unwrap_or("unknown").to_string(),
                                session: ev["session"].as_str().unwrap_or("").to_string(),
                                quote: quote.chars().take(200).collect(),
                            })
                        })
                        .take(5)
                        .collect()
                })
                .unwrap_or_default();
            Some(Trait {
                id: t["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| super::traits::trait_id(&statement)),
                statement,
                category,
                confidence: t["confidence"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0) as f32,
                evidence,
                first_seen_ms: now,
                last_seen_ms: now,
            })
        })
        .collect()
}

fn attach_refs(mut v: serde_json::Value, refs: &[(String, String)]) -> serde_json::Value {
    if let Some(arr) = v["traits"].as_array_mut() {
        for t in arr {
            if t["evidence"].is_string() {
                let q = t["evidence"].as_str().unwrap_or_default().to_string();
                let (src, ses) = refs.first().cloned().unwrap_or(("unknown".into(), String::new()));
                t["evidence"] = serde_json::json!([{ "source": src, "session": ses, "quote": q }]);
            }
        }
    }
    v
}
