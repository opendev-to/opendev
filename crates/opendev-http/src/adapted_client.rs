//! Adapted HTTP client that wraps HttpClient + ProviderAdapter.
//!
//! Transparently converts requests/responses through the provider adapter
//! so the rest of the codebase can use a uniform Chat Completions format.

use crate::adapters::base::ProviderAdapter;
use crate::adapters::detect_provider_from_key;
use crate::client::HttpClient;
use crate::models::{HttpError, HttpResult};
use crate::streaming::{StreamCallback, StreamEvent};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// HTTP client with provider-specific request/response adaptation.
///
/// Wraps `HttpClient` and an optional `ProviderAdapter`. When an adapter
/// is present, `post_json` will:
/// 1. Convert the payload via `adapter.convert_request()`
/// 2. Send via `HttpClient::post_json()`
/// 3. Convert the response body via `adapter.convert_response()`
pub struct AdaptedClient {
    client: HttpClient,
    adapter: Option<Box<dyn ProviderAdapter>>,
    /// When set, all requests use curl subprocess instead of reqwest.
    /// Required for endpoints like DashScope that reject reqwest with HTTP 405.
    curl_auth_header: Option<String>,
}

impl AdaptedClient {
    /// Create an adapted client without any adapter (passthrough).
    pub fn new(client: HttpClient) -> Self {
        Self {
            client,
            adapter: None,
            curl_auth_header: None,
        }
    }

    /// Create an adapted client with a provider adapter.
    pub fn with_adapter(client: HttpClient, adapter: Box<dyn ProviderAdapter>) -> Self {
        Self {
            client,
            adapter: Some(adapter),
            curl_auth_header: None,
        }
    }

    /// Enable curl subprocess transport for all requests.
    ///
    /// Required for endpoints like `coding-intl.dashscope.aliyuncs.com` that
    /// return HTTP 405 when accessed via reqwest but work correctly via curl.
    /// The `auth_header` should be the full Authorization header value,
    /// e.g. `"Authorization: Bearer sk-..."`.
    pub fn with_curl_transport(mut self, auth_header: String) -> Self {
        self.curl_auth_header = Some(auth_header);
        self
    }

    /// Whether this client uses curl subprocess transport instead of reqwest.
    pub fn uses_curl_transport(&self) -> bool {
        self.curl_auth_header.is_some()
    }

    /// Create an adapter for a specific provider name.
    ///
    /// Recognized providers:
    /// - `"anthropic"` → [`AnthropicAdapter`](crate::adapters::anthropic::AnthropicAdapter)
    /// - `"openai"` → [`OpenAiAdapter`](crate::adapters::openai::OpenAiAdapter)
    /// - `"gemini"` | `"google"` → [`GeminiAdapter`](crate::adapters::gemini::GeminiAdapter)
    ///
    /// Returns `None` for providers that use the Chat Completions format natively
    /// (groq, fireworks, mistral, etc.).
    pub fn adapter_for_provider(provider: &str) -> Option<Box<dyn ProviderAdapter>> {
        match provider {
            "anthropic" => Some(Box::new(crate::adapters::anthropic::AnthropicAdapter::new())),
            "openai" => Some(Box::new(crate::adapters::openai::OpenAiAdapter::new())),
            "gemini" | "google" => {
                Some(Box::new(crate::adapters::gemini::GeminiAdapter::default()))
            }
            _ => None,
        }
    }

    /// Resolve the provider name, falling back to auto-detection from the API key.
    ///
    /// If `provider` is non-empty, returns it as-is. Otherwise, inspects the
    /// API key prefix via [`detect_provider_from_key`] and returns the detected
    /// provider or `"openai"` as the final fallback.
    pub fn resolve_provider(provider: &str, api_key: &str) -> String {
        if !provider.is_empty() {
            return provider.to_string();
        }
        detect_provider_from_key(api_key)
            .unwrap_or("openai")
            .to_string()
    }

    /// POST JSON with optional request/response conversion.
    pub async fn post_json(
        &self,
        payload: &serde_json::Value,
        cancel: Option<&CancellationToken>,
    ) -> Result<HttpResult, HttpError> {
        // Build the effective payload (strip/convert as needed).
        let effective_owned;
        let effective_payload = match &self.adapter {
            Some(adapter) => {
                effective_owned = adapter.convert_request(payload.clone());
                &effective_owned
            }
            None => {
                if payload.get("_reasoning_effort").is_some() {
                    let mut cleaned = payload.clone();
                    cleaned.as_object_mut().unwrap().remove("_reasoning_effort");
                    effective_owned = cleaned;
                    &effective_owned
                } else {
                    payload
                }
            }
        };

        // Use curl subprocess for endpoints that reject reqwest (e.g. DashScope).
        let mut result = if let Some(ref auth) = self.curl_auth_header {
            self.post_json_curl(effective_payload, auth).await?
        } else {
            self.client.post_json(effective_payload, cancel).await?
        };

        // Convert response body back to Chat Completions format
        if let (Some(adapter), Some(body)) = (&self.adapter, &result.body)
            && result.success
        {
            result.body = Some(adapter.convert_response(body.clone()));
        }

        Ok(result)
    }

    /// Execute a single non-streaming POST via curl subprocess.
    async fn post_json_curl(
        &self,
        payload: &serde_json::Value,
        auth_header: &str,
    ) -> Result<HttpResult, HttpError> {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;
        use uuid::Uuid;

        let request_id = Uuid::new_v4().to_string();
        let url = self.client.api_url();
        debug!(request_id = %request_id, url = %url, "Sending LLM request via curl");

        let body = serde_json::to_vec(payload)
            .map_err(|e| HttpError::Other(format!("serialize error: {e}")))?;

        let mut child = tokio::process::Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--http1.1",
                "--location",
                "--request",
                "POST",
                url,
                "--header",
                auth_header,
                "--header",
                "Content-Type: application/json",
                "--data-binary",
                "@-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| HttpError::Other(format!("Failed to spawn curl: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&body)
                .await
                .map_err(|e| HttpError::Other(format!("curl stdin write error: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| HttpError::Other(format!("curl wait error: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let msg = if stderr.is_empty() {
                format!("curl exited {:?}", output.status.code())
            } else {
                stderr
            };
            warn!(request_id = %request_id, error = %msg, "curl request failed");
            return Ok(HttpResult::fail(
                format!("[request_id={request_id}] {msg}"),
                false,
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(body_val) => {
                if let Some(err_obj) = body_val.get("error") {
                    let msg = err_obj
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("API error")
                        .to_string();
                    warn!(request_id = %request_id, error = %msg, "curl API error");
                    return Ok(HttpResult {
                        success: false,
                        status: None,
                        body: Some(body_val),
                        error: Some(format!("[request_id={request_id}] {msg}")),
                        interrupted: false,
                        retryable: false,
                        request_id: Some(request_id),
                        retry_after: None,
                        retry_after_ms: None,
                    });
                }
                Ok(HttpResult::ok(200, body_val).with_request_id(request_id))
            }
            Err(e) => {
                let msg = format!("parse error: {e}");
                warn!(request_id = %request_id, error = %msg, "curl response parse error");
                Ok(HttpResult::fail(
                    format!("[request_id={request_id}] {msg}"),
                    false,
                ))
            }
        }
    }

    /// Whether streaming is supported for this client's adapter.
    pub fn supports_streaming(&self) -> bool {
        // curl transport always supports Chat Completions SSE streaming.
        if self.curl_auth_header.is_some() {
            return true;
        }
        self.adapter
            .as_ref()
            .map(|a| a.supports_streaming())
            .unwrap_or(false)
    }

    /// POST JSON with SSE streaming, calling the callback for each event.
    ///
    /// Falls back to `post_json` if the adapter doesn't support streaming.
    /// Returns the final accumulated response as an `HttpResult`.
    pub async fn post_json_streaming(
        &self,
        payload: &serde_json::Value,
        cancel: Option<&CancellationToken>,
        callback: &dyn StreamCallback,
    ) -> Result<HttpResult, HttpError> {
        // For curl-transport endpoints, bypass the adapter gate and stream via curl.
        if let Some(ref auth) = self.curl_auth_header {
            let converted = if let Some(adapter) = &self.adapter {
                let mut c = adapter.convert_request(payload.clone());
                adapter.enable_streaming(&mut c);
                c
            } else {
                let mut c = payload.clone();
                if let Some(obj) = c.as_object_mut() {
                    obj.remove("_reasoning_effort");
                }
                c["stream"] = serde_json::json!(true);
                c
            };
            return self
                .post_json_streaming_curl(&converted, auth, callback, cancel)
                .await;
        }

        let adapter = match &self.adapter {
            Some(a) if a.supports_streaming() => a,
            _ => {
                return self.post_json(payload, cancel).await;
            }
        };

        // Convert request and add streaming flag
        let mut converted = adapter.convert_request(payload.clone());
        adapter.enable_streaming(&mut converted);

        // Use streaming URL if the adapter provides one, otherwise fall back to client URL
        let base_url = self.client.api_url();
        let streaming_url_owned = adapter.streaming_url(base_url);
        let url = streaming_url_owned.as_deref().unwrap_or(base_url);

        // Send request and get raw response for streaming.
        // On failure (after internal retries are exhausted), soft-fail to an
        // HttpResult so the react loop can retry on the next iteration, matching
        // the non-streaming post_json behavior.
        debug!(url = %url, "Sending streaming request");
        let response = match self
            .client
            .send_streaming_request(url, &converted, cancel)
            .await
        {
            Ok(resp) => resp,
            Err(HttpError::Interrupted) => return Ok(HttpResult::interrupted()),
            Err(e) => {
                warn!(error = %e, "Streaming request failed after retries, soft-failing");
                return Ok(HttpResult::fail(e.to_string(), true));
            }
        };

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        debug!(content_type = %content_type, status = %response.status(), "Streaming response headers received");
        // If the response isn't SSE, fall back to reading as JSON
        if !content_type.contains("text/event-stream") {
            warn!(content_type = %content_type, "Streaming fallback: response is not SSE, reading as JSON");
            let body = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| HttpError::Other(format!("Failed to parse response: {e}")))?;

            // Check for API error
            if let Some(error_obj) = body.get("error") {
                let msg = error_obj
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown API error");
                return Err(HttpError::Other(format!("API error: {msg}")));
            }

            let converted_body = adapter.convert_response(body);
            return Ok(HttpResult::ok(200, converted_body));
        }

        // Read SSE events from the response body
        let mut final_body: Option<serde_json::Value> = None;
        let mut accumulated_text = String::new();
        let mut accumulated_reasoning = String::new();
        let mut usage_data: Option<serde_json::Value> = None;
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut current_tool_args: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        // OpenAI Responses API: map output_index → tool_call vec index
        let mut tool_call_index: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut stop_reason: Option<String> = None;
        let mut line_buf = String::new();
        let mut event_type: Option<String> = None;

        use futures::StreamExt;
        let mut byte_stream = response.bytes_stream();

        // Buffer for incomplete UTF-8 or line fragments
        let mut buf = Vec::new();

        let mut stream_done = false;
        let mut stream_end_reason: Option<&str> = None;
        let stream_start = std::time::Instant::now();
        // Maximum total stream duration (5 minutes). Prevents indefinite hangs
        // when the API sends heartbeat events but never completes.
        const MAX_STREAM_DURATION: std::time::Duration = std::time::Duration::from_secs(300);

        loop {
            // Check total stream duration
            if stream_start.elapsed() > MAX_STREAM_DURATION {
                warn!(
                    elapsed_secs = stream_start.elapsed().as_secs(),
                    "SSE stream total duration exceeded 300s, forcing termination"
                );
                stream_end_reason = Some("stream duration exceeded 5 minutes");
                break;
            }

            let chunk_result =
                match tokio::time::timeout(std::time::Duration::from_secs(120), byte_stream.next())
                    .await
                {
                    Ok(Some(result)) => result,
                    Ok(None) => {
                        stream_end_reason = Some("connection closed by server");
                        break;
                    }
                    Err(_elapsed) => {
                        warn!("SSE stream idle timeout (120s with no data)");
                        stream_end_reason = Some("idle timeout (120s with no data)");
                        break;
                    }
                };

            // Check cancellation
            if let Some(token) = cancel
                && token.is_cancelled()
            {
                return Ok(HttpResult::interrupted());
            }

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "SSE stream error");
                    callback.on_event(&StreamEvent::Error(e.to_string()));
                    stream_end_reason = Some("network error during stream");
                    break;
                }
            };

            buf.extend_from_slice(&chunk);

            // Process complete lines from the buffer
            while let Some(newline_pos) = buf.iter().position(|&b| b == b'\n') {
                let line_bytes = buf.drain(..=newline_pos).collect::<Vec<u8>>();
                let line = String::from_utf8_lossy(&line_bytes).trim().to_string();

                if line.is_empty() {
                    // Empty line = end of SSE event block
                    if !line_buf.is_empty() && line_buf.trim() == "data: [DONE]" {
                        stream_done = true;
                        line_buf.clear();
                        event_type = None;
                        continue;
                    }
                    if !line_buf.is_empty()
                        && let Some(data_json) = crate::streaming::parse_sse_data(&line_buf)
                    {
                        // Get event type from SSE `event:` line or from JSON `type` field.
                        // OpenAI Responses API sends only `data:` lines with a `type` field
                        // in the JSON payload (no `event:` lines).
                        let et = event_type.as_deref().unwrap_or_else(|| {
                            data_json.get("type").and_then(|t| t.as_str()).unwrap_or("")
                        });
                        if let Some(stream_event) = adapter.parse_stream_event(et, &data_json) {
                            debug!(event_type = %et, "Stream event received");
                            match &stream_event {
                                StreamEvent::Done(body) => {
                                    final_body = Some(body.clone());
                                    stream_done = true;
                                }
                                StreamEvent::TextDelta(text) => {
                                    accumulated_text.push_str(text);
                                }
                                StreamEvent::ReasoningBlockStart => {
                                    if !accumulated_reasoning.is_empty() {
                                        accumulated_reasoning.push_str("\n\n");
                                    }
                                }
                                StreamEvent::ReasoningDelta(text) => {
                                    accumulated_reasoning.push_str(text);
                                }
                                StreamEvent::FunctionCallStart {
                                    index,
                                    call_id,
                                    name,
                                } => {
                                    let tc_idx = tool_calls.len();
                                    tool_calls.push(serde_json::json!({
                                        "id": call_id,
                                        "type": "function",
                                        "function": {
                                            "name": name,
                                            "arguments": "",
                                        }
                                    }));
                                    tool_call_index.insert(*index, tc_idx);
                                    current_tool_args.insert(tc_idx, String::new());
                                }
                                StreamEvent::FunctionCallDelta { index, delta } => {
                                    if let Some(&tc_idx) = tool_call_index.get(index) {
                                        current_tool_args
                                            .entry(tc_idx)
                                            .or_default()
                                            .push_str(delta);
                                    }
                                }
                                StreamEvent::FunctionCallDone { index, arguments } => {
                                    if let Some(&tc_idx) = tool_call_index.get(index) {
                                        current_tool_args.insert(tc_idx, arguments.clone());
                                    }
                                }
                                StreamEvent::UsageUpdate {
                                    usage,
                                    stop_reason: sr,
                                } => {
                                    if let Some(u) = usage {
                                        usage_data = Some(u.clone());
                                    }
                                    if let Some(r) = sr {
                                        stop_reason = Some(r.clone());
                                    }
                                }
                                StreamEvent::Error(_) => {}
                            }
                            callback.on_event(&stream_event);
                        } else {
                            debug!(event_type = %et, "Unhandled stream event type");
                        }
                    }
                    line_buf.clear();
                    event_type = None;
                    continue;
                }

                if let Some(et) = line.strip_prefix("event: ") {
                    event_type = Some(et.to_string());
                } else if line.starts_with("data: ") {
                    // Process any previous pending data line before starting a new one
                    if !line_buf.is_empty() {
                        if line_buf.trim() == "data: [DONE]" {
                            stream_done = true;
                        } else if let Some(data_json) = crate::streaming::parse_sse_data(&line_buf)
                        {
                            let et = event_type.as_deref().unwrap_or_else(|| {
                                data_json.get("type").and_then(|t| t.as_str()).unwrap_or("")
                            });
                            if let Some(stream_event) = adapter.parse_stream_event(et, &data_json) {
                                if let StreamEvent::Done(ref body) = stream_event {
                                    final_body = Some(body.clone());
                                    stream_done = true;
                                }
                                callback.on_event(&stream_event);
                            }
                        }
                        event_type = None;
                    }
                    line_buf = line;
                }
                // Ignore other SSE fields (id:, retry:, comments)
            }

            // Eagerly process pending line_buf for stream-terminating events
            // that arrive without a trailing blank line (e.g. last chunk).
            if !stream_done && !line_buf.is_empty() {
                if line_buf.trim() == "data: [DONE]" {
                    stream_done = true;
                } else if let Some(data_json) = crate::streaming::parse_sse_data(&line_buf) {
                    let et = event_type.as_deref().unwrap_or_else(|| {
                        data_json.get("type").and_then(|t| t.as_str()).unwrap_or("")
                    });
                    if let Some(stream_event) = adapter.parse_stream_event(et, &data_json) {
                        if let StreamEvent::Done(ref body) = stream_event {
                            final_body = Some(body.clone());
                            stream_done = true;
                        }
                        callback.on_event(&stream_event);
                    }
                }
                if stream_done {
                    line_buf.clear();
                    event_type = None;
                }
            }

            if stream_done {
                break;
            }
        }

        // Process any remaining data in buffer
        if !line_buf.is_empty()
            && let Some(data_json) = crate::streaming::parse_sse_data(&line_buf)
        {
            let et = event_type
                .as_deref()
                .unwrap_or_else(|| data_json.get("type").and_then(|t| t.as_str()).unwrap_or(""));
            if let Some(stream_event) = adapter.parse_stream_event(et, &data_json) {
                if let StreamEvent::Done(ref body) = stream_event {
                    final_body = Some(body.clone());
                }
                callback.on_event(&stream_event);
            }
        }

        // Convert the final accumulated response through the adapter
        match final_body {
            Some(body) => {
                let converted = adapter.convert_response(body);
                debug!("Streaming complete, final response converted");
                Ok(HttpResult::ok(200, converted))
            }
            None if !accumulated_text.is_empty()
                || !accumulated_reasoning.is_empty()
                || !tool_calls.is_empty() =>
            {
                // Build synthetic Chat Completions response from accumulated deltas.
                // This handles providers like Anthropic that don't send a single
                // "done" event with the full response.
                let mut message = serde_json::json!({
                    "role": "assistant",
                    "content": if accumulated_text.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(accumulated_text)
                    },
                });
                if !accumulated_reasoning.is_empty() {
                    message["reasoning_content"] = serde_json::Value::String(accumulated_reasoning);
                }
                // Finalize tool call arguments
                if !tool_calls.is_empty() {
                    let mut finalized = tool_calls;
                    for (idx, args) in &current_tool_args {
                        if let Some(tc) = finalized.get_mut(*idx)
                            && let Some(func) = tc.get_mut("function")
                        {
                            func["arguments"] = serde_json::Value::String(args.clone());
                        }
                    }
                    message["tool_calls"] = serde_json::Value::Array(finalized);
                }
                // Normalize provider-specific stop reasons to Chat Completions values
                let finish = match stop_reason.as_deref() {
                    Some("end_turn") => "stop",
                    Some("max_tokens") => "length",
                    Some("tool_use") => "tool_calls",
                    Some(other) => other,
                    None => {
                        if message.get("tool_calls").is_some() {
                            "tool_calls"
                        } else {
                            "stop"
                        }
                    }
                };
                let response = serde_json::json!({
                    "id": "stream-accumulated",
                    "object": "chat.completion",
                    "model": "",
                    "choices": [{"index": 0, "message": message, "finish_reason": finish}],
                    "usage": usage_data.unwrap_or(serde_json::json!({})),
                });
                debug!("Streaming complete, built response from accumulated deltas");
                Ok(HttpResult::ok(200, response))
            }
            None => {
                let reason = stream_end_reason.unwrap_or("unknown");
                warn!(reason = %reason, "Stream ended with no content");
                Ok(HttpResult::fail(
                    format!("No response received from stream ({reason})"),
                    true,
                ))
            }
        }
    }

    /// POST JSON with SSE streaming via curl subprocess.
    ///
    /// Reads Chat Completions SSE format from curl stdout and calls the callback.
    async fn post_json_streaming_curl(
        &self,
        payload: &serde_json::Value,
        auth_header: &str,
        callback: &dyn StreamCallback,
        cancel: Option<&CancellationToken>,
    ) -> Result<HttpResult, HttpError> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let url = self.client.api_url();
        debug!(url = %url, "Sending streaming request via curl");

        let body = serde_json::to_vec(payload)
            .map_err(|e| HttpError::Other(format!("serialize error: {e}")))?;

        let mut child = tokio::process::Command::new("curl")
            .args([
                "--silent",
                "--show-error",
                "--http1.1",
                "--no-buffer",
                "--location",
                "--request",
                "POST",
                url,
                "--header",
                auth_header,
                "--header",
                "Content-Type: application/json",
                "--data-binary",
                "@-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| HttpError::Other(format!("Failed to spawn curl: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&body)
                .await
                .map_err(|e| HttpError::Other(format!("curl stdin write error: {e}")))?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HttpError::Other("No stdout from curl".to_string()))?;

        let mut accumulated_text = String::new();
        let mut tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut current_tool_args: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        let mut tool_call_index: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        let mut stop_reason: Option<String> = None;
        let mut stream_done = false;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        loop {
            if let Some(token) = cancel
                && token.is_cancelled()
            {
                let _ = child.kill().await;
                return Ok(HttpResult::interrupted());
            }

            line.clear();
            let bytes_read = match reader.read_line(&mut line).await {
                Ok(n) => n,
                Err(e) => {
                    warn!(error = %e, "curl SSE read error");
                    break;
                }
            };
            if bytes_read == 0 {
                break;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(':') {
                continue;
            }
            let data = match trimmed.strip_prefix("data: ") {
                Some(d) => d.trim(),
                None => continue,
            };
            if data == "[DONE]" {
                stream_done = true;
                break;
            }

            let chunk: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                for choice in choices {
                    let delta = match choice.get("delta") {
                        Some(d) => d,
                        None => continue,
                    };

                    if let Some(text) = delta.get("content").and_then(|c| c.as_str())
                        && !text.is_empty()
                    {
                        accumulated_text.push_str(text);
                        callback.on_event(&StreamEvent::TextDelta(text.to_string()));
                    }

                    if let Some(tc_deltas) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc_delta in tc_deltas {
                            let idx = tc_delta.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                as usize;
                            let tc_idx = if let Some(&existing) = tool_call_index.get(&idx) {
                                existing
                            } else {
                                let new_idx = tool_calls.len();
                                let call_id = tc_delta
                                    .get("id")
                                    .and_then(|i| i.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = tc_delta
                                    .get("function")
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                tool_calls.push(serde_json::json!({
                                    "id": call_id, "type": "function",
                                    "function": { "name": name, "arguments": "" }
                                }));
                                tool_call_index.insert(idx, new_idx);
                                current_tool_args.insert(new_idx, String::new());
                                new_idx
                            };
                            if let Some(args) = tc_delta
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|a| a.as_str())
                            {
                                current_tool_args.entry(tc_idx).or_default().push_str(args);
                            }
                        }
                    }

                    if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str())
                        && !reason.is_empty()
                        && reason != "null"
                    {
                        stop_reason = Some(reason.to_string());
                    }
                }
            }
        }

        let _ = child.wait().await;

        // Finalize tool call arguments
        for (idx, args) in &current_tool_args {
            if let Some(tc) = tool_calls.get_mut(*idx)
                && let Some(func) = tc.get_mut("function")
            {
                func["arguments"] = serde_json::Value::String(args.clone());
            }
        }

        let mut message = serde_json::json!({
            "role": "assistant",
            "content": if accumulated_text.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(accumulated_text)
            },
        });
        if !tool_calls.is_empty() {
            message["tool_calls"] = serde_json::Value::Array(tool_calls);
        }

        let finish = match stop_reason.as_deref() {
            Some("tool_calls") => "tool_calls",
            Some(other) if !other.is_empty() => other,
            _ => {
                if message.get("tool_calls").is_some() {
                    "tool_calls"
                } else {
                    "stop"
                }
            }
        };

        if !stream_done && message["content"].is_null() && message.get("tool_calls").is_none() {
            warn!("curl stream ended with no content");
            return Ok(HttpResult::fail(
                "No response received from curl stream",
                true,
            ));
        }

        let response = serde_json::json!({
            "id": "curl-stream",
            "object": "chat.completion",
            "model": "",
            "choices": [{"index": 0, "message": message, "finish_reason": finish}],
            "usage": {}
        });
        Ok(HttpResult::ok(200, response))
    }

    /// Get the configured API URL.
    pub fn api_url(&self) -> &str {
        self.client.api_url()
    }
}

impl std::fmt::Debug for AdaptedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdaptedClient")
            .field("api_url", &self.client.api_url())
            .field(
                "adapter",
                &self
                    .adapter
                    .as_ref()
                    .map(|a| a.provider_name())
                    .unwrap_or("none"),
            )
            .finish()
    }
}

#[cfg(test)]
#[path = "adapted_client_tests.rs"]
mod tests;
