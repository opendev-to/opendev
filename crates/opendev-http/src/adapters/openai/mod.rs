//! OpenAI Responses API adapter.
//!
//! The Responses API (`/v1/responses`) is OpenAI's recommended replacement for
//! Chat Completions.  This adapter transparently converts the internal
//! Chat Completions-shaped payload to the Responses API format and converts
//! responses back so the rest of the agent code is unaffected.
//!
//! See: https://platform.openai.com/docs/guides/migrate-to-responses

mod request;
mod response;

use serde_json::{Value, json};

const DEFAULT_API_URL: &str = "https://api.openai.com/v1/responses";

/// Set of model prefixes that are reasoning models (o1, o3).
const REASONING_PREFIXES: &[&str] = &["o1", "o3"];

/// Adapter for the OpenAI Responses API.
///
/// Converts internal Chat Completions payloads to the Responses API format
/// and converts responses back to Chat Completions format.
#[derive(Debug, Clone)]
pub struct OpenAiAdapter {
    api_url: String,
}

impl OpenAiAdapter {
    /// Create a new OpenAI adapter with the default Responses API URL.
    pub fn new() -> Self {
        Self {
            api_url: DEFAULT_API_URL.to_string(),
        }
    }

    /// Create with a custom API URL (for Azure, proxies, etc.).
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            api_url: url.into(),
        }
    }
}

impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for OpenAiAdapter {
    fn provider_name(&self) -> &str {
        "openai"
    }

    fn convert_request(&self, payload: Value) -> Value {
        let mut payload = payload;

        // Extract and remove internal reasoning effort field
        let reasoning_effort = payload
            .as_object_mut()
            .and_then(|obj| obj.remove("_reasoning_effort"))
            .and_then(|v| v.as_str().map(String::from));

        let messages = payload
            .get("messages")
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        let (instructions, input_items) = Self::convert_messages(&messages);

        let mut responses_payload = json!({
            "model": payload.get("model").cloned().unwrap_or(json!("")),
            "input": input_items,
            "store": false,
        });

        if let Some(instr) = instructions {
            responses_payload["instructions"] = Value::String(instr);
        }

        // max_tokens / max_completion_tokens → max_output_tokens
        let max_tok = payload
            .get("max_completion_tokens")
            .or_else(|| payload.get("max_tokens"));
        if let Some(tok) = max_tok {
            responses_payload["max_output_tokens"] = tok.clone();
        }

        // Reasoning config — always request when effort is configured.
        // Works for o-series, GPT-5+, and any future reasoning-capable models.
        // Non-reasoning models will simply not return reasoning output.
        if let Some(ref effort) = reasoning_effort {
            responses_payload["reasoning"] = json!({
                "effort": effort,
                "summary": "detailed",
            });
            // Include encrypted reasoning content for full thinking traces.
            // Without this, OpenAI only returns brief summaries.
            responses_payload["include"] = json!(["reasoning.encrypted_content"]);
            // OpenAI rejects temperature when reasoning is set
        } else if !Self::is_reasoning_model(&payload) {
            // Temperature (only when reasoning is NOT active)
            if let Some(temp) = payload.get("temperature") {
                responses_payload["temperature"] = temp.clone();
            }
        }

        // Tools
        if let Some(tools) = payload.get("tools").and_then(|t| t.as_array()) {
            responses_payload["tools"] = Value::Array(Self::convert_tools(tools));
        }

        responses_payload
    }

    fn convert_response(&self, response: Value) -> Value {
        Self::build_chat_completion(&response)
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn enable_streaming(&self, payload: &mut Value) {
        payload["stream"] = json!(true);
    }

    fn parse_stream_event(
        &self,
        event_type: &str,
        data: &Value,
    ) -> Option<crate::streaming::StreamEvent> {
        use crate::streaming::StreamEvent;
        match event_type {
            "response.output_text.delta" => {
                let delta = data.get("delta")?.as_str()?;
                Some(StreamEvent::TextDelta(delta.to_string()))
            }
            "response.reasoning_summary_part.added" => Some(StreamEvent::ReasoningBlockStart),
            "response.reasoning_summary_text.delta" => {
                let delta = data.get("delta")?.as_str()?;
                Some(StreamEvent::ReasoningDelta(delta.to_string()))
            }
            // ── Function call streaming ──
            "response.output_item.added" => {
                let item = data.get("item")?;
                if item.get("type").and_then(|t| t.as_str()) != Some("function_call") {
                    return None;
                }
                let index = data
                    .get("output_index")
                    .and_then(|i| i.as_u64())
                    .unwrap_or(0) as usize;
                let call_id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(StreamEvent::FunctionCallStart {
                    index,
                    call_id,
                    name,
                    initial_args: None,
                })
            }
            "response.function_call_arguments.delta" => {
                let index = data
                    .get("output_index")
                    .and_then(|i| i.as_u64())
                    .unwrap_or(0) as usize;
                let delta = data.get("delta")?.as_str()?.to_string();
                Some(StreamEvent::FunctionCallDelta { index, delta })
            }
            "response.function_call_arguments.done" => {
                let index = data
                    .get("output_index")
                    .and_then(|i| i.as_u64())
                    .unwrap_or(0) as usize;
                let arguments = data
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}")
                    .to_string();
                Some(StreamEvent::FunctionCallDone { index, arguments })
            }
            // ── Stream lifecycle ──
            "response.completed" | "response.incomplete" => {
                let response = data
                    .get("response")
                    .cloned()
                    .unwrap_or_else(|| data.clone());
                Some(StreamEvent::Done(response))
            }
            "error" => {
                let msg = data
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown streaming error");
                Some(StreamEvent::Error(msg.to_string()))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
