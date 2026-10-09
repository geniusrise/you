use aiyou::config::Config;

#[test]
fn init_creates_store_and_config() {
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    assert!(st.config_path().is_file());
    let cfg = Config::load(&st.config_path()).unwrap();
    assert_eq!(cfg.profile.workers, 4);
    assert_eq!(cfg.sources.enabled, None);
    assert_eq!(cfg.profile.model, "gpt-5.2");
    assert!(st.chats_dir().is_dir());
    assert!(st.profile_dir().is_dir());
    assert!(st.imports_dir().is_dir());
    assert!(!aiyou::git::git_commit_all(&st.root, "empty").unwrap());
    std::fs::write(st.root.join("x.txt"), "hi").unwrap();
    assert!(aiyou::git::git_commit_all(&st.root, "add x").unwrap());
}

#[test]
fn config_roundtrip_preserves_source_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.sources.claude_dir = Some(tmp.path().join("projects"));
    let p = tmp.path().join("aiyou.toml");
    cfg.save(&p).unwrap();
    let loaded = Config::load(&p).unwrap();
    assert_eq!(loaded.sources.claude_dir, cfg.sources.claude_dir);
    assert!(loaded.sources.enabled.is_none()); // absent = all sources enabled
}

#[test]
fn default_root_honors_env() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("AIYOU_HOME", tmp.path());
    let root = aiyou::store::default_root();
    std::env::remove_var("AIYOU_HOME");
    assert_eq!(root, tmp.path());
}
