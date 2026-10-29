use aiyou::model::{Msg, SessionMeta};
use aiyou::profile::client::LlmClient;
use axum::routing::post;
use axum::Json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn setup() -> (tempfile::TempDir, aiyou::store::Store, aiyou::config::Config) {
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    let mut cfg = aiyou::config::Config::default();
    cfg.profile.workers = 2;
    cfg.profile.map_context_chars = 10_000;
    (tmp, st, cfg)
}

fn write_session(st: &aiyou::store::Store, cfg: &aiyou::config::Config, id: &str, text: &str) {
    let meta = SessionMeta {
        source: "claude-code".into(),
        id: id.into(),
        project: "p".into(),
        title: format!("session {id}"),
        created_ms: 1,
        updated_ms: 2,
    };
    let mut msgs = vec![Msg {
        id: "m1".into(),
        role: "user".into(),
        ts_ms: 10,
        text: text.into(),
        tool: None,
    }];
    st.write_session(&meta, &mut msgs, cfg).unwrap();
}

/// Mock LLM: first call returns map output, subsequent calls return reduced traits.
fn mock_client() -> (LlmClient, std::sync::Arc<AtomicUsize>) {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let calls2 = calls.clone();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        rt.block_on(async move {
            let app = axum::Router::new().route(
                "/v1/chat/completions",
                post(move |Json(body): Json<serde_json::Value>| {
                    let calls = calls2.clone();
                    async move {
                        let n = calls.fetch_add(1, Ordering::SeqCst);
                        let user = body["messages"][1]["content"].as_str().unwrap_or("").to_string();
                        let content = if n == 0 {
                            assert!(user.contains("----- SESSION"), "map should receive transcripts");
                            r#"{"traits":[{"trait":"Prefers terse answers","category":"communication","evidence":"be brief","confidence":0.9},{"trait":"Loves Rust","category":"tech-stack","evidence":"rust is great","confidence":0.8}]}"#
                        } else {
                            assert!(user.contains("candidate_traits"), "reduce should receive candidates");
                            r#"{"traits":[{"id":"prefers-terse-answers","trait":"Prefers terse answers","category":"communication","confidence":0.92,"evidence":[{"source":"claude-code","session":"s1","quote":"be brief"}]},{"id":"loves-rust","trait":"Loves Rust","category":"tech-stack","confidence":0.85,"evidence":[{"source":"claude-code","session":"s1","quote":"rust is great"}]}]}"#
                        };
                        axum::Json(serde_json::json!({
                            "choices": [{"message": {"content": content}}]
                        }))
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let _ = ready_tx.send(format!("http://{addr}/v1"));
            axum::serve(listener, app).await.unwrap();
        });
    });
    let base = ready_rx.recv().unwrap();
    (LlmClient::new(base, "k".into(), "m".into()), calls)
}

#[test]
fn pipeline_end_to_end() {
    let (_tmp, st, cfg) = setup();
    write_session(&st, &cfg, "s1", "hello world");
    write_session(&st, &cfg, "s2", "second session");

    let (client, calls) = mock_client();
    aiyou::profile::pipeline::run_with_client(
        &st,
        &cfg,
        aiyou::profile::pipeline::Opts { if_changed: false, yes: true },
        &client,
    )
    .unwrap();

    // traits.json written with valid categories and evidence
    let doc = aiyou::profile::traits::ProfileDoc::load(&st.traits_path()).unwrap();
    assert_eq!(doc.traits.len(), 2);
    assert!(doc
        .traits
        .iter()
        .all(|t| aiyou::profile::traits::CATEGORIES.contains(&t.category.as_str())));
    assert!(doc.traits.iter().any(|t| t.statement == "Loves Rust" && t.confidence > 0.8));
    assert!(doc.traits.iter().all(|t| !t.evidence.is_empty()));

    // SKILL.md rendered with frontmatter
    let skill = std::fs::read_to_string(st.skill_path()).unwrap();
    assert!(skill.starts_with("---\nname: me\n"));
    assert!(skill.contains("## How to talk to me"));
    assert!(skill.contains("Prefers terse answers"));

    // audit run file exists
    let runs: Vec<_> = std::fs::read_dir(st.runs_dir()).unwrap().flatten().collect();
    assert_eq!(runs.len(), 1);

    // map + reduce called
    assert!(calls.load(Ordering::SeqCst) >= 2);

    // git committed
    let out = std::process::Command::new("git")
        .current_dir(&st.root)
        .args(["log", "-1", "--format=%s"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("profile:"));
}

#[test]
fn pipeline_if_changed_noop() {
    let (_tmp, st, cfg) = setup();
    write_session(&st, &cfg, "s1", "hello");
    let (client, calls) = mock_client();
    let opts = aiyou::profile::pipeline::Opts { if_changed: true, yes: true };
    aiyou::profile::pipeline::run_with_client(&st, &cfg, opts.clone(), &client).unwrap();
    let after_first = calls.load(Ordering::SeqCst);
    // second run: nothing changed -> no-op, no additional LLM calls
    aiyou::profile::pipeline::run_with_client(&st, &cfg, opts, &client).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), after_first);
}

// ---------- approval flow ----------

use aiyou::profile::pipeline::{run_with_client, Opts};
use aiyou::profile::review::{self, Proposal};
use aiyou::profile::traits::ProfileDoc;

fn staged(client: &LlmClient, st: &aiyou::store::Store, cfg: &aiyou::config::Config) {
    run_with_client(st, cfg, Opts { if_changed: false, yes: false }, client).unwrap();
}

#[test]
fn profile_stages_changes_without_touching_live_profile() {
    let (_tmp, st, cfg) = setup();
    write_session(&st, &cfg, "s1", "be brief, rust is great");
    let (client, _) = mock_client();
    staged(&client, &st, &cfg);

    assert!(!st.traits_path().exists(), "live profile must not change before approval");
    assert!(!st.skill_path().exists());
    let p = Proposal::load(&st).unwrap().expect("proposal staged");
    assert_eq!(p.changes.len(), 2);
    assert!(p.changes.iter().all(|c| c.kind == review::ChangeKind::Add));

    // a second run refuses to stack another proposal on top
    let err = run_with_client(&st, &cfg, Opts { if_changed: false, yes: false }, &client).unwrap_err();
    assert!(err.to_string().contains("aiyou review"));
}

#[test]
fn accept_all_applies_and_marks_chats_absorbed() {
    let (_tmp, st, cfg) = setup();
    write_session(&st, &cfg, "s1", "hello");
    let (client, calls) = mock_client();
    staged(&client, &st, &cfg);

    review::resolve(&st, &[], true).unwrap();
    let doc = ProfileDoc::load(&st.traits_path()).unwrap();
    assert_eq!(doc.traits.len(), 2);
    assert!(st.skill_path().exists());
    assert!(Proposal::load(&st).unwrap().is_none());

    // nothing new -> no further LLM calls
    let before = calls.load(Ordering::SeqCst);
    staged(&client, &st, &cfg);
    assert_eq!(calls.load(Ordering::SeqCst), before);
}

#[test]
fn partial_accept_keeps_the_rest_pending_and_reject_is_remembered() {
    let (_tmp, st, cfg) = setup();
    write_session(&st, &cfg, "s1", "hello");
    let (client, _) = mock_client();
    staged(&client, &st, &cfg);

    review::resolve(&st, &["loves-rust".into()], true).unwrap();
    let doc = ProfileDoc::load(&st.traits_path()).unwrap();
    assert_eq!(doc.traits.len(), 1);
    assert_eq!(doc.traits[0].id, "loves-rust");
    assert_eq!(Proposal::load(&st).unwrap().unwrap().changes.len(), 1);

    review::resolve(&st, &[], false).unwrap();
    assert!(Proposal::load(&st).unwrap().is_none());
    assert_eq!(review::load_rejected(&st), vec!["prefers-terse-answers".to_string()]);

    assert!(review::resolve(&st, &[], true).is_err(), "nothing pending any more");
}

#[test]
fn diff_keeps_unmentioned_traits_and_ignores_noise() {
    use aiyou::profile::traits::Trait;
    let t = |id: &str, st: &str, c: f32| Trait {
        id: id.into(),
        statement: st.into(),
        category: "preferences".into(),
        confidence: c,
        evidence: vec![],
        first_seen_ms: 1,
        last_seen_ms: 1,
    };
    let existing = vec![t("a", "A", 0.8), t("b", "B", 0.8), t("c", "C", 0.8)];
    // a: tiny confidence wobble (noise); b: reworded (update); d: new but previously rejected
    let merged = vec![t("a", "A", 0.85), t("b", "B2", 0.8), t("d", "D", 0.9)];
    let changes = review::diff(&existing, merged, &["c".into()], &["d".into()]);
    let kinds: Vec<_> = changes.iter().map(|c| (c.id.as_str(), c.kind)).collect();
    assert_eq!(kinds, vec![("b", review::ChangeKind::Update), ("c", review::ChangeKind::Retire)]);

    // applying: b updated, c removed, a untouched even though the reducer never "retained" c
    let mut doc = ProfileDoc { updated_at_ms: 0, traits: existing };
    review::apply(&mut doc, &changes, 5);
    let ids: Vec<_> = doc.traits.iter().map(|t| t.id.as_str()).collect();
    assert!(ids.contains(&"a") && ids.contains(&"b") && !ids.contains(&"c"));
    assert_eq!(doc.traits.iter().find(|t| t.id == "b").unwrap().statement, "B2");
}

#[test]
fn failed_batches_are_retried_not_forgotten() {
    // LLM that fails every map call with a non-JSON body
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        rt.block_on(async move {
            let app = axum::Router::new().route(
                "/v1/chat/completions",
                post(|| async { axum::Json(serde_json::json!({"choices":[{"message":{"content":"sorry, no"}}]})) }),
            );
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let _ = tx.send(format!("http://{}/v1", l.local_addr().unwrap()));
            axum::serve(l, app).await.unwrap();
        });
    });
    let bad = LlmClient::new(rx.recv().unwrap(), "k".into(), "m".into());

    let (_tmp, st, cfg) = setup();
    write_session(&st, &cfg, "s1", "hello");
    assert!(run_with_client(&st, &cfg, Opts { if_changed: false, yes: true }, &bad).is_err());

    // chat was NOT marked absorbed, so a healthy run picks it up
    let (good, calls) = mock_client();
    run_with_client(&st, &cfg, Opts { if_changed: false, yes: true }, &good).unwrap();
    assert!(calls.load(Ordering::SeqCst) >= 2);
    assert_eq!(ProfileDoc::load(&st.traits_path()).unwrap().traits.len(), 2);
}
