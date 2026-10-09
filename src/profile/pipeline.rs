use super::client::LlmClient;
use super::traits::{batch_chats, select_chats, ChatRef, Evidence, ProfileDoc, Trait};
use crate::config::Config;
use crate::store::Store;
use anyhow::{Context, Result};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct Opts {
    pub if_changed: bool,
}

#[derive(Serialize)]
struct RunAudit {
    ran_at_ms: i64,
    batches: usize,
    sessions_covered: usize,
    sessions_total: usize,
    traits: usize,
    failed_batches: Vec<String>,
    full_run: bool,
}

pub const MAP_SYSTEM: &str = "You extract user traits from coding/chat transcripts. Return strict JSON {\"traits\":[{\"trait\":\"...\",\"category\":\"one of personality|communication|preferences|workflow|tech-stack|values|goals\",\"evidence\":\"quote\",\"confidence\":0.0-1.0}]} about the HUMAN user only, never the assistant. Be specific and grounded in the transcript; prefer fewer, well-supported traits over many vague ones.";

pub const REDUCE_SYSTEM: &str = "You merge candidate user traits and an existing profile into one canonical profile. Return strict JSON {\"traits\":[{\"id\":\"slug-of-statement\",\"trait\":\"...\",\"category\":\"one of personality|communication|preferences|workflow|tech-stack|values|goals\",\"confidence\":0.0-1.0,\"evidence\":[{\"source\":\"...\",\"session\":\"...\",\"quote\":\"...\"}]}]}. Rules: merge duplicates keeping the clearest statement (preserve the existing id when the statement is essentially the same); combine their evidence (max 5 each, no invented evidence); confidence = your merged judgment. Only traits about the HUMAN user.";

pub fn run(store: &Store, cfg: &Config, opts: Opts) -> Result<()> {
    let client = LlmClient::from_config(&cfg.profile, None)?;
    run_with_client(store, cfg, opts, &client)
}

pub fn run_with_client(store: &Store, cfg: &Config, opts: Opts, client: &LlmClient) -> Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    let traits_path = store.traits_path();
    let existing = if traits_path.exists() {
        ProfileDoc::load(&traits_path).ok().unwrap_or_default()
    } else {
        ProfileDoc::default()
    };
    let last_epoch = existing.updated_at_ms;

    let all = select_chats(store, &cfg.profile.exclude_sources)?;
    // incremental: only chats whose md mtime > last profile run
    let (fresh, full_run): (Vec<ChatRef>, bool) = if last_epoch == 0 {
        (all.clone(), true)
    } else {
        let fresh: Vec<ChatRef> = all
            .clone()
            .into_iter()
            .filter(|c| {
                c.path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0)
                    > last_epoch
            })
            .collect();
        let f = fresh.clone();
        (f, false)
    };

    if fresh.is_empty() {
        if opts.if_changed {
            println!("profile: no changes since last run");
            return Ok(());
        }
        println!("profile: no chats to analyze (nothing changed since last run; use a fresh store or touch chats)");
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

    runtime.block_on(async {
        let mut jobs = Vec::new();
        for (i, batch) in batches.into_iter().enumerate() {
            let permit_sem = semaphore.clone();
            let client = client.clone();
            jobs.push(tokio::spawn(async move {
                let _permit = permit_sem.acquire_owned().await?;
                type MapTexts = Result<(usize, String, Vec<(String, String)>)>;
                let texts = tokio::task::spawn_blocking(move || -> MapTexts {
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
                    Ok((i, combined, refs))
                })
                .await??;
                let (i, combined, refs) = texts;
                let out = client.chat(MAP_SYSTEM, &combined).await?;
                Ok::<_, anyhow::Error>((i, out, refs))
            }));
        }
        for job in jobs {
            match job.await {
                Ok(Ok((i, out, refs))) => {
                    match LlmClient::salvage_json(&out) {
                        Ok(v) => {
                            // attach session refs to evidence so the reducer can cite
                            let v = attach_refs(v, &refs);
                            candidates.push(v);
                        }
                        Err(e) => failed_batches.push(format!("batch {i}: {e}")),
                    }
                }
                Ok(Err(e)) => failed_batches.push(format!("batch: {e}")),
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
    let reduce_out = client.chat_blocking(
        REDUCE_SYSTEM,
        &serde_json::to_string_pretty(&reduce_input)?,
    )?;
    let merged = LlmClient::salvage_json(&reduce_out)?;
    let now_traits: Vec<Trait> = merged["traits"]
        .as_array()
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
                            if let Some(quote) = ev["quote"].as_str() {
                                Some(Evidence {
                                    source: ev["source"].as_str().unwrap_or("unknown").to_string(),
                                    session: ev["session"].as_str().unwrap_or("").to_string(),
                                    quote: quote.chars().take(200).collect(),
                                })
                            } else {
                                ev["evidence"].as_str().map(|q| Evidence {
                                    source: "unknown".into(),
                                    session: String::new(),
                                    quote: q.chars().take(200).collect(),
                                })
                            }
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
                confidence: t["confidence"].as_f64().unwrap_or(0.5) as f32,
                evidence,
                first_seen_ms: now,
                last_seen_ms: now,
            })
        })
        .collect();

    let mut doc = ProfileDoc { updated_at_ms: now, traits: now_traits };
    // carry over first_seen from previous profile for matching ids
    for t in doc.traits.iter_mut() {
        if let Some(prev) = existing.traits.iter().find(|p| p.id == t.id) {
            t.first_seen_ms = prev.first_seen_ms;
        }
    }
    doc.normalize(now);
    doc.save(&traits_path)?;

    // render skill in the same run/commit
    let skill = super::render_skill::render(&doc);
    std::fs::create_dir_all(store.profile_dir())?;
    std::fs::write(store.skill_path(), skill)?;

    // audit trail
    let audit = RunAudit {
        ran_at_ms: now,
        batches: candidates.len() + failed_batches.len(),
        sessions_covered: count_covered(&candidates),
        sessions_total: all.len(),
        traits: doc.traits.len(),
        failed_batches: failed_batches.clone(),
        full_run,
    };
    std::fs::create_dir_all(store.runs_dir())?;
    std::fs::write(
        store.runs_dir().join(format!("{now}.json")),
        serde_json::to_string_pretty(&audit)?,
    )?;

    crate::git::git_commit_all(
        &store.root,
        &format!("profile: {} traits from {} sessions", doc.traits.len(), audit.sessions_covered),
    )?;
    println!("profile: {} traits written to {}", doc.traits.len(), traits_path.display());
    if !failed_batches.is_empty() {
        eprintln!("[warn] {} batches failed and were skipped", failed_batches.len());
    }
    Ok(())
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

fn count_covered(candidates: &[serde_json::Value]) -> usize {
    candidates
        .iter()
        .filter_map(|c| c["traits"].as_array().map(|a| a.len()))
        .sum()
}

