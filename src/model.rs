use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub source: String,
    pub id: String,
    pub project: String,
    pub title: String,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    pub id: String,
    pub role: String,
    pub ts_ms: i64,
    pub text: String,
    pub tool: Option<String>,
}

pub fn stable_msg_id(role: &str, text: &str, ts: i64) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(role.as_bytes());
    h.update(b"|");
    h.update(text.as_bytes());
    h.update(b"|");
    h.update(ts.to_string().as_bytes());
    hex::encode(h.finalize())
}
