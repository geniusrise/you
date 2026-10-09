pub mod antigravity;
pub mod claude_code;
pub mod codex;
pub mod copilot;
pub mod crush;
pub mod cursor;
pub mod gemini_cli;
pub mod generic;
pub mod goose;
pub mod opencode;
pub mod pi;
pub mod scan;

pub trait Adapter {
    fn name(&self) -> &'static str;
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredSession>>;
    fn read(&self, key: &str) -> anyhow::Result<(crate::model::SessionMeta, Vec<crate::model::Msg>)>;
}

#[derive(Debug)]
pub struct DiscoveredSession {
    pub key: String,
    pub fingerprint: u64,
    pub meta_hint: Option<(String, String)>,
}
