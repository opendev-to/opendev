//! Web fetch tool — fetch URL content via HTTP.
//!
//! Supports optional HTML-to-markdown extraction for LLM-friendly output,
//! mirroring Python's `extract_text` parameter.

mod html_converter;

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use html_converter::html_to_markdown;

/// Maximum response body size (1 MB).
const MAX_BODY_SIZE: usize = 1_024 * 1_024;

/// Maximum timeout (120 seconds).
const MAX_TIMEOUT_SECS: u64 = 120;

/// Default timeout (30 seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Tool for fetching web page content.
#[derive(Debug)]
pub struct WebFetchTool;

#[async_trait::async_trait]
impl BaseTool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch the content of a URL. Supports optional HTML-to-markdown extraction for clean, LLM-friendly output."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "headers": {
                    "type": "object",
                    "description": "Optional HTTP headers as key-value pairs"
                },
                "extract_markdown": {
                    "type": "boolean",
                    "description": "Convert HTML to clean markdown for easier reading (default: true for HTML content)"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html"],
                    "description": "Output format: 'text' for plain text, 'markdown' for HTML-to-markdown conversion (default for HTML), 'html' for raw HTML"
                },
                "timeout": {
                    "type": "number",
                    "description": "Request timeout in seconds (default: 30, max: 120)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::fail("url is required"),
        };

        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::fail("URL must start with http:// or https://");
        }

        // Parse timeout (capped at MAX_TIMEOUT_SECS).
        let timeout_secs = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .map(|t| t.min(MAX_TIMEOUT_SECS))
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        // Parse format parameter.
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("markdown");

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        // Build Accept header based on format.
        let accept_header = match format {
            "html" => "text/html,application/xhtml+xml,*/*;q=0.8",
            "text" => "text/plain,text/html;q=0.5,*/*;q=0.3",
            _ => "text/html,application/xhtml+xml,text/plain;q=0.8,*/*;q=0.5", // markdown (default)
        };

        let mut request = client
            .get(url)
            .header("Accept", accept_header)
            .header("Accept-Language", "en-US,en;q=0.9");

        // Add custom headers (may override Accept/Accept-Language).
        if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
            for (key, value) in headers {
                if let Some(val) = value.as_str() {
                    request = request.header(key.as_str(), val);
                }
            }
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::fail(format!("Request failed: {e}")),
        };

        let status = response.status().as_u16();

        // Detect Cloudflare bot challenge: 403 with cf-mitigated header.
        let is_cf_blocked = status == 403
            && response
                .headers()
                .get("cf-mitigated")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("challenge"));

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::fail(format!("Failed to read response body: {e}")),
        };

        // Retry with simpler User-Agent if Cloudflare blocked us.
        let (status, content_type, body) = if is_cf_blocked {
            tracing::debug!("Cloudflare challenge detected, retrying with simpler UA");
            let retry = client
                .get(url)
                .header("User-Agent", "opendev")
                .header("Accept", accept_header)
                .header("Accept-Language", "en-US,en;q=0.9")
                .send()
                .await;
            match retry {
                Ok(r) => {
                    let s = r.status().as_u16();
                    let ct = r
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("unknown")
                        .to_string();
                    let b = r.text().await.unwrap_or_default();
                    (s, ct, b)
                }
                Err(_) => (status, content_type, body), // fall back to original
            }
        } else {
            (status, content_type, body)
        };

        // Determine if we should extract markdown based on format and extract_markdown params.
        let extract_markdown = match format {
            "html" => false, // raw HTML requested
            "text" => false, // plain text, no conversion
            _ => {
                // "markdown" or default: respect extract_markdown param or auto-detect
                args.get("extract_markdown")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(content_type.contains("html"))
            }
        };

        // Convert HTML to markdown if requested and content is HTML
        let body = if extract_markdown && content_type.contains("html") {
            html_to_markdown(&body)
        } else {
            body
        };

        let truncated = body.len() > MAX_BODY_SIZE;
        let body = if truncated {
            format!(
                "{}...\n\n[truncated, showing first {} bytes of {}]",
                &body[..MAX_BODY_SIZE],
                MAX_BODY_SIZE,
                body.len()
            )
        } else {
            body
        };

        let mut metadata = HashMap::new();
        metadata.insert("status".into(), serde_json::json!(status));
        metadata.insert("content_type".into(), serde_json::json!(content_type));
        metadata.insert("truncated".into(), serde_json::json!(truncated));
        metadata.insert(
            "extracted_markdown".into(),
            serde_json::json!(extract_markdown),
        );

        if status >= 400 {
            return ToolResult {
                success: false,
                output: Some(body),
                error: Some(format!("HTTP {status}")),
                metadata,
                duration_ms: None,
                llm_suffix: None,
            };
        }

        ToolResult::ok_with_metadata(body, metadata)
    }
}

#[cfg(test)]
mod tests;
