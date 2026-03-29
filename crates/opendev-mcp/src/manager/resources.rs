//! MCP resource and prompt operations.

use std::collections::HashMap;

use tracing::debug;

use crate::error::{McpError, McpResult};
use crate::models::{JsonRpcRequest, McpContent, McpPromptResult, McpPromptSummary, McpResource};

use super::McpManager;

impl McpManager {
    /// List prompts from all connected servers.
    pub async fn list_prompts(&self) -> Vec<McpPromptSummary> {
        let connections = self.connections.read().await;
        let mut prompts = Vec::new();

        for (server_name, conn) in connections.iter() {
            let request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: self.next_request_id(),
                method: "prompts/list".to_string(),
                params: None,
            };

            match conn.transport.send_request(&request).await {
                Ok(response) => {
                    if let Some(result) = response.result
                        && let Some(prompt_list) = result.get("prompts").and_then(|p| p.as_array())
                    {
                        for prompt_val in prompt_list {
                            let name = prompt_val
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            let description = prompt_val
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_string();
                            let arguments = prompt_val
                                .get("arguments")
                                .and_then(|a| a.as_array())
                                .map(|args| {
                                    args.iter()
                                        .filter_map(|a| {
                                            a.get("name").and_then(|n| n.as_str()).map(String::from)
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();

                            prompts.push(McpPromptSummary {
                                server_name: server_name.clone(),
                                prompt_name: name.clone(),
                                description,
                                arguments,
                                command: format!("/{}:{}", server_name, name),
                            });
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to list prompts from '{}': {}", server_name, e);
                }
            }
        }

        prompts
    }

    /// Get a prompt from a specific server with optional arguments.
    ///
    /// Sends `prompts/get` to the server with the prompt name and any arguments.
    /// Returns the prompt messages that can be injected into the conversation.
    pub async fn get_prompt(
        &self,
        server_name: &str,
        prompt_name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> McpResult<McpPromptResult> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(server_name.to_string()))?;

        let mut params = HashMap::new();
        params.insert(
            "name".to_string(),
            serde_json::Value::String(prompt_name.to_string()),
        );
        if let Some(args) = arguments {
            params.insert("arguments".to_string(), serde_json::to_value(args).unwrap());
        }

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "prompts/get".to_string(),
            params: Some(params),
        };

        let response = conn.transport.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(McpError::Protocol(format!(
                "prompts/get failed: {}",
                error.message
            )));
        }

        let result = response
            .result
            .ok_or_else(|| McpError::Protocol("prompts/get returned no result".to_string()))?;

        serde_json::from_value(result)
            .map_err(|e| McpError::Protocol(format!("Failed to parse prompt result: {e}")))
    }

    /// List resources from all connected servers.
    ///
    /// Sends `resources/list` to each connected server and aggregates the results.
    pub async fn list_resources(&self) -> Vec<(String, McpResource)> {
        let connections = self.connections.read().await;
        let mut resources = Vec::new();

        for (server_name, conn) in connections.iter() {
            let request = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: self.next_request_id(),
                method: "resources/list".to_string(),
                params: None,
            };

            match conn.transport.send_request(&request).await {
                Ok(response) => {
                    if let Some(result) = response.result
                        && let Some(resource_list) =
                            result.get("resources").and_then(|r| r.as_array())
                    {
                        for res_val in resource_list {
                            if let Ok(resource) =
                                serde_json::from_value::<McpResource>(res_val.clone())
                            {
                                resources.push((server_name.clone(), resource));
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to list resources from '{}': {}", server_name, e);
                }
            }
        }

        resources
    }

    /// Read a specific resource from a server.
    ///
    /// Sends `resources/read` with the resource URI and returns the content.
    pub async fn read_resource(
        &self,
        server_name: &str,
        resource_uri: &str,
    ) -> McpResult<Vec<McpContent>> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(server_name.to_string()))?;

        let mut params = HashMap::new();
        params.insert(
            "uri".to_string(),
            serde_json::Value::String(resource_uri.to_string()),
        );

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_request_id(),
            method: "resources/read".to_string(),
            params: Some(params),
        };

        let response = conn.transport.send_request(&request).await?;

        if let Some(error) = response.error {
            return Err(McpError::Protocol(format!(
                "resources/read failed: {}",
                error.message
            )));
        }

        let result = response
            .result
            .ok_or_else(|| McpError::Protocol("resources/read returned no result".to_string()))?;

        // Parse content array from response
        let contents = result
            .get("contents")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v: &serde_json::Value| {
                        // MCP resources return {uri, text?, blob?, mimeType?}
                        let text = v.get("text").and_then(|t| t.as_str());
                        if let Some(text) = text {
                            Some(McpContent::Text {
                                text: text.to_string(),
                            })
                        } else {
                            let blob = v.get("blob").and_then(|b| b.as_str());
                            let mime = v
                                .get("mimeType")
                                .and_then(|m| m.as_str())
                                .unwrap_or("application/octet-stream");
                            blob.map(|data: &str| McpContent::Image {
                                data: data.to_string(),
                                mime_type: mime.to_string(),
                            })
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(contents)
    }
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
