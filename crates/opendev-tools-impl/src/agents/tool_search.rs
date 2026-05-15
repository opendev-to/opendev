//! ToolSearch — on-demand tool schema fetching.
//!
//! Like Claude Code's tool deferral, only core tools are included in every
//! LLM API call. Deferred tools are listed by name in the system prompt.
//! The LLM calls ToolSearch to fetch full schemas for tools it needs,
//! which activates them for subsequent API calls.

use std::collections::HashMap;
use std::sync::Arc;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool that fetches full schema definitions for deferred tools so they
/// can be called in subsequent turns.
#[derive(Debug)]
pub struct ToolSearchTool {
    registry: Arc<opendev_tools_core::ToolRegistry>,
}

impl ToolSearchTool {
    pub fn new(registry: Arc<opendev_tools_core::ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl BaseTool for ToolSearchTool {
    fn name(&self) -> &str {
        "ToolSearch"
    }

    fn description(&self) -> &str {
        "Fetch full schema definitions for deferred tools so they can be called.\n\n\
         Deferred tools appear by name in <system-reminder> messages. Until fetched, \
         only the name is known — there is no parameter schema, so the tool cannot \
         be invoked. This tool takes a query, matches it against the deferred tool \
         list, and returns the matched tools' complete schemas. Once fetched, the \
         tool becomes callable in subsequent turns.\n\n\
         Query forms:\n\
         - \"select:Read,Edit,Grep\" — fetch these exact tools by name\n\
         - \"notebook jupyter\" — keyword search, up to max_results best matches\n\
         - \"+web fetch\" — require \"web\" in the name, rank by remaining terms"
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query to find deferred tools. Use \"select:<tool_name>\" for direct selection, or keywords to search."
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::fail("Missing required parameter: query"),
        };
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let deferred = self.registry.get_deferred_summaries();

        // Parse query
        let matched_names: Vec<String> = if let Some(names) = query.strip_prefix("select:") {
            // Direct selection: "select:WebFetch,WebSearch"
            names
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|name| deferred.iter().any(|(n, _)| n == name))
                .collect()
        } else {
            // Keyword search
            let (required_prefix, search_terms) = if let Some(rest) = query.strip_prefix('+') {
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                let prefix = parts[0].to_lowercase();
                let terms = parts.get(1).unwrap_or(&"").to_lowercase();
                (Some(prefix), terms)
            } else {
                (None, query.to_lowercase())
            };

            let terms: Vec<&str> = search_terms.split_whitespace().collect();

            let mut scored: Vec<(usize, &str, &str)> = deferred
                .iter()
                .filter_map(|(name, desc)| {
                    let name_lower = name.to_lowercase();
                    let desc_lower = desc.to_lowercase();

                    // Required prefix filter
                    if let Some(ref prefix) = required_prefix
                        && !name_lower.contains(prefix.as_str())
                    {
                        return None;
                    }

                    // Score by matching terms
                    let mut score = 0usize;
                    for term in &terms {
                        if name_lower.contains(term) {
                            score += 3; // Name match is worth more
                        } else if desc_lower.contains(term) {
                            score += 1;
                        }
                    }
                    if score > 0 || terms.is_empty() {
                        Some((score, name.as_str(), desc.as_str()))
                    } else {
                        None
                    }
                })
                .collect();

            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored
                .into_iter()
                .take(max_results)
                .map(|(_, name, _)| name.to_string())
                .collect()
        };

        if matched_names.is_empty() {
            return ToolResult::fail(format!(
                "No deferred tools matched query: \"{query}\". \
                 Available deferred tools: {}",
                deferred
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Build detailed output with schemas
        let mut output_parts = Vec::new();
        let mut activated = Vec::new();

        for name in &matched_names {
            if let Some(tool) = self.registry.get(name) {
                let schema = serde_json::json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "parameters": tool.parameter_schema()
                });
                output_parts.push(format!(
                    "## {}\n{}",
                    tool.name(),
                    serde_json::to_string_pretty(&schema).unwrap_or_default()
                ));
                activated.push(serde_json::Value::String(tool.name().to_string()));
            }
        }

        let output = format!(
            "Found {} tool(s) matching your query:\n\n{}",
            output_parts.len(),
            output_parts.join("\n\n")
        );

        // Signal which tools to activate via metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "activated_tools".to_string(),
            serde_json::Value::Array(activated),
        );

        ToolResult::ok_with_metadata(output, metadata)
    }
}
