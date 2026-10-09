use std::path::{Path, PathBuf};

use aiyou::adapters::Adapter;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn codex_parses_rollout() {
    let a = aiyou::adapters::codex::Codex::new(fixture("codex"));
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = a.read(&found[0].key).unwrap();
    assert_eq!(meta.source, "codex");
    assert_eq!(meta.project, "webapp");
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "add auth");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "Adding JWT");
    assert_eq!(msgs[2].role, "tool");
    assert_eq!(msgs[2].tool.as_deref(), Some("shell"));
    assert_eq!(msgs[3].role, "tool");
    assert_eq!(msgs[3].text, "src test");
}

#[test]
fn codex_missing_dir_ok() {
    let a = aiyou::adapters::codex::Codex::new(PathBuf::from("/nonexistent/codex"));
    assert!(a.discover().unwrap().is_empty());
}

#[test]
fn gemini_parses_chats_and_history() {
    let a = aiyou::adapters::gemini_cli::GeminiCli::new(fixture("gemini-cli"));
    let found = a.discover().unwrap();
    // one session from tmp/*/chats, one from history/
    assert!(found.len() >= 2, "found: {found:?}");
    let keys: Vec<&str> = found.iter().map(|d| d.key.as_str()).collect();
    assert!(keys.contains(&"session-b"));
    assert!(keys.contains(&"h1"));
    let (meta, msgs) = a.read("session-b").unwrap();
    assert_eq!(meta.source, "gemini-cli");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "explain traits");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "Traits are...");
}

#[test]
fn gemini_missing_dir_ok() {
    let a = aiyou::adapters::gemini_cli::GeminiCli::new(PathBuf::from("/nonexistent/gemini"));
    assert!(a.discover().unwrap().is_empty());
}
