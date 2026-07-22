//! ChatGPT Codex Responses adapter.
//!
//! Payload conversion starts from the Platform Responses adapter, then removes
//! fields the ChatGPT Codex backend rejects. Authentication and the trusted
//! endpoint stay outside the adapter.

use serde_json::Value;

use super::base::ProviderAdapter;
use super::openai::OpenAiAdapter;
use crate::chatgpt_auth::protocol::CHATGPT_CODEX_RESPONSES_URL;
use crate::streaming::StreamEvent;

/// Converts OpenDev's internal Chat Completions payload to the stateless
/// ChatGPT Codex Responses contract.
#[derive(Debug, Clone, Default)]
pub struct OpenAiChatGptAdapter {
    openai: OpenAiAdapter,
}

impl OpenAiChatGptAdapter {
    pub fn new() -> Self {
        Self {
            openai: OpenAiAdapter::with_url(CHATGPT_CODEX_RESPONSES_URL),
        }
    }
}

#[async_trait::async_trait]
impl ProviderAdapter for OpenAiChatGptAdapter {
    fn provider_name(&self) -> &str {
        "openai-chatgpt"
    }

    fn convert_request(&self, payload: Value) -> Value {
        // OpenAiAdapter produces `store: false` and preserves function-call
        // history in the Responses input form, both required for stateless use.
        let mut converted = self.openai.convert_request(payload);

        let Some(body) = converted.as_object_mut() else {
            return converted;
        };

        // This backend does not accept the generic Platform output-token
        // parameter. OpenCode removes it before sending its direct Codex
        // request; doing the same avoids a 400 for dynamically discovered
        // models such as gpt-5.4.
        body.remove("max_output_tokens");

        // Preserve encrypted reasoning content across stateless turns and use
        // the Codex-compatible summary mode rather than the Platform adapter's
        // detailed-only default.
        let include = body
            .entry("include".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(include) = include.as_array_mut()
            && !include
                .iter()
                .any(|item| item.as_str() == Some("reasoning.encrypted_content"))
        {
            include.push(Value::String("reasoning.encrypted_content".to_string()));
        }
        if let Some(reasoning) = body.get_mut("reasoning").and_then(Value::as_object_mut) {
            reasoning.insert("summary".to_string(), Value::String("auto".to_string()));
        }

        converted
    }

    fn convert_response(&self, response: Value) -> Value {
        self.openai.convert_response(response)
    }

    fn api_url(&self) -> &str {
        CHATGPT_CODEX_RESPONSES_URL
    }

    fn supports_streaming(&self) -> bool {
        self.openai.supports_streaming()
    }

    fn enable_streaming(&self, payload: &mut Value) {
        self.openai.enable_streaming(payload);
    }

    fn parse_stream_event(&self, event_type: &str, data: &Value) -> Option<StreamEvent> {
        self.openai.parse_stream_event(event_type, data)
    }
}

#[cfg(test)]
#[path = "openai_chatgpt_tests.rs"]
mod tests;
