//! Web search tool — search the web via DuckDuckGo HTML scraping.
//!
//! Uses DuckDuckGo's HTML interface for privacy-respecting web searches
//! without requiring API keys. Results are parsed from the HTML response.

mod parser;

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use parser::{filter_by_domain, parse_ddg_html, urlencoded};

/// Default number of search results to return.
const DEFAULT_MAX_RESULTS: usize = 10;

/// Maximum body size to read from DuckDuckGo (256 KB).
const MAX_BODY_SIZE: usize = 256 * 1024;

/// Tool for searching the web using DuckDuckGo.
#[derive(Debug)]
pub struct WebSearchTool;

#[async_trait::async_trait]
impl BaseTool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        "Search the web using DuckDuckGo. Returns titles, URLs, and snippets."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query string"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 10)"
                },
                "allowed_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Only include results from these domains"
                },
                "blocked_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Exclude results from these domains"
                }
            },
            "required": ["query"]
        })
    }

    fn is_read_only(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn is_concurrent_safe(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Web
    }

    fn truncation_rule(&self) -> Option<opendev_tools_core::TruncationRule> {
        Some(opendev_tools_core::TruncationRule::head(10000))
    }

    fn search_hint(&self) -> Option<&str> {
        Some("search the web for information")
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) if !q.trim().is_empty() => q.trim(),
            _ => return ToolResult::fail("Search query is required"),
        };

        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_MAX_RESULTS);

        let allowed_domains: Vec<String> = args
            .get("allowed_domains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();

        let blocked_domains: Vec<String> = args
            .get("blocked_domains")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();

        // Build DuckDuckGo HTML search URL
        let encoded_query = urlencoded(query);
        let url = format!("https://html.duckduckgo.com/html/?q={encoded_query}");

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            )
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        let response = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::fail(format!("Search request failed: {e}")),
        };

        if !response.status().is_success() {
            return ToolResult::fail(format!("DuckDuckGo returned HTTP {}", response.status()));
        }

        let body = match response.text().await {
            Ok(t) => {
                if t.len() > MAX_BODY_SIZE {
                    t[..MAX_BODY_SIZE].to_string()
                } else {
                    t
                }
            }
            Err(e) => return ToolResult::fail(format!("Failed to read response: {e}")),
        };

        // Parse results from HTML
        let mut results = parse_ddg_html(&body);

        // Filter by domain
        if !allowed_domains.is_empty() || !blocked_domains.is_empty() {
            results = filter_by_domain(results, &allowed_domains, &blocked_domains);
        }

        // Limit results
        results.truncate(max_results);

        let result_count = results.len();

        // Format output
        let mut output_parts = Vec::new();
        output_parts.push(format!(
            "Search results for \"{query}\" ({result_count} results):\n"
        ));

        for (i, result) in results.iter().enumerate() {
            output_parts.push(format!(
                "{}. {}\n   {}\n   {}\n",
                i + 1,
                result.title,
                result.url,
                result.snippet
            ));
        }

        if results.is_empty() {
            output_parts.push("No results found.".to_string());
        }

        let output = output_parts.join("");

        let mut metadata = HashMap::new();
        metadata.insert("query".into(), serde_json::json!(query));
        metadata.insert("result_count".into(), serde_json::json!(result_count));
        metadata.insert(
            "results".into(),
            serde_json::to_value(&results).unwrap_or_default(),
        );

        ToolResult::ok_with_metadata(output, metadata)
    }
}

#[cfg(test)]
mod tests;
