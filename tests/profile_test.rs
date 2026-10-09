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
        aiyou::profile::pipeline::Opts { if_changed: false },
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
    assert!(skill.starts_with("---\nname: user-persona\n"));
    assert!(skill.contains("## Communication"));
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
    let opts = aiyou::profile::pipeline::Opts { if_changed: true };
    aiyou::profile::pipeline::run_with_client(&st, &cfg, opts.clone(), &client).unwrap();
    let after_first = calls.load(Ordering::SeqCst);
    // second run: nothing changed -> no-op, no additional LLM calls
    aiyou::profile::pipeline::run_with_client(&st, &cfg, opts, &client).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), after_first);
}
