use anyhow::{anyhow, Context, Result};
use std::time::Duration;

/// Minimal OpenAI-compatible chat-completions client with retry.
#[derive(Clone)]
pub struct LlmClient {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl LlmClient {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self { base_url, api_key, model }
    }

    pub fn from_config(cfg: &crate::config::ProfileCfg, model_override: Option<&str>) -> Result<Self> {
        let api_key = std::env::var(&cfg.api_key_env)
            .map_err(|_| anyhow!("environment variable {} is not set (needed for profile)", cfg.api_key_env))?;
        Ok(Self::new(
            cfg.provider_base_url.trim_end_matches('/').to_string(),
            api_key,
            model_override.unwrap_or(&cfg.model).to_string(),
        ))
    }

    /// Blocking wrapper for use from non-async contexts.
    pub fn chat_blocking(&self, system: &str, user: &str) -> Result<String> {
        tokio::runtime::Runtime::new()
            .context("creating tokio runtime")?
            .block_on(self.chat(system, user))
    }

    pub async fn chat(&self, system: &str, user: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "response_format": {"type": "json_object"},
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .context("building http client")?;

        let mut last_err = None;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(match attempt {
                    1 => 1,
                    _ => 4,
                }))
                .await;
            }
            let resp = client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let v: serde_json::Value = r.json().await.context("parsing llm response json")?;
                    let content = v["choices"][0]["message"]["content"]
                        .as_str()
                        .ok_or_else(|| anyhow!("missing choices[0].message.content in response: {v}"))?
                        .to_string();
                    return Ok(content);
                }
                Ok(r) => {
                    let status = r.status();
                    let text = r.text().await.unwrap_or_default();
                    last_err = Some(anyhow!("llm returned {status}: {text}"));
                    // retry only on 5xx / 429
                    if !status.is_server_error() && status.as_u16() != 429 {
                        return Err(last_err.unwrap());
                    }
                }
                Err(e) => {
                    last_err = Some(anyhow!("llm request failed: {e}"));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("llm request failed")))
    }

    /// Salvage a JSON object from model output that may be wrapped in prose.
    pub fn salvage_json(raw: &str) -> Result<serde_json::Value> {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
            if v.is_object() {
                return Ok(v);
            }
        }
        if let (Some(start), Some(end)) = (raw.find('{'), raw.rfind('}')) {
            if start < end {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw[start..=end]) {
                    if v.is_object() {
                        return Ok(v);
                    }
                }
            }
        }
        anyhow::bail!("model output is not a JSON object: {}", &raw[..raw.len().min(200)])
    }
}
