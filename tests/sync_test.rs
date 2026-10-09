use aiyou::adapters::{Adapter, DiscoveredSession};
use aiyou::model::{Msg, SessionMeta};

struct Fake {
    v: u64,
    fail_read: bool,
}

impl Adapter for Fake {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredSession>> {
        Ok(vec![DiscoveredSession {
            key: "f1".into(),
            fingerprint: self.v,
            meta_hint: Some(("p".into(), "Fake".into())),
        }])
    }
    fn read(&self, _k: &str) -> anyhow::Result<(SessionMeta, Vec<Msg>)> {
        if self.fail_read {
            anyhow::bail!("boom");
        }
        Ok((
            SessionMeta {
                source: "fake".into(),
                id: "f1".into(),
                project: "p".into(),
                title: "Fake".into(),
                created_ms: 1,
                updated_ms: self.v as i64,
            },
            vec![Msg { id: "m1".into(), role: "user".into(), ts_ms: 1, text: "x".into(), tool: None }],
        ))
    }
}

fn setup() -> (tempfile::TempDir, aiyou::store::Store, aiyou::config::Config) {
    let tmp = tempfile::tempdir().unwrap();
    let st = aiyou::store::Store::open(tmp.path()).unwrap();
    st.init().unwrap();
    let cfg = aiyou::config::Config::default();
    (tmp, st, cfg)
}

fn commits(st: &aiyou::store::Store) -> usize {
    let out = std::process::Command::new("git")
        .current_dir(&st.root)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

#[test]
fn sync_incremental_flow() {
    let (_tmp, st, cfg) = setup();
    // first run: 1 added, 1 commit
    let r = aiyou::sync::run_with_adapters(&st, &cfg, vec![Box::new(Fake { v: 1, fail_read: false })]).unwrap();
    assert_eq!(r.per_source, vec![("fake".to_string(), 1, 0, 0)]);
    let c1 = commits(&st);
    assert!(c1 >= 2); // init commit + sync commit
    // same fingerprint: nothing new, no commit
    let r = aiyou::sync::run_with_adapters(&st, &cfg, vec![Box::new(Fake { v: 1, fail_read: false })]).unwrap();
    assert_eq!(r.per_source, vec![("fake".to_string(), 0, 0, 0)]);
    assert_eq!(commits(&st), c1);
    // changed fingerprint: 1 updated, new commit
    let r = aiyou::sync::run_with_adapters(&st, &cfg, vec![Box::new(Fake { v: 2, fail_read: false })]).unwrap();
    assert_eq!(r.per_source, vec![("fake".to_string(), 0, 1, 0)]);
    assert_eq!(commits(&st), c1 + 1);
    // store content reflects update
    let (meta, _) = st.read_session_jsonl("fake", "f1").unwrap();
    assert_eq!(meta.updated_ms, 2);
}

#[test]
fn sync_read_error_is_counted_not_fatal() {
    let (_tmp, st, cfg) = setup();
    let r = aiyou::sync::run_with_adapters(&st, &cfg, vec![Box::new(Fake { v: 3, fail_read: true })]).unwrap();
    assert_eq!(r.per_source, vec![("fake".to_string(), 0, 0, 1)]);
}

#[test]
fn fingerprints_persist_across_runs() {
    let (_tmp, st, cfg) = setup();
    aiyou::sync::run_with_adapters(&st, &cfg, vec![Box::new(Fake { v: 5, fail_read: false })]).unwrap();
    let raw = std::fs::read_to_string(st.fingerprints_path()).unwrap();
    assert!(raw.contains("\"fake/f1\""), "raw: {raw}");
    assert!(raw.contains("5"));
}
