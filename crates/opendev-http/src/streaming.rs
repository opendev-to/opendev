//! SSE streaming support for LLM provider responses.
//!
//! Provides types and parsing for Server-Sent Events (SSE) used by
//! streaming LLM APIs (OpenAI Responses API, Anthropic, etc.).

use serde_json::Value;

/// Events emitted during a streaming LLM response.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A chunk of text content from the assistant.
    TextDelta(String),
    /// A chunk of reasoning/thinking content.
    ReasoningDelta(String),
    /// A new reasoning/thinking block is starting (used to insert separators
    /// between multiple interleaved thinking blocks in a single response).
    ReasoningBlockStart,
    /// A new function/tool call is starting.
    /// Fields: `(index, call_id, function_name)`
    FunctionCallStart {
        index: usize,
        call_id: String,
        name: String,
    },
    /// A chunk of function call arguments.
    FunctionCallDelta { index: usize, delta: String },
    /// Function call arguments are complete.
    FunctionCallDone { index: usize, arguments: String },
    /// Usage/metadata update (input_tokens, output_tokens, stop_reason).
    UsageUpdate {
        usage: Option<Value>,
        stop_reason: Option<String>,
    },
    /// The complete response is available (streaming finished).
    /// Contains the full response body for final processing.
    Done(Value),
    /// An error occurred during streaming.
    Error(String),
}

/// Callback for stream events. Implementations should be cheap/non-blocking.
pub trait StreamCallback: Send + Sync {
    fn on_event(&self, event: &StreamEvent);
}

/// A closure-based StreamCallback.
pub struct FnStreamCallback<F: Fn(&StreamEvent) + Send + Sync>(pub F);

impl<F: Fn(&StreamEvent) + Send + Sync> StreamCallback for FnStreamCallback<F> {
    fn on_event(&self, event: &StreamEvent) {
        (self.0)(event);
    }
}

/// A composite callback that fans out events to multiple callbacks.
///
/// Used to combine the UI emitter callback with the streaming tool executor,
/// so both receive events during a single streaming LLM call.
pub struct CompositeStreamCallback<'a> {
    callbacks: Vec<&'a dyn StreamCallback>,
}

impl<'a> CompositeStreamCallback<'a> {
    /// Create a composite from a list of callbacks.
    pub fn new(callbacks: Vec<&'a dyn StreamCallback>) -> Self {
        Self { callbacks }
    }
}

impl StreamCallback for CompositeStreamCallback<'_> {
    fn on_event(&self, event: &StreamEvent) {
        for cb in &self.callbacks {
            cb.on_event(event);
        }
    }
}

/// Parse a single SSE data line (after "data: " prefix) as JSON.
pub fn parse_sse_data(line: &str) -> Option<Value> {
    let data = line.strip_prefix("data: ")?;
    if data == "[DONE]" {
        return None;
    }
    serde_json::from_str(data).ok()
}

/// Parse an SSE event block (event type + data) from raw lines.
///
/// Returns `(event_type, data_json)` if both are present.
pub fn parse_sse_block(event_line: Option<&str>, data_line: &str) -> Option<(String, Value)> {
    let event_type = event_line
        .and_then(|l| l.strip_prefix("event: "))
        .map(|s| s.to_string())
        .unwrap_or_default();

    let data = parse_sse_data(data_line)?;
    Some((event_type, data))
}
