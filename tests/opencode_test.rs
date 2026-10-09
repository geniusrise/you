use std::path::PathBuf;

use aiyou::adapters::Adapter;

fn make_opencode_db(dir: &std::path::Path) -> PathBuf {
    use rusqlite::Connection;
    let db_path = dir.join("opencode.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, directory TEXT NOT NULL, title TEXT NOT NULL, time_created INTEGER, time_updated INTEGER);
        CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER, time_updated INTEGER, data TEXT NOT NULL);
        CREATE TABLE part (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, message_id TEXT NOT NULL, time_created INTEGER, data TEXT NOT NULL);
        INSERT INTO session VALUES ('ses_1', 'prj_1', '/home/x/proj', 'Fix login bug', 1700000000000, 1700000099000);
        INSERT INTO message VALUES ('msg_1', 'ses_1', 1700000000000, 1700000000000, '{"role":"user"}');
        INSERT INTO message VALUES ('msg_2', 'ses_1', 1700000099000, 1700000099000, '{"role":"assistant","model":{"providerID":"openrouter","modelID":"m"}}');
        INSERT INTO part VALUES ('prt_1', 'ses_1', 'msg_1', 1700000000000, '{"type":"text","text":"please fix the login bug"}');
        INSERT INTO part VALUES ('prt_2', 'ses_1', 'msg_2', 1700000099000, '{"type":"text","text":"On it"}');
        INSERT INTO part VALUES ('prt_3', 'ses_1', 'msg_2', 1700000099001, '{"type":"tool","tool":"Bash","state":{"output":"done"}}');
        "#,
    )
    .unwrap();
    db_path
}

#[test]
fn opencode_reads_sqlite() {
    let tmp = tempfile::tempdir().unwrap();
    let db = make_opencode_db(tmp.path());
    let a = aiyou::adapters::opencode::OpenCode::new(db.clone(), tmp.path().join("storage"));
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key, "ses_1");
    let (meta, msgs) = a.read("ses_1").unwrap();
    assert_eq!(meta.id, "ses_1");
    assert_eq!(meta.source, "opencode");
    assert_eq!(meta.title, "Fix login bug");
    assert_eq!(meta.project, "proj");
    // msg_1 user text, msg_2 assistant "On it" + tool Bash
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "please fix the login bug");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "On it");
    assert_eq!(msgs[2].role, "tool");
    assert_eq!(msgs[2].tool.as_deref(), Some("Bash"));
}

#[test]
fn opencode_falls_back_to_storage_json() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = tmp.path().join("storage");
    let ses_dir = storage.join("session");
    let msg_dir = storage.join("message");
    std::fs::create_dir_all(&ses_dir).unwrap();
    std::fs::create_dir_all(&msg_dir).unwrap();
    std::fs::write(
        ses_dir.join("ses_9.json"),
        r#"{"id":"ses_9","projectID":"p","directory":"/home/x/p2","title":"Storage session","time":{"created":1700000000000}}"#,
    )
    .unwrap();
    std::fs::write(
        msg_dir.join("m1.json"),
        r#"{"id":"m1","sessionID":"ses_9","role":"user","time":{"created":1700000000000},"parts":[{"part":true,"type":"text","text":"from storage"}]}"#,
    )
    .unwrap();
    std::fs::write(
        msg_dir.join("m2.json"),
        r#"{"id":"m2","sessionID":"ses_9","role":"assistant","time":{"created":1700000001000},"parts":[{"part":true,"type":"text","text":"ok"}]}"#,
    )
    .unwrap();
    let a = aiyou::adapters::opencode::OpenCode::new(tmp.path().join("nope.db"), storage);
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = a.read("ses_9").unwrap();
    assert_eq!(meta.title, "Storage session");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].text, "from storage");
}

#[test]
fn opencode_missing_everything_is_empty() {
    let a = aiyou::adapters::opencode::OpenCode::new(
        std::path::PathBuf::from("/nonexistent/x.db"),
        std::path::PathBuf::from("/nonexistent/storage"),
    );
    assert!(a.discover().unwrap().is_empty());
}
