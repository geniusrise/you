use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn setup() -> (tempfile::TempDir, aiyou::store::Store, aiyou::config::Config) {
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    (tmp, st, aiyou::config::Config::default())
}

#[test]
fn claude_ai_progressive() {
    let (_tmp, st, cfg) = setup();
    // older dump: 1 message
    let r = aiyou::importers::import_dump(&st, &cfg, "claude-ai", &fixture("claude-ai/conversations_v1.json")).unwrap();
    assert_eq!((r.added, r.updated, r.unchanged), (1, 0, 0));
    // newer dump: same uuid, 3 messages
    let r = aiyou::importers::import_dump(&st, &cfg, "claude-ai", &fixture("claude-ai/conversations.json")).unwrap();
    assert_eq!((r.added, r.updated, r.unchanged), (0, 1, 0));
    let all = st.list_sessions().unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].1.id.contains("conv-1"));
    let p = st.find_session("claude-ai", &all[0].1.id).unwrap();
    let (_, msgs) = st.read_session_jsonl_at(&p).unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[2].text, "fair");
}

#[test]
fn gemini_takeout_progressive() {
    let (_tmp, st, cfg) = setup();
    let r = aiyou::importers::import_dump(&st, &cfg, "gemini", &fixture("gemini/MyChat.json")).unwrap();
    assert_eq!((r.added, r.updated, r.unchanged), (1, 0, 0));
    let all = st.list_sessions().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].1.source, "gemini-import");
    let p = st.find_session("gemini-import", &all[0].1.id).unwrap();
    let (_, msgs) = st.read_session_jsonl_at(&p).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "how to bake sourdough");
    assert_eq!(msgs[1].role, "assistant");
    // identical re-import no-op
    let r = aiyou::importers::import_dump(&st, &cfg, "gemini", &fixture("gemini/MyChat.json")).unwrap();
    assert_eq!((r.added, r.updated, r.unchanged), (0, 0, 1));
}
