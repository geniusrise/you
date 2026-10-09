use aiyou::model::{Msg, SessionMeta};

fn store() -> (tempfile::TempDir, aiyou::store::Store) {
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    (tmp, st)
}

#[test]
fn slug_shape() {
    assert_eq!(aiyou::render::slug("My Project: /run/media/x!"), "my-project-run-media-x");
    assert_eq!(aiyou::render::slug("???"), "untitled");
}

#[test]
fn stable_msg_id_is_deterministic() {
    let a = aiyou::model::stable_msg_id("user", "hi", 123);
    let b = aiyou::model::stable_msg_id("user", "hi", 123);
    let c = aiyou::model::stable_msg_id("user", "hi", 124);
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.len(), 64);
}

#[test]
fn redaction_masks_common_secrets() {
    let cases = [
        ("key sk-abcdefghijklmnop here", "key [REDACTED] here"),
        ("aws AKIAABCDEFGHIJKLMNOP", "aws [REDACTED]"),
        ("tok ghp_abcdefghijklmnopqrst", "tok [REDACTED]"),
        ("slack xoxb-1234567890-abcdef", "slack [REDACTED]"),
    ];
    for (input, expected) in cases {
        assert_eq!(aiyou::render::redact(input), expected, "input: {input}");
    }
    assert_eq!(aiyou::render::redact("no secrets"), "no secrets");
}

#[test]
fn write_and_roundtrip() {
    let (_tmp, st) = store();
    let meta = SessionMeta {
        source: "claude-code".into(),
        id: "s1".into(),
        project: "proj".into(),
        title: "T".into(),
        created_ms: 1,
        updated_ms: 2,
    };
    let mut msgs = vec![
        Msg { id: "b".into(), role: "user".into(), ts_ms: 20, text: "hi sk-abcdefghijklmnop".into(), tool: None },
        Msg { id: "a".into(), role: "assistant".into(), ts_ms: 10, text: "hello".into(), tool: Some("Bash".into()) },
    ];
    let cfg = aiyou::config::Config::default();
    st.write_session(&meta, &mut msgs, &cfg).unwrap();
    let (m2, msgs2) = st.read_session_jsonl("claude-code", "s1").unwrap();
    assert_eq!(m2.id, "s1");
    assert_eq!(msgs2.len(), 2);
    assert_eq!(msgs2[0].id, "a"); // sorted by ts
    assert_eq!(msgs2[1].text, "hi sk-abcdefghijklmnop"); // jsonl keeps original
    let md = std::fs::read_to_string(st.chats_dir().join("claude-code/proj/s1.md")).unwrap();
    assert!(md.contains("# T"));
    assert!(md.contains("**user**:"));
    assert!(md.contains("[REDACTED]"));
    assert!(md.contains("tool: `Bash`"));
    assert!(md.contains("claude-code · proj ·"));
    // list_sessions finds it
    let all = st.list_sessions().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].1.id, "s1");
}

#[test]
fn redact_all_redacts_jsonl() {
    let (_tmp, st) = store();
    let meta = SessionMeta {
        source: "opencode".into(),
        id: "o1".into(),
        project: "p".into(),
        title: "O".into(),
        created_ms: 1,
        updated_ms: 2,
    };
    let mut msgs = vec![Msg { id: "m".into(), role: "user".into(), ts_ms: 1, text: "sk-abcdefghijklmnop".into(), tool: None }];
    let cfg = aiyou::config::Config {
        redact_all: true,
        ..Default::default()
    };
    st.write_session(&meta, &mut msgs, &cfg).unwrap();
    let (_, msgs2) = st.read_session_jsonl("opencode", "o1").unwrap();
    assert_eq!(msgs2[0].text, "[REDACTED]");
}
