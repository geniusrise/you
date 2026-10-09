use std::path::Path;

use aiyou::adapters::Adapter;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn project_fixture() -> std::path::PathBuf {
    fixture("claude-code")
}

#[test]
fn claude_code_discovers_and_parses() {
    let a = aiyou::adapters::claude_code::ClaudeCode::new(project_fixture());
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key, "session-a");
    assert!(found[0].fingerprint > 0);
    let (meta, msgs) = a.read("session-a").unwrap();
    assert_eq!(meta.id, "session-a");
    assert_eq!(meta.source, "claude-code");
    assert_eq!(meta.project, "sample-project");
    assert_eq!(meta.title, "fix the bug");
    assert_eq!(meta.created_ms, 1790848800000);
    // u1 user, u2 assistant(text+tool_use -> 2 msgs), u3 tool_result -> tool msg
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "fix the bug");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "Looking at it");
    assert_eq!(msgs[2].role, "tool");
    assert_eq!(msgs[2].tool.as_deref(), Some("Bash"));
    assert_eq!(msgs[3].role, "tool");
    assert_eq!(msgs[3].text, "file1");
}

#[test]
fn claude_code_missing_dir_is_ok_empty() {
    let a = aiyou::adapters::claude_code::ClaudeCode::new(Path::new("/nonexistent/zzz").to_path_buf());
    assert!(a.discover().unwrap().is_empty());
}

#[test]
fn claude_code_syncs_into_store() {
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    let mut cfg = aiyou::config::Config::default();
    cfg.sources.claude_dir = Some(project_fixture());
    cfg.sources.enabled = Some(vec!["claude-code".into()]);
    let r = aiyou::sync::run(&st, &cfg, None).unwrap();
    assert_eq!(r.per_source, vec![("claude-code".to_string(), 1, 0, 0)]);
    let (meta, msgs) = st.read_session_jsonl("claude-code", "session-a").unwrap();
    assert_eq!(meta.id, "session-a");
    assert_eq!(msgs.len(), 4);
    let md = std::fs::read_to_string(st.chats_dir().join("claude-code/sample-project/session-a.md")).unwrap();
    assert!(md.contains("fix the bug"));
}
