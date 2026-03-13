//! Web fetch tool — fetch URL content via HTTP.

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Maximum response body size (1 MB).
const MAX_BODY_SIZE: usize = 1_024 * 1_024;

/// Tool for fetching web page content.
#[derive(Debug)]
pub struct WebFetchTool;

#[async_trait::async_trait]
impl BaseTool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch the content of a URL. Returns the response body as text."
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

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        let mut request = client.get(url);

        // Add custom headers
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

        if status >= 400 {
            return ToolResult {
                success: false,
                output: Some(body),
                error: Some(format!("HTTP {status}")),
                metadata,
                duration_ms: None,
            };
        }

        ToolResult::ok_with_metadata(body, metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_web_fetch_missing_url() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("url is required"));
    }

    #[tokio::test]
    async fn test_web_fetch_invalid_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("url", serde_json::json!("ftp://example.com"))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("http://"));
    }

    #[tokio::test]
    async fn test_web_fetch_bad_host() {
        let tool = WebFetchTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "url",
            serde_json::json!("http://this-host-does-not-exist-12345.invalid"),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
    }
}
