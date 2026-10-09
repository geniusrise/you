use std::path::{Path, PathBuf};

use aiyou::adapters::Adapter;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn crush_parses_sessions() {
    let a = aiyou::adapters::crush::Crush::new(vec![fixture("crush")]);
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = a.read(&found[0].key).unwrap();
    assert_eq!(meta.source, "crush");
    assert_eq!(meta.title, "Crush chat");
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "hello crush");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[2].role, "tool");
    assert_eq!(msgs[2].tool.as_deref(), Some("Bash"));
}

#[test]
fn crush_missing_dirs_ok() {
    let a = aiyou::adapters::crush::Crush::new(vec![PathBuf::from("/nonexistent/crush")]);
    assert!(a.discover().unwrap().is_empty());
}

#[test]
fn pi_parses_sessions() {
    let a = aiyou::adapters::pi::Pi::new(fixture("pi"));
    let found = a.discover().unwrap();
    assert_eq!(found.len(), 1);
    let (meta, msgs) = a.read(&found[0].key).unwrap();
    assert_eq!(meta.source, "pi");
    assert_eq!(meta.id, "p1");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "hello pi");
    assert_eq!(msgs[0].ts_ms, 1700000000000);
}

#[test]
fn pi_missing_dir_ok() {
    let a = aiyou::adapters::pi::Pi::new(PathBuf::from("/nonexistent/pi"));
    assert!(a.discover().unwrap().is_empty());
}
