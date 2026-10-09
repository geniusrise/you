use std::path::{Path, PathBuf};

use aiyou::adapters::Adapter;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn fresh_store() -> (tempfile::TempDir, aiyou::store::Store) {
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    (tmp, st)
}

// ---------- antigravity ----------

#[test]
fn antigravity_parses_brain_messages() {
    let a = aiyou::adapters::antigravity::Antigravity::new(fixture("antigravity-cli"));
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key, "brain-1");
    let (meta, msgs) = a.read("brain-1").unwrap();
    assert_eq!(meta.source, "antigravity");
    // visible user + assistant messages only; hideFromUser filtered
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "restart the service");
    assert_eq!(msgs[1].role, "assistant");
}

#[test]
fn antigravity_missing_dir_ok() {
    let a = aiyou::adapters::antigravity::Antigravity::new(PathBuf::from("/nonexistent/ag"));
    assert!(a.discover().unwrap().is_empty());
}

// ---------- goose ----------

#[test]
fn goose_reads_sqlite_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let db = make_goose_db(tmp.path());
    let a = aiyou::adapters::goose::Goose::new(db);
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = a.read(&found[0].key).unwrap();
    assert_eq!(meta.source, "goose");
    assert_eq!(meta.title, "Project cleanup");
    assert_eq!(meta.project, "myproj");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "clean the project");
    assert_eq!(msgs[1].role, "assistant");
}

#[test]
fn goose_missing_db_ok() {
    let a = aiyou::adapters::goose::Goose::new(PathBuf::from("/nonexistent/goose.db"));
    assert!(a.discover().unwrap().is_empty());
}

fn make_goose_db(dir: &Path) -> PathBuf {
    use rusqlite::Connection;
    let db = dir.join("sessions.db");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(
        r#"
        CREATE TABLE sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '', working_dir TEXT NOT NULL, created_at TIMESTAMP, updated_at TIMESTAMP);
        CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, message_id TEXT, session_id TEXT NOT NULL, role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL);
        INSERT INTO sessions VALUES ('20260408_1', 'Project cleanup', '', '/home/x/src/myproj', '2026-04-08 14:54:20', '2026-04-08 15:00:00');
        INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp)
          VALUES ('m1', '20260408_1', 'user', '[{"type":"text","text":"clean the project"}]', 1775660060);
        INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp)
          VALUES ('m2', '20260408_1', 'assistant', '[{"type":"text","text":"on it"}]', 1775660070);
        "#,
    )
    .unwrap();
    db
}

// ---------- copilot ----------

#[test]
fn copilot_reads_workspace_sessions() {
    let a = aiyou::adapters::copilot::Copilot::new(vec![fixture("vscode/User/workspaceStorage")]);
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = a.read(&found[0].key).unwrap();
    assert_eq!(meta.source, "copilot");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "explain this code");
    assert_eq!(msgs[1].role, "assistant");
}

#[test]
fn copilot_missing_dir_ok() {
    let a = aiyou::adapters::copilot::Copilot::new(vec![PathBuf::from("/nonexistent/vscode")]);
    assert!(a.discover().unwrap().is_empty());
}

// ---------- cursor ----------

#[test]
fn cursor_reads_state_vscdb() {
    let tmp = tempfile::tempdir().unwrap();
    make_cursor_db(tmp.path());
    let a = aiyou::adapters::cursor::Cursor::new(vec![tmp.path().to_path_buf()]);
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = a.read(&found[0].key).unwrap();
    assert_eq!(meta.source, "cursor");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "write a test");
    assert_eq!(msgs[1].role, "assistant");
}

fn make_cursor_db(dir: &Path) -> PathBuf {
    use rusqlite::Connection;
    let ws = dir.join("ws1");
    std::fs::create_dir_all(&ws).unwrap();
    let db = ws.join("state.vscdb");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(
        r#"
        CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
        "#,
    )
    .unwrap();
    let chat = serde_json::json!({
        "messages": [
            {"role": "user", "text": "write a test"},
            {"role": "assistant", "text": "here it is"}
        ]
    });
    c.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params!["composer.composerData", chat.to_string()],
    )
    .unwrap();
    db
}

// ---------- generic (amp, continue, zed) ----------

#[test]
fn generic_scans_messages_files_and_jsonl() {
    // file with messages array (zed style)
    let g = aiyou::adapters::generic::GenericScan::new("zed", vec![fixture("zed")]);
    let found = g.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = g.read(&found[0].key).unwrap();
    assert_eq!(meta.source, "zed");
    assert_eq!(meta.title, "Zed chat");
    assert_eq!(msgs.len(), 2);

    // jsonl where each line is a message (continue style)
    let g = aiyou::adapters::generic::GenericScan::new("continue", vec![fixture("continue")]);
    let found = g.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (_, msgs) = g.read(&found[0].key).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "fix the login bug");
    assert_eq!(msgs[1].role, "assistant");
}

#[test]
fn generic_missing_dir_ok() {
    let g = aiyou::adapters::generic::GenericScan::new("amp", vec![PathBuf::from("/nonexistent/amp")]);
    assert!(g.discover().unwrap().is_empty());
}

// ---------- registry integration ----------

#[test]
fn new_sources_sync_into_store() {
    let (_tmp, st) = fresh_store();
    let mut cfg = aiyou::config::Config::default();
    cfg.sources.gemini_dir = Some(PathBuf::from("/nonexistent")); // disable noisy sources
    cfg.sources.claude_dir = Some(PathBuf::from("/nonexistent"));
    cfg.sources.opencode_db = Some(PathBuf::from("/nonexistent/x.db"));
    cfg.sources.opencode_storage = Some(PathBuf::from("/nonexistent"));
    cfg.sources.codex_dir = Some(PathBuf::from("/nonexistent"));
    cfg.sources.crush_dirs = Some(vec![PathBuf::from("/nonexistent")]);
    cfg.sources.pi_dir = Some(PathBuf::from("/nonexistent"));
    cfg.sources.antigravity_dir = Some(fixture("antigravity-cli"));
    cfg.sources.goose_db = Some(make_goose_db(_tmp.path()));
    cfg.sources.vscode_storage = Some(vec![fixture("vscode/User/workspaceStorage")]);
    cfg.sources.cursor_dirs = Some(vec![PathBuf::from("/nonexistent")]);
    cfg.sources.zed_dirs = Some(vec![fixture("zed")]);
    cfg.sources.continue_dir = Some(fixture("continue"));
    cfg.sources.amp_dirs = Some(vec![PathBuf::from("/nonexistent")]);
    cfg.sources.enabled = Some(vec![
        "antigravity".into(),
        "goose".into(),
        "copilot".into(),
        "zed".into(),
        "continue".into(),
    ]);
    let r = aiyou::sync::run(&st, &cfg, None).unwrap();
    let by_src: std::collections::BTreeMap<String, (usize, usize, usize)> = r
        .per_source
        .into_iter()
        .map(|(s, a, u, e)| (s, (a, u, e)))
        .collect();
    assert_eq!(by_src["antigravity"].0, 1);
    assert_eq!(by_src["goose"].0, 1);
    assert_eq!(by_src["copilot"].0, 1);
    assert_eq!(by_src["zed"].0, 1);
    assert_eq!(by_src["continue"].0, 1);
}

fn make_copilot_vscdb(dir: &Path) -> PathBuf {
    use rusqlite::Connection;
    let ws = dir.join("ws9");
    std::fs::create_dir_all(&ws).unwrap();
    let db = ws.join("state.vscdb");
    let c = Connection::open(&db).unwrap();
    c.execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);").unwrap();
    let session = serde_json::json!({
        "version": 1,
        "messages": [
            {"role": "user", "text": "generate a poem"},
            {"role": "assistant", "text": "roses are red"}
        ]
    });
    c.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        rusqlite::params!["chat.ChatSessionStore.abc123", session.to_string()],
    )
    .unwrap();
    db
}

#[test]
fn copilot_reads_vscdb_chat_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let ws_root = tmp.path().to_path_buf();
    make_copilot_vscdb(&ws_root);
    let a = aiyou::adapters::copilot::Copilot::new(vec![ws_root]);
    let found = a.discover().unwrap();
    assert!(found.iter().any(|d| d.key == "vscdb-ws9"));
    let (meta, msgs) = a.read("vscdb-ws9").unwrap();
    assert_eq!(meta.source, "copilot");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].text, "generate a poem");
}

#[test]
fn sync_skips_empty_sessions() {
    use aiyou::adapters::{Adapter, DiscoveredSession};
    struct EmptyFake;
    impl Adapter for EmptyFake {
        fn name(&self) -> &'static str { "emptyfake" }
        fn discover(&self) -> anyhow::Result<Vec<DiscoveredSession>> {
            Ok(vec![DiscoveredSession { key: "e1".into(), fingerprint: 1, meta_hint: None }])
        }
        fn read(&self, _k: &str) -> anyhow::Result<(aiyou::model::SessionMeta, Vec<aiyou::model::Msg>)> {
            Ok((aiyou::model::SessionMeta {
                source: "emptyfake".into(), id: "e1".into(), project: "p".into(),
                title: "t".into(), created_ms: 1, updated_ms: 1,
            }, vec![]))
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    let cfg = aiyou::config::Config::default();
    let r = aiyou::sync::run_with_adapters(&st, &cfg, vec![Box::new(EmptyFake)]).unwrap();
    assert_eq!(r.per_source, vec![("emptyfake".to_string(), 0, 0, 0)]);
    assert!(st.find_session("emptyfake", "e1").is_none());
    // fingerprint recorded -> second run is a no-op read
    let raw = std::fs::read_to_string(st.fingerprints_path()).unwrap();
    assert!(raw.contains("emptyfake/e1"));
}
