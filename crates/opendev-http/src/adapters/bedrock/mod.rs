//! AWS Bedrock provider adapter.
//!
//! Transforms OpenAI Chat Completions payloads to Amazon Bedrock's
//! `InvokeModel` format and converts responses back.
//!
//! Bedrock uses SigV4 request signing. Since `aws-sigv4` is not available
//! as a dependency, the signing logic is stubbed with a TODO. In production,
//! either add `aws-sigv4`/`aws-credential-types` crates or implement
//! minimal HMAC-SHA256 signing with the `hmac` and `sha2` crates.
//!
//! Environment variables:
//! - `AWS_ACCESS_KEY_ID` — IAM access key
//! - `AWS_SECRET_ACCESS_KEY` — IAM secret key
//! - `AWS_REGION` — AWS region (defaults to `us-east-1`)
//! - `AWS_SESSION_TOKEN` — optional session token for temporary credentials

mod request;
mod response;

use serde_json::{Value, json};

/// Default AWS region when `AWS_REGION` is not set.
const DEFAULT_REGION: &str = "us-east-1";

/// Adapter for Amazon Bedrock's InvokeModel API.
///
/// Bedrock wraps foundation models behind a REST API at:
/// `https://bedrock-runtime.{region}.amazonaws.com/model/{model_id}/invoke`
///
/// This adapter handles:
/// - Converting Chat Completions messages to Bedrock's Anthropic-style format
/// - Building the correct endpoint URL from region + model
/// - SigV4 header generation (TODO: requires `hmac`/`sha2` crates)
#[derive(Debug, Clone)]
pub struct BedrockAdapter {
    region: String,
    model_id: String,
    api_url: String,
}

impl BedrockAdapter {
    /// Create a new Bedrock adapter for the given model.
    ///
    /// Reads `AWS_REGION` from the environment (defaults to `us-east-1`).
    pub fn new(model_id: impl Into<String>) -> Self {
        let model_id = model_id.into();
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| DEFAULT_REGION.to_string());
        let api_url = Self::build_url(&region, &model_id);
        Self {
            region,
            model_id,
            api_url,
        }
    }

    /// Create a new Bedrock adapter with a custom region.
    pub fn with_region(model_id: impl Into<String>, region: impl Into<String>) -> Self {
        let model_id = model_id.into();
        let region = region.into();
        let api_url = Self::build_url(&region, &model_id);
        Self {
            region,
            model_id,
            api_url,
        }
    }

    /// Build the Bedrock InvokeModel URL.
    fn build_url(region: &str, model_id: &str) -> String {
        format!("https://bedrock-runtime.{region}.amazonaws.com/model/{model_id}/invoke")
    }

    /// Get the configured AWS region.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Get the model ID.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Generate SigV4 authorization headers for the request.
    ///
    /// TODO: Implement SigV4 signing. Requires either:
    /// - Adding `aws-sigv4` + `aws-credential-types` crates, or
    /// - Adding `hmac` + `sha2` crates for minimal manual signing.
    ///
    /// For now, this returns empty headers. To use Bedrock in production,
    /// implement the SigV4 signing algorithm:
    /// 1. Create canonical request (method, URI, query, headers, payload hash)
    /// 2. Create string-to-sign (algorithm, date, scope, canonical request hash)
    /// 3. Derive signing key via HMAC-SHA256 chain (date → region → service → signing)
    /// 4. Calculate signature = HMAC-SHA256(signing_key, string_to_sign)
    /// 5. Build Authorization header
    #[allow(dead_code)]
    fn sigv4_headers(&self, _payload: &[u8]) -> Vec<(String, String)> {
        let _access_key = std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default();
        let _secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").unwrap_or_default();
        let _session_token = std::env::var("AWS_SESSION_TOKEN").ok();

        // TODO: Implement SigV4 signing when `hmac` and `sha2` crates are available.
        vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
        ]
    }
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for BedrockAdapter {
    fn provider_name(&self) -> &str {
        "bedrock"
    }

    fn convert_request(&self, mut payload: Value) -> Value {
        // Strip internal reasoning effort field (Bedrock doesn't support it)
        payload
            .as_object_mut()
            .map(|obj| obj.remove("_reasoning_effort"));

        request::extract_system(&mut payload);
        request::convert_tools(&mut payload);
        request::convert_tool_messages(&mut payload);
        request::ensure_max_tokens(&mut payload);

        // Bedrock wraps the model in the URL, not the payload.
        // Remove fields Bedrock does not accept.
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("model");
            obj.remove("n");
            obj.remove("frequency_penalty");
            obj.remove("presence_penalty");
            obj.remove("logprobs");
            obj.remove("stream");
        }

        // Set anthropic_version required by Bedrock's Anthropic models.
        payload["anthropic_version"] = json!("bedrock-2023-05-31");

        payload
    }

    fn convert_response(&self, response: Value) -> Value {
        response::response_to_chat_completions(response, &self.model_id)
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }

    fn extra_headers(&self) -> Vec<(String, String)> {
        // TODO: SigV4 headers should be generated per-request with the payload.
        // The ProviderAdapter trait's `extra_headers()` is called without payload
        // context, so full SigV4 signing would require a trait extension.
        // For now, return content-type headers only.
        vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
        ]
    }
}

#[cfg(test)]
mod tests;
