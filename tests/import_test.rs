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

fn chats_md(st: &aiyou::store::Store) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = walkdir::WalkDir::new(st.chats_dir().join("chatgpt"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .map(|e| e.path().to_path_buf())
        .collect();
    v.sort();
    v
}

#[test]
fn chatgpt_progressive_import() {
    let (_tmp, st, cfg) = setup();
    // first import
    let r = aiyou::importers::import_dump(&st, &cfg, "chatgpt", &fixture("chatgpt/conversations.json")).unwrap();
    assert_eq!((r.conversations, r.added, r.updated, r.unchanged), (1, 1, 0, 0));
    let files = chats_md(&st);
    assert_eq!(files.len(), 1);
    let name = files[0].file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("rust-help-"), "name: {name}");
    let (meta, msgs) = st.read_session_jsonl_at(&files[0].with_extension("jsonl")).unwrap();
    assert_eq!(meta.source, "chatgpt");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "how do lifetimes work");
    assert_eq!(msgs[1].role, "assistant");
    // identity: external id absent -> title slug + hash8 of first user msg
    assert!(meta.id.starts_with("rust-help-"), "id: {}", meta.id);

    // identical re-import: no-op
    let r = aiyou::importers::import_dump(&st, &cfg, "chatgpt", &fixture("chatgpt/conversations.json")).unwrap();
    assert_eq!((r.conversations, r.added, r.updated, r.unchanged), (1, 0, 0, 1));

    // newer dump with extra message
    let r = aiyou::importers::import_dump(&st, &cfg, "chatgpt", &fixture("chatgpt/conversations_v2.json")).unwrap();
    assert_eq!((r.conversations, r.added, r.updated, r.unchanged), (1, 0, 1, 0));
    let files = chats_md(&st);
    assert_eq!(files.len(), 1); // no dupes
    let (_, msgs) = st.read_session_jsonl_at(&files[0].with_extension("jsonl")).unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[2].text, "and borrowing?");

    // manifest: 2 entries (identical dump was a no-op)
    let raw = std::fs::read_to_string(st.imports_dir().join("manifest.json")).unwrap();
    assert!(raw.contains("file_sha256"));
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["entries"].as_array().unwrap().len(), 2);
}

#[test]
fn import_commits_to_git() {
    let (_tmp, st, cfg) = setup();
    let before = git_commits(&st);
    aiyou::importers::import_dump(&st, &cfg, "chatgpt", &fixture("chatgpt/conversations.json")).unwrap();
    assert_eq!(git_commits(&st), before + 1);
    // no-op import does not commit
    let mid = git_commits(&st);
    aiyou::importers::import_dump(&st, &cfg, "chatgpt", &fixture("chatgpt/conversations.json")).unwrap();
    assert_eq!(git_commits(&st), mid);
}

fn git_commits(st: &aiyou::store::Store) -> usize {
    let out = std::process::Command::new("git")
        .current_dir(&st.root)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

#[test]
fn import_rejects_unknown_source() {
    let (_tmp, st, cfg) = setup();
    let err = aiyou::importers::import_dump(&st, &cfg, "slack", &fixture("chatgpt/conversations.json"));
    assert!(err.is_err());
}
