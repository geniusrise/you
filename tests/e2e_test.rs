use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// Full user journey in a temp store: init -> sync -> import (progressive) ->
/// profile (against a mock LLM) -> install skill.
#[test]
fn e2e_full_journey() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("store");
    std::env::set_var("AIYOU_HOME", &root);
    let _guard = EnvGuard;

    // init via the public API (equivalent to `aiyou init`)
    let store = aiyou::store::Store::open(&root).unwrap();
    store.init().unwrap();
    assert!(root.join(".git").is_dir());

    // sync: point config at fixtures
    let mut cfg = aiyou::config::Config::load(&store.config_path()).unwrap();
    cfg.sources.claude_dir = Some(fixture("claude-code"));
    cfg.sources.enabled = Some(vec!["claude-code".into()]);
    cfg.sources.enabled.as_mut().unwrap().push("chatgpt".into());
    cfg.save(&store.config_path()).unwrap();
    let rep = aiyou::sync::run(&store, &cfg, None).unwrap();
    assert_eq!(rep.per_source, vec![("claude-code".to_string(), 1, 0, 0)]);
    assert!(store.chats_dir().join("claude-code/sample-project/session-a.md").is_file());

    // import: twice (identical + newer)
    let r1 = aiyou::importers::import_dump(&store, &cfg, "chatgpt", &fixture("chatgpt/conversations.json")).unwrap();
    assert_eq!(r1.added, 1);
    let r2 = aiyou::importers::import_dump(&store, &cfg, "chatgpt", &fixture("chatgpt/conversations_v2.json")).unwrap();
    assert_eq!(r2.updated, 1);

    // profile against mock LLM
    let (client, _calls) = mock_client();
    aiyou::profile::pipeline::run_with_client(
        &store,
        &cfg,
        aiyou::profile::pipeline::Opts { if_changed: false },
        &client,
    )
    .unwrap();
    let skill = std::fs::read_to_string(store.skill_path()).unwrap();
    assert!(skill.starts_with("---\nname: user-persona\n"));
    assert!(store.traits_path().is_file());

    // install into a custom agents dir
    aiyou::profile::render_skill::install(&store, None, Some(&tmp.path().join("agents"))).unwrap();
    let installed = std::fs::read_to_string(tmp.path().join("agents/user-persona/SKILL.md")).unwrap();
    assert_eq!(installed, skill);

    // git log: init + sync + import + import + profile
    let out = std::process::Command::new("git")
        .current_dir(&root)
        .args(["log", "--oneline"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(log.contains("sync: +1"));
    assert!(log.contains("import: chatgpt"));
    assert!(log.contains("profile:"));
}

struct EnvGuard;
impl Drop for EnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("AIYOU_HOME");
    }
}

use axum::routing::post;
use axum::Json;

fn mock_client() -> (aiyou::profile::client::LlmClient, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls2 = calls.clone();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        rt.block_on(async move {
            let app = axum::Router::new().route(
                "/v1/chat/completions",
                post(move |Json(_body): Json<serde_json::Value>| {
                    let calls = calls2.clone();
                    async move {
                        let n = calls.fetch_add(1, Ordering::SeqCst);
                        let content = if n == 0 {
                            r#"{"traits":[{"trait":"Likes tests","category":"workflow","evidence":"writes tests","confidence":0.9}]}"#
                        } else {
                            r#"{"traits":[{"id":"likes-tests","trait":"Likes tests","category":"workflow","confidence":0.9,"evidence":[{"source":"claude-code","session":"session-a","quote":"writes tests"}]}]}"#
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
    (aiyou::profile::client::LlmClient::new(base, "k".into(), "m".into()), calls)
}

use std::sync::atomic::Ordering;
