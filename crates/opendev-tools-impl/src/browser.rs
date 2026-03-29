//! Browser automation tool — headless browser interaction.
//!
//! Provides browser automation actions (navigate, click, type, fill,
//! screenshot, get_text, evaluate JS, etc.) using a headless browser.
//!
//! This implementation uses `reqwest` for simple page fetching and
//! JavaScript-free DOM extraction. For full Playwright-equivalent
//! functionality, a Playwright/Chrome DevTools Protocol (CDP) bridge
//! would be needed. This tool degrades gracefully when full browser
//! automation is not available, offering HTTP-based fallbacks.

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolDisplayMeta, ToolResult};

/// Maximum page body size to process (5 MB).
const MAX_PAGE_SIZE: usize = 5 * 1024 * 1024;

/// Maximum text content length to return.
const MAX_TEXT_LENGTH: usize = 5000;

/// Default action timeout in milliseconds.
const DEFAULT_TIMEOUT_MS: u64 = 10_000;

/// Available browser actions.
const AVAILABLE_ACTIONS: &[&str] = &[
    "navigate",
    "get_text",
    "screenshot",
    "evaluate",
    "back",
    "forward",
    "reload",
    "tabs_list",
    "tab_close",
    "click",
    "type",
    "fill",
    "wait",
];

/// Tool for browser automation.
#[derive(Debug)]
pub struct BrowserTool;

#[async_trait::async_trait]
impl BaseTool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Interactive browser automation. Supports actions: navigate, click, type, fill, \
         screenshot, get_text, wait, evaluate, tabs_list, tab_close, back, forward, reload."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Browser action to perform",
                    "enum": AVAILABLE_ACTIONS
                },
                "target": {
                    "type": "string",
                    "description": "Target for the action (URL, CSS selector, JS expression)"
                },
                "value": {
                    "type": "string",
                    "description": "Value for the action (text to type, JS to evaluate)"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Action timeout in milliseconds (default: 10000)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        let target = args.get("target").and_then(|v| v.as_str());
        let value = args.get("value").and_then(|v| v.as_str());
        let _timeout = args
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        match action {
            "navigate" => self.navigate(target, ctx).await,
            "get_text" => self.get_text(target, ctx).await,
            "screenshot" => self.screenshot(target, ctx).await,
            "click" => self.click(target).await,
            "type" => self.type_text(target, value).await,
            "fill" => self.fill(target, value).await,
            "wait" => self.wait(target).await,
            "evaluate" => self.evaluate(target, value).await,
            "tabs_list" => self.tabs_list().await,
            "tab_close" => self.tab_close(target).await,
            "back" => self.back().await,
            "forward" => self.forward().await,
            "reload" => self.reload().await,
            other => ToolResult::fail(format!(
                "Unknown browser action: {other}. Available: {}",
                AVAILABLE_ACTIONS.join(", ")
            )),
        }
    }

    fn display_meta(&self) -> Option<ToolDisplayMeta> {
        Some(ToolDisplayMeta {
            verb: "Browse",
            label: "page",
            category: "Web",
            primary_arg_keys: &["action", "target"],
        })
    }
}

impl BrowserTool {
    /// Navigate to a URL and return page info.
    ///
    /// Uses HTTP GET to fetch the page. For JavaScript-rendered pages,
    /// a real browser engine would be needed.
    async fn navigate(&self, target: Option<&str>, _ctx: &ToolContext) -> ToolResult {
        let url = match target {
            Some(u) if !u.is_empty() => u,
            _ => return ToolResult::fail("URL is required for navigate"),
        };

        // Normalize URL
        let url = normalize_url(url);

        let client = match build_client() {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        let response = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::fail(format!("Navigation failed: {e}")),
        };

        let status = response.status().as_u16();
        let final_url = response.url().to_string();

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::fail(format!("Failed to read page: {e}")),
        };

        let title = extract_title(&body).unwrap_or_else(|| "Untitled".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("status".into(), serde_json::json!(status));
        metadata.insert("url".into(), serde_json::json!(final_url));
        metadata.insert("title".into(), serde_json::json!(title));

        ToolResult::ok_with_metadata(
            format!("Navigated to: {url}\nTitle: {title}\nURL: {final_url}"),
            metadata,
        )
    }

    /// Get text content from the page or a specific element.
    async fn get_text(&self, target: Option<&str>, _ctx: &ToolContext) -> ToolResult {
        // Without a real browser session, we need a URL to fetch
        let url = match target {
            Some(t) if t.starts_with("http://") || t.starts_with("https://") => t,
            Some(selector) => {
                return ToolResult::fail(format!(
                    "CSS selector '{selector}' requires an active browser session. \
                     Use 'navigate' action first, then provide a URL for get_text, or \
                     use the web_fetch tool instead."
                ));
            }
            None => {
                return ToolResult::fail("Target (URL or CSS selector) is required for get_text");
            }
        };

        let client = match build_client() {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::fail(format!("Request failed: {e}")),
        };

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::fail(format!("Failed to read response: {e}")),
        };

        // Extract visible text from HTML
        let text = extract_visible_text(&body);

        let truncated = text.len() > MAX_TEXT_LENGTH;
        let text = if truncated {
            format!("{}...\n[truncated]", &text[..MAX_TEXT_LENGTH])
        } else {
            text
        };

        ToolResult::ok(text)
    }

    /// Capture a screenshot — saves as HTML snapshot since we don't have a real browser.
    async fn screenshot(&self, target: Option<&str>, _ctx: &ToolContext) -> ToolResult {
        let url = match target {
            Some(u) if u.starts_with("http://") || u.starts_with("https://") => u,
            Some(_) | None => {
                return ToolResult::fail(
                    "URL is required for screenshot. Use web_screenshot tool for full \
                     browser screenshots with JavaScript rendering.",
                );
            }
        };

        let client = match build_client() {
            Ok(c) => c,
            Err(e) => return ToolResult::fail(format!("Failed to create HTTP client: {e}")),
        };

        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::fail(format!("Request failed: {e}")),
        };

        let body = match response.text().await {
            Ok(t) => {
                if t.len() > MAX_PAGE_SIZE {
                    t[..MAX_PAGE_SIZE].to_string()
                } else {
                    t
                }
            }
            Err(e) => return ToolResult::fail(format!("Failed to read page: {e}")),
        };

        // Save HTML snapshot
        let screenshot_dir = std::env::temp_dir().join("opendev-screenshots");
        std::fs::create_dir_all(&screenshot_dir).ok();
        let filename = format!("browser_{}.html", std::process::id());
        let path = screenshot_dir.join(&filename);

        match std::fs::write(&path, &body) {
            Ok(_) => {
                let mut metadata = HashMap::new();
                metadata.insert(
                    "screenshot_path".into(),
                    serde_json::json!(path.to_string_lossy()),
                );
                metadata.insert("format".into(), serde_json::json!("html"));
                metadata.insert(
                    "note".into(),
                    serde_json::json!(
                        "HTML snapshot saved. For rendered screenshots, use the web_screenshot tool."
                    ),
                );

                ToolResult::ok_with_metadata(
                    format!(
                        "HTML snapshot saved: {}\nPage: {url}\n\
                         Note: For visual screenshots, use the web_screenshot tool.",
                        path.display()
                    ),
                    metadata,
                )
            }
            Err(e) => ToolResult::fail(format!("Failed to save snapshot: {e}")),
        }
    }

    /// Click action — requires active browser session.
    async fn click(&self, target: Option<&str>) -> ToolResult {
        let selector = match target {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::fail("CSS selector is required for click"),
        };
        ToolResult::fail(format!(
            "Click on '{selector}' requires a browser session with JavaScript support. \
             Consider using the web_fetch tool for content retrieval, or the bash tool \
             to run a headless browser script."
        ))
    }

    /// Type text into an element.
    async fn type_text(&self, target: Option<&str>, value: Option<&str>) -> ToolResult {
        let selector = match target {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::fail("CSS selector is required for type"),
        };
        let _text = match value {
            Some(v) => v,
            None => return ToolResult::fail("value (text) is required for type"),
        };
        ToolResult::fail(format!(
            "Typing into '{selector}' requires a browser session with JavaScript support. \
             Consider using curl/wget via the bash tool for form submission."
        ))
    }

    /// Fill a form field.
    async fn fill(&self, target: Option<&str>, value: Option<&str>) -> ToolResult {
        let selector = match target {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::fail("CSS selector is required for fill"),
        };
        let _text = match value {
            Some(v) => v,
            None => return ToolResult::fail("value (text) is required for fill"),
        };
        ToolResult::fail(format!(
            "Filling '{selector}' requires a browser session with JavaScript support."
        ))
    }

    /// Wait for an element — requires active browser session.
    async fn wait(&self, target: Option<&str>) -> ToolResult {
        let selector = match target {
            Some(s) if !s.is_empty() => s,
            _ => return ToolResult::fail("CSS selector is required for wait"),
        };
        ToolResult::fail(format!(
            "Waiting for '{selector}' requires a browser session with JavaScript support."
        ))
    }

    /// Evaluate JavaScript — requires active browser session.
    async fn evaluate(&self, target: Option<&str>, value: Option<&str>) -> ToolResult {
        let _js_code = value.or(target);
        if _js_code.is_none() || _js_code.unwrap().is_empty() {
            return ToolResult::fail("JavaScript expression is required for evaluate");
        }
        ToolResult::fail(
            "JavaScript evaluation requires a browser session. \
             Consider using the bash tool to run Node.js scripts."
                .to_string(),
        )
    }

    /// List open tabs — no persistent browser state in HTTP mode.
    async fn tabs_list(&self) -> ToolResult {
        ToolResult::ok(
            "No browser context open (HTTP-only mode). \
                        Use 'navigate' to fetch a page.",
        )
    }

    /// Close a tab.
    async fn tab_close(&self, _target: Option<&str>) -> ToolResult {
        ToolResult::ok("No browser context open (HTTP-only mode).")
    }

    /// Navigate back.
    async fn back(&self) -> ToolResult {
        ToolResult::fail("Browser history navigation requires a persistent browser session.")
    }

    /// Navigate forward.
    async fn forward(&self) -> ToolResult {
        ToolResult::fail("Browser history navigation requires a persistent browser session.")
    }

    /// Reload the current page.
    async fn reload(&self) -> ToolResult {
        ToolResult::fail(
            "Reload requires a persistent browser session. Use 'navigate' to re-fetch a URL.",
        )
    }
}

/// Build an HTTP client with browser-like settings.
fn build_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
             AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .build()
}

/// Normalize a URL, adding `https://` if no scheme is present.
fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("https://") || url.starts_with("http://") {
        return url.to_string();
    }
    if url.starts_with("https:/") && !url.starts_with("https://") {
        return url.replacen("https:/", "https://", 1);
    }
    if url.starts_with("http:/") && !url.starts_with("http://") {
        return url.replacen("http:/", "http://", 1);
    }
    format!("https://{url}")
}

/// Extract the `<title>` from HTML.
fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let rest = &html[start..];
    let tag_end = rest.find('>')?;
    let after_tag = &rest[tag_end + 1..];
    let end = after_tag.find('<')?;
    let title = after_tag[..end].trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some(html_decode(&title))
    }
}

/// Extract visible text from HTML, stripping tags, scripts, and styles.
fn extract_visible_text(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if !in_tag && chars[i] == '<' {
            in_tag = true;
            // Check if entering script or style
            let remaining: String = lower_chars[i..].iter().take(20).collect();
            if remaining.starts_with("<script") {
                in_script = true;
            } else if remaining.starts_with("<style") {
                in_style = true;
            } else if remaining.starts_with("</script") {
                in_script = false;
            } else if remaining.starts_with("</style") {
                in_style = false;
            }
        } else if in_tag && chars[i] == '>' {
            in_tag = false;
            // Add space to separate content from different tags
            if !result.ends_with(' ') && !result.ends_with('\n') {
                result.push(' ');
            }
        } else if !in_tag && !in_script && !in_style {
            result.push(chars[i]);
        }
        i += 1;
    }

    // Decode HTML entities and collapse whitespace
    let decoded = html_decode(&result);
    let lines: Vec<&str> = decoded
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

/// Decode common HTML entities.
fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
