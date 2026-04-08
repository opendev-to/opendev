//! Shared cheap-LLM memory selector used by both the `SemanticMemoryCollector`
//! (auto-injection) and the `MemoryTool` (semantic search fallback).

use tracing::debug;

/// Cheap models per provider for the selection side-query.
pub const CHEAP_MODELS: &[(&str, &str)] = &[
    ("openai", "gpt-4o-mini"),
    ("anthropic", "claude-3-5-haiku-20241022"),
    (
        "fireworks",
        "accounts/fireworks/models/llama-v3p1-8b-instruct",
    ),
];

/// Env var names per provider.
pub const ENV_KEYS: &[(&str, &str)] = &[
    ("openai", "OPENAI_API_KEY"),
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("fireworks", "FIREWORKS_API_KEY"),
];

/// API endpoint per provider.
pub fn api_endpoint(provider: &str) -> &'static str {
    match provider {
        "fireworks" => "https://api.fireworks.ai/inference/v1/chat/completions",
        _ => "https://api.openai.com/v1/chat/completions",
    }
}

/// Default selection system prompt.
pub const SELECTION_PROMPT: &str = "\
You select memories relevant to the current coding task. \
Given a list of memory files with descriptions, return a JSON array of filenames \
(max 5) that are most relevant to the user's query. Return [] if none are relevant. \
Only return the JSON array, no other text.";

/// Makes a cheap LLM side-query to select relevant memory files.
pub struct MemorySelector {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub client: reqwest::Client,
}

impl MemorySelector {
    /// Try to create a selector by resolving a cheap model and API key.
    pub fn try_new() -> Option<Self> {
        for &(prov, model) in CHEAP_MODELS {
            let env_key = ENV_KEYS
                .iter()
                .find(|&&(p, _)| p == prov)
                .map(|&(_, k)| k)
                .unwrap_or("");
            if let Ok(key) = std::env::var(env_key)
                && !key.is_empty()
            {
                return Some(Self {
                    provider: prov.to_string(),
                    model: model.to_string(),
                    api_key: key,
                    client: reqwest::Client::new(),
                });
            }
        }
        debug!("No API key found for memory selector");
        None
    }

    /// Call the LLM to select relevant memory filenames.
    pub async fn select(
        &self,
        manifest: &str,
        user_query: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        self.select_with_prompt(manifest, user_query, SELECTION_PROMPT)
            .await
    }

    /// Call the LLM with a custom system prompt.
    pub async fn select_with_prompt(
        &self,
        manifest: &str,
        user_query: &str,
        system_prompt: &str,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = api_endpoint(&self.provider);

        let user_content = format!("Memory files:\n{manifest}\n\nCurrent task: {user_query}");

        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_content},
            ],
            "max_tokens": 200,
            "temperature": 0.0,
        });

        let resp = self
            .client
            .post(endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Memory selector API returned {}", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let content = body
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or("No content in memory selector response")?;

        let filenames: Vec<String> = serde_json::from_str(content.trim())?;
        Ok(filenames)
    }
}
