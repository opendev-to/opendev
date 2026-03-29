//! LLM-based topic detection for dynamic session titling.
//!
//! On each user message, fires a lightweight LLM call in a background task
//! to detect whether the conversation topic has changed. If it has, updates
//! the session title via `SessionManager::set_title()`.
//!
//! Graceful degradation: no API key -> no-op. LLM failure -> keep existing title.
//! Never panics.

use std::env;
use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::debug;

use crate::session_manager::SessionManager;

/// Maximum number of recent messages to send for topic detection.
const MAX_RECENT_MESSAGES: usize = 4;

/// Maximum title length in characters.
const MAX_TITLE_LEN: usize = 50;

/// Cheap models per provider — small, fast, inexpensive.
const CHEAP_MODELS: &[(&str, &str)] = &[
    ("openai", "gpt-4o-mini"),
    ("anthropic", "claude-3-5-haiku-20241022"),
    (
        "fireworks",
        "accounts/fireworks/models/llama-v3p1-8b-instruct",
    ),
];

/// Env var names per provider.
const ENV_KEYS: &[(&str, &str)] = &[
    ("openai", "OPENAI_API_KEY"),
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("fireworks", "FIREWORKS_API_KEY"),
];

/// System prompt for topic detection.
const TOPIC_DETECTION_PROMPT: &str = "\
You are a conversation topic analyzer. Determine whether the user's latest message \
introduces or shifts to a new conversation topic.

Respond with a JSON object containing exactly two fields:
- \"isNewTopic\": boolean - true if the topic is new or has meaningfully changed, \
or if there is no current title yet
- \"title\": string or null - a concise 2-4 word lowercase title starting with an action verb \
if isNewTopic is true, null otherwise

If no current title exists, you MUST set isNewTopic to true and provide a title.

Title rules: always start with a verb. Max 4 words.
Title examples: \"refactor auth middleware\", \"debug login flow\", \"add search feature\", \"fix test failures\".

Output only the JSON object, no other text.";

/// API endpoint patterns per provider.
fn api_endpoint(provider: &str) -> &'static str {
    match provider {
        "openai" => "https://api.openai.com/v1/chat/completions",
        "anthropic" => "https://api.openai.com/v1/chat/completions", // uses adapter
        "fireworks" => "https://api.fireworks.ai/inference/v1/chat/completions",
        _ => "https://api.openai.com/v1/chat/completions",
    }
}

/// A simple message with role and content for topic detection.
#[derive(Debug, Clone)]
pub struct SimpleMessage {
    pub role: String,
    pub content: String,
}

/// Parsed LLM response for topic detection.
#[derive(Debug, Deserialize)]
struct TopicResult {
    #[serde(rename = "isNewTopic")]
    is_new_topic: bool,
    title: Option<String>,
}

/// Fire-and-forget LLM-based topic detector for session titles.
///
/// Usage:
/// ```ignore
/// let detector = TopicDetector::new("openai");
/// detector.detect(session_manager, session_id, &messages);
/// // Or use the inline async version:
/// if let Some(title) = detector.detect_title(&messages).await {
///     session_manager.set_title(&session_id, &title).ok();
/// }
/// ```
///
/// The `detect()` call is non-blocking — it spawns a tokio task.
/// The `detect_title()` call is async and returns the title directly.
#[derive(Clone)]
pub struct TopicDetector {
    enabled: bool,
    provider: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl TopicDetector {
    /// Create a new topic detector.
    ///
    /// Resolves a cheap model and API key. If no key is available, the
    /// detector is created in disabled mode (all calls are no-ops).
    pub fn new(preferred_provider: &str) -> Self {
        let resolved = resolve_cheap_model(preferred_provider);

        match resolved {
            Some((provider, model, api_key)) => Self {
                enabled: true,
                provider: provider.to_string(),
                model: model.to_string(),
                api_key,
                client: reqwest::Client::new(),
            },
            None => Self {
                enabled: false,
                provider: String::new(),
                model: String::new(),
                api_key: String::new(),
                client: reqwest::Client::new(),
            },
        }
    }

    /// Check if topic detection is enabled (has a valid API key).
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Detect the topic inline and return the title if a new topic is found.
    ///
    /// This is the async version — call it when you want the title directly
    /// without needing `Arc<Mutex<SessionManager>>`.
    pub async fn detect_title(
        &self,
        messages: &[SimpleMessage],
        current_title: Option<&str>,
    ) -> Option<String> {
        if !self.enabled {
            return None;
        }

        let recent: Vec<SimpleMessage> = if messages.len() > MAX_RECENT_MESSAGES {
            messages[messages.len() - MAX_RECENT_MESSAGES..].to_vec()
        } else {
            messages.to_vec()
        };

        if recent.is_empty() {
            return None;
        }

        match call_llm(
            &self.client,
            &self.provider,
            &self.model,
            &self.api_key,
            &recent,
            current_title,
        )
        .await
        {
            Ok(result) if result.is_new_topic => result
                .title
                .map(|t| {
                    let trimmed = t.trim().to_string();
                    if trimmed.len() > MAX_TITLE_LEN {
                        trimmed[..MAX_TITLE_LEN].to_string()
                    } else {
                        trimmed
                    }
                })
                .filter(|t| !t.is_empty()),
            Ok(_) => None,
            Err(e) => {
                debug!("Topic detection failed: {e}");
                None
            }
        }
    }

    /// Trigger topic detection in a background task.
    ///
    /// Non-blocking — spawns a tokio task that calls the LLM and updates
    /// the session title if a new topic is detected.
    pub fn detect(
        &self,
        session_manager: Arc<Mutex<SessionManager>>,
        session_id: String,
        messages: &[SimpleMessage],
    ) {
        if !self.enabled {
            return;
        }

        let recent: Vec<SimpleMessage> = if messages.len() > MAX_RECENT_MESSAGES {
            messages[messages.len() - MAX_RECENT_MESSAGES..].to_vec()
        } else {
            messages.to_vec()
        };

        if recent.is_empty() {
            return;
        }

        let provider = self.provider.clone();
        let model = self.model.clone();
        let api_key = self.api_key.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            if let Err(e) = detect_and_update(
                &client,
                &provider,
                &model,
                &api_key,
                session_manager,
                &session_id,
                &recent,
            )
            .await
            {
                debug!("Topic detection failed: {e}");
            }
        });
    }
}

/// Internal: make LLM call and update title if topic changed.
async fn detect_and_update(
    client: &reqwest::Client,
    provider: &str,
    model: &str,
    api_key: &str,
    session_manager: Arc<Mutex<SessionManager>>,
    session_id: &str,
    recent_messages: &[SimpleMessage],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let current_title = {
        let mgr = session_manager.lock().await;
        mgr.current_session()
            .and_then(|s| s.metadata.get("title"))
            .and_then(|v| v.as_str())
            .map(|t| t.to_string())
    };
    let result = call_llm(
        client,
        provider,
        model,
        api_key,
        recent_messages,
        current_title.as_deref(),
    )
    .await?;

    if result.is_new_topic
        && let Some(title) = result.title
    {
        let title = title.trim();
        if !title.is_empty() {
            let title = if title.len() > MAX_TITLE_LEN {
                &title[..MAX_TITLE_LEN]
            } else {
                title
            };

            let mut mgr = session_manager.lock().await;
            mgr.set_title(session_id, title)?;
            debug!(session_id, title, "Topic detector updated session title");
        }
    }

    Ok(())
}

/// Internal: call LLM and parse JSON response.
async fn call_llm(
    client: &reqwest::Client,
    provider: &str,
    model: &str,
    api_key: &str,
    recent_messages: &[SimpleMessage],
    current_title: Option<&str>,
) -> Result<TopicResult, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = api_endpoint(provider);

    // Build messages array
    let mut api_messages = vec![serde_json::json!({
        "role": "system",
        "content": TOPIC_DETECTION_PROMPT,
    })];

    for msg in recent_messages {
        api_messages.push(serde_json::json!({
            "role": msg.role,
            "content": msg.content,
        }));
    }

    let analysis_prompt = if let Some(title) = current_title {
        format!(
            "The current topic title is \"{title}\". \
             Analyze the conversation above. Has the topic meaningfully changed?"
        )
    } else {
        "There is no current title. Analyze the conversation above and provide a title.".to_string()
    };

    api_messages.push(serde_json::json!({
        "role": "user",
        "content": analysis_prompt,
    }));

    let payload = serde_json::json!({
        "model": model,
        "messages": api_messages,
        "max_tokens": 100,
        "temperature": 0.0,
    });

    let resp = client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!("LLM API returned {}", resp.status()).into());
    }

    let body: serde_json::Value = resp.json().await?;
    let content = body
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or("No content in LLM response")?;

    let result: TopicResult = serde_json::from_str(content)?;
    Ok(result)
}

/// Resolve a cheap model and API key for topic detection.
///
/// Tries the preferred provider first, then falls back to any provider
/// with an API key set.
fn resolve_cheap_model(preferred: &str) -> Option<(&'static str, &'static str, String)> {
    // Try preferred provider first
    for &(prov, model) in CHEAP_MODELS {
        if prov == preferred
            && let Some(key) = get_api_key(prov)
        {
            return Some((prov, model, key));
        }
    }

    // Fallback: try each provider
    for &(prov, model) in CHEAP_MODELS {
        if let Some(key) = get_api_key(prov) {
            return Some((prov, model, key));
        }
    }

    None
}

/// Get the API key for a provider from environment variables.
fn get_api_key(provider: &str) -> Option<String> {
    for &(prov, env_key) in ENV_KEYS {
        if prov == provider {
            return env::var(env_key).ok().filter(|k| !k.is_empty());
        }
    }
    None
}

#[cfg(test)]
#[path = "topic_detector_tests.rs"]
mod tests;
