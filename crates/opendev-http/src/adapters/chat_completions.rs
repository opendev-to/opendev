//! Default Chat Completions adapter for OpenAI-compatible providers.
//!
//! Used as a fallback for providers (Groq, Mistral, Ollama, etc.) that speak
//! the standard Chat Completions format natively but need streaming SSE support.
//! Request/response conversion is passthrough; the adapter adds `stream: true`
//! and parses Chat Completions SSE chunks.

use serde_json::Value;

/// Default adapter for standard Chat Completions-compatible providers.
///
/// Passes requests through with minimal changes and handles
/// Chat Completions SSE streaming format.
#[derive(Debug, Clone)]
pub struct ChatCompletionsAdapter {
    api_url: String,
}

impl ChatCompletionsAdapter {
    pub fn new(api_url: String) -> Self {
        Self { api_url }
    }

    /// Parse a standard Chat Completions SSE chunk into a `StreamEvent`.
    ///
    /// This is a shared helper that can be called by any adapter that uses
    /// the standard `choices[].delta` SSE format (Ollama, Groq, Mistral, etc.).
    pub fn parse_chat_completions_sse(data: &Value) -> Option<crate::streaming::StreamEvent> {
        use crate::streaming::StreamEvent;

        let choices = data.get("choices")?.as_array()?;

        for choice in choices {
            let delta = choice.get("delta")?;

            if let Some(text) = delta.get("content").and_then(|c| c.as_str())
                && !text.is_empty()
            {
                return Some(StreamEvent::TextDelta(text.to_string()));
            }

            if let Some(tc_deltas) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc_delta in tc_deltas {
                    let idx =
                        tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                    if let Some(id) = tc_delta.get("id").and_then(|i| i.as_str()) {
                        let name = tc_delta
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        return Some(StreamEvent::FunctionCallStart {
                            index: idx,
                            call_id: id.to_string(),
                            name,
                        });
                    }

                    if let Some(args) = tc_delta
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str())
                    {
                        return Some(StreamEvent::FunctionCallDelta {
                            index: idx,
                            delta: args.to_string(),
                        });
                    }
                }
            }

            if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str())
                && !reason.is_empty()
                && reason != "null"
            {
                return Some(StreamEvent::UsageUpdate {
                    usage: data.get("usage").cloned(),
                    stop_reason: Some(reason.to_string()),
                });
            }
        }

        None
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for ChatCompletionsAdapter {
    fn provider_name(&self) -> &str {
        "chat_completions"
    }

    fn convert_request(&self, payload: Value) -> Value {
        // Passthrough — strip internal fields only.
        let mut payload = payload;
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("_reasoning_effort");
        }
        payload
    }

    fn convert_response(&self, response: Value) -> Value {
        // Passthrough — already in Chat Completions format.
        response
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn enable_streaming(&self, payload: &mut Value) {
        payload["stream"] = serde_json::json!(true);
    }

    fn parse_stream_event(
        &self,
        _event_type: &str,
        data: &Value,
    ) -> Option<crate::streaming::StreamEvent> {
        Self::parse_chat_completions_sse(data)
    }
}
