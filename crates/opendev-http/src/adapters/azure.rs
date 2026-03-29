//! Azure OpenAI adapter.
//!
//! Azure OpenAI uses the same Chat Completions format as OpenAI but with
//! a different URL scheme and an `api-version` query parameter.
//!
//! URL format: `{base}/openai/deployments/{deployment}/chat/completions?api-version={version}`
//!
//! Authentication uses `api-key` header instead of `Authorization: Bearer`.

use serde_json::Value;

const DEFAULT_API_VERSION: &str = "2024-02-15-preview";

/// Adapter for Azure OpenAI Service.
///
/// Azure OpenAI uses deployment-based URLs instead of passing the model name
/// in the request body. The adapter constructs the correct URL and adds the
/// required `api-version` query parameter.
#[derive(Debug, Clone)]
pub struct AzureOpenAiAdapter {
    /// Base URL of the Azure OpenAI resource (e.g., `https://myresource.openai.azure.com`).
    base_url: String,
    /// Deployment name (maps to the model deployed in Azure).
    deployment: String,
    /// API version query parameter.
    api_version: String,
    /// Cached full API URL.
    api_url: String,
}

impl AzureOpenAiAdapter {
    /// Create a new Azure OpenAI adapter.
    ///
    /// # Arguments
    /// * `base_url` - Azure resource URL (e.g., `https://myresource.openai.azure.com`)
    /// * `deployment` - Deployment name (e.g., `gpt-4o`)
    pub fn new(base_url: impl Into<String>, deployment: impl Into<String>) -> Self {
        let base_url = base_url.into();
        let deployment = deployment.into();
        let api_version = DEFAULT_API_VERSION.to_string();
        let api_url = build_azure_url(&base_url, &deployment, &api_version);
        Self {
            base_url,
            deployment,
            api_version,
            api_url,
        }
    }

    /// Set a custom API version.
    pub fn with_api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = version.into();
        self.api_url = build_azure_url(&self.base_url, &self.deployment, &self.api_version);
        self
    }

    /// Remove the `model` field from the request, since Azure uses the
    /// deployment name in the URL instead.
    fn strip_model(payload: &mut Value) {
        if let Some(obj) = payload.as_object_mut() {
            obj.remove("model");
        }
    }
}

/// Build the full Azure OpenAI API URL.
pub fn build_azure_url(base_url: &str, deployment: &str, api_version: &str) -> String {
    let base = base_url.trim_end_matches('/');
    format!("{base}/openai/deployments/{deployment}/chat/completions?api-version={api_version}")
}

#[async_trait::async_trait]
impl super::base::ProviderAdapter for AzureOpenAiAdapter {
    fn provider_name(&self) -> &str {
        "azure"
    }

    fn convert_request(&self, mut payload: Value) -> Value {
        Self::strip_model(&mut payload);
        // Strip internal reasoning effort field
        payload
            .as_object_mut()
            .map(|obj| obj.remove("_reasoning_effort"));
        payload
    }

    fn convert_response(&self, response: Value) -> Value {
        // Azure responses are already in Chat Completions format
        response
    }

    fn api_url(&self) -> &str {
        &self.api_url
    }

    fn extra_headers(&self) -> Vec<(String, String)> {
        // Azure uses `api-key` header for authentication (in addition to
        // the standard Authorization header, the caller may set either).
        vec![]
    }
}

#[cfg(test)]
#[path = "azure_tests.rs"]
mod tests;
