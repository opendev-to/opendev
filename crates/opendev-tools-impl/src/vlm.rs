//! Vision Language Model (VLM) tool — analyze images using vision-capable LLMs.
//!
//! Supports multiple providers (OpenAI, Fireworks, Anthropic) for image analysis.
//! Images can be provided as local file paths (base64-encoded) or URLs.

use std::collections::HashMap;
use std::path::PathBuf;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::path_utils::validate_path_access;

/// Supported image file extensions and their MIME types.
const IMAGE_MIME_TYPES: &[(&str, &str)] = &[
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

/// Default request timeout in seconds for VLM calls.
const VLM_TIMEOUT_SECS: u64 = 300;

/// Tool for analyzing images using Vision Language Models.
#[derive(Debug)]
pub struct VlmTool;

#[async_trait::async_trait]
impl BaseTool for VlmTool {
    fn name(&self) -> &str {
        "vlm"
    }

    fn description(&self) -> &str {
        "Analyze images using a Vision Language Model. Provide either a local \
         image file path or a URL, along with a text prompt describing what to analyze."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text prompt describing what to analyze in the image"
                },
                "image_path": {
                    "type": "string",
                    "description": "Path to a local image file"
                },
                "image_url": {
                    "type": "string",
                    "description": "URL of an online image"
                },
                "provider": {
                    "type": "string",
                    "description": "Provider to use: 'openai' (default), 'fireworks', or 'anthropic'",
                    "enum": ["openai", "fireworks", "anthropic"]
                },
                "model": {
                    "type": "string",
                    "description": "Model ID to use (provider-specific)"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Maximum tokens in the response (default: 4096)"
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let prompt = match args.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.trim().is_empty() => p,
            _ => return ToolResult::fail("prompt is required"),
        };

        let image_path = args.get("image_path").and_then(|v| v.as_str());
        let image_url = args.get("image_url").and_then(|v| v.as_str());

        if image_path.is_none() && image_url.is_none() {
            return ToolResult::fail("Either image_path or image_url must be provided");
        }

        let provider = args
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("openai");

        let model = args
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let max_tokens = args
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(4096) as u32;

        // Resolve the image URL
        let final_image_url = if let Some(path_str) = image_path {
            // Local file — encode to base64
            let path = {
                let p = PathBuf::from(path_str);
                if p.is_absolute() {
                    p
                } else {
                    ctx.working_dir.join(p)
                }
            };

            if let Err(msg) = validate_path_access(&path, &ctx.working_dir) {
                return ToolResult::fail(msg);
            }

            if !path.exists() {
                return ToolResult::fail(format!("Image file not found: {path_str}"));
            }

            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => return ToolResult::fail(format!("Failed to read image file: {e}")),
            };

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpeg")
                .to_lowercase();

            let mime_type = IMAGE_MIME_TYPES
                .iter()
                .find(|(e, _)| *e == ext)
                .map(|(_, m)| *m)
                .unwrap_or("image/jpeg");

            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            format!("data:{mime_type};base64,{b64}")
        } else if let Some(url) = image_url {
            if !url.starts_with("http://")
                && !url.starts_with("https://")
                && !url.starts_with("data:")
            {
                return ToolResult::fail(
                    "Invalid image URL: must start with http://, https://, or data:",
                );
            }
            url.to_string()
        } else {
            unreachable!("Already checked above");
        };

        // Get API key from environment
        let (api_key_env, default_model, api_url) = match provider {
            "openai" => (
                "OPENAI_API_KEY",
                "gpt-4o",
                "https://api.openai.com/v1/chat/completions",
            ),
            "fireworks" => (
                "FIREWORKS_API_KEY",
                "accounts/fireworks/models/llama-v3p2-90b-vision-instruct",
                "https://api.fireworks.ai/inference/v1/chat/completions",
            ),
            "anthropic" => {
                return ToolResult::fail(
                    "Anthropic vision API requires a different request format. \
                     Please use 'openai' or 'fireworks' provider for VLM analysis.",
                );
            }
            other => {
                return ToolResult::fail(format!(
                    "Unsupported provider '{other}'. Use 'openai', 'fireworks', or 'anthropic'."
                ));
            }
        };

        let api_key = match std::env::var(api_key_env) {
            Ok(k) if !k.is_empty() => k,
            _ => {
                return ToolResult::fail(format!(
                    "API key not found. Please set {api_key_env} environment variable."
                ));
            }
        };

        let model_id = model.as_deref().unwrap_or(default_model);

        // Build the request
        let payload = serde_json::json!({
            "model": model_id,
            "max_tokens": max_tokens,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": final_image_url}}
                ]
            }]
        });

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(VLM_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        let response = match client
            .post(api_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Accept", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                if e.is_timeout() {
                    return ToolResult::fail(format!(
                        "Request timed out after {VLM_TIMEOUT_SECS} seconds"
                    ));
                }
                return ToolResult::fail(format!("Request failed: {e}"));
            }
        };

        let status = response.status().as_u16();
        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::fail(format!("Failed to read response: {e}")),
        };

        if status != 200 {
            return ToolResult::fail(format!("HTTP {status}: {body}"));
        }

        // Parse response
        let response_json: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => return ToolResult::fail(format!("Failed to parse response: {e}")),
        };

        let content = response_json
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        if content.is_empty() {
            return ToolResult::fail("VLM returned empty response");
        }

        let mut metadata = HashMap::new();
        metadata.insert("model".into(), serde_json::json!(model_id));
        metadata.insert("provider".into(), serde_json::json!(provider));

        ToolResult::ok_with_metadata(content, metadata)
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
    async fn test_vlm_missing_prompt() {
        let tool = VlmTool;
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("prompt is required"));
    }

    #[tokio::test]
    async fn test_vlm_missing_image() {
        let tool = VlmTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("prompt", serde_json::json!("Describe this image"))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("image_path or image_url"));
    }

    #[tokio::test]
    async fn test_vlm_invalid_image_url() {
        let tool = VlmTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            ("prompt", serde_json::json!("Describe this")),
            ("image_url", serde_json::json!("ftp://invalid.com/img.png")),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Invalid image URL"));
    }

    #[tokio::test]
    async fn test_vlm_image_not_found() {
        let tool = VlmTool;
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[
            ("prompt", serde_json::json!("Describe this")),
            (
                "image_path",
                serde_json::json!("/tmp/nonexistent_image.png"),
            ),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_vlm_unsupported_provider() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let tool = VlmTool;
        let ctx = ToolContext::new(&dir_path);

        let img_path = dir_path.join("test_vlm.png");
        std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let args = make_args(&[
            ("prompt", serde_json::json!("Describe")),
            ("image_path", serde_json::json!(img_path.to_string_lossy())),
            ("provider", serde_json::json!("google")),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Unsupported provider"));
    }

    #[tokio::test]
    async fn test_vlm_anthropic_not_supported() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let tool = VlmTool;
        let ctx = ToolContext::new(&dir_path);

        let img_path = dir_path.join("test_vlm_anthropic.png");
        std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        let args = make_args(&[
            ("prompt", serde_json::json!("Describe")),
            ("image_path", serde_json::json!(img_path.to_string_lossy())),
            ("provider", serde_json::json!("anthropic")),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("different request format"));
    }

    #[tokio::test]
    async fn test_vlm_no_api_key() {
        let dir = tempfile::TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let tool = VlmTool;
        let ctx = ToolContext::new(&dir_path);

        let img_path = dir_path.join("test_vlm_nokey.png");
        std::fs::write(&img_path, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

        // Ensure no API key is set
        // SAFETY: test-only; ensures no API key interferes with the test.
        unsafe { std::env::remove_var("OPENAI_API_KEY") };

        let args = make_args(&[
            ("prompt", serde_json::json!("Describe")),
            ("image_path", serde_json::json!(img_path.to_string_lossy())),
            ("provider", serde_json::json!("openai")),
        ]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("API key not found"));
    }
}
