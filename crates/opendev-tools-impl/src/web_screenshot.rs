//! Web screenshot tool — capture web page screenshots.
//!
//! Provides web page capture functionality. Since Rust doesn't have native
//! Playwright bindings, this implementation uses HTTP + HTML extraction as
//! a fallback. For full rendering, it can shell out to a headless browser
//! via the system's `chromium` or `google-chrome` CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::path_utils::{resolve_file_path, validate_path_access};

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool for capturing web page screenshots.
#[derive(Debug)]
pub struct WebScreenshotTool;

#[async_trait::async_trait]
impl BaseTool for WebScreenshotTool {
    fn name(&self) -> &str {
        "web_screenshot"
    }

    fn description(&self) -> &str {
        "Capture a screenshot of a web page. Saves as PNG using headless Chrome/Chromium, \
         or falls back to saving page HTML."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL of the web page to capture"
                },
                "output_path": {
                    "type": "string",
                    "description": "Path to save the screenshot (optional, auto-generated if not provided)"
                },
                "viewport_width": {
                    "type": "integer",
                    "description": "Browser viewport width in pixels (default: 1920)"
                },
                "viewport_height": {
                    "type": "integer",
                    "description": "Browser viewport height in pixels (default: 1080)"
                },
                "action": {
                    "type": "string",
                    "description": "Action: 'capture' (default), 'list', or 'clear'",
                    "enum": ["capture", "list", "clear"]
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("capture");

        match action {
            "list" => list_screenshots(),
            "clear" => clear_screenshots(5),
            _ => {
                let url = match args.get("url").and_then(|v| v.as_str()) {
                    Some(u) if !u.trim().is_empty() => u.trim(),
                    _ => return ToolResult::fail("url is required for screenshot capture"),
                };

                let output_path = args.get("output_path").and_then(|v| v.as_str());
                let viewport_width = args
                    .get("viewport_width")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1920) as u32;
                let viewport_height = args
                    .get("viewport_height")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1080) as u32;

                capture_screenshot(url, output_path, viewport_width, viewport_height, ctx).await
            }
        }
    }
}

/// Normalize a URL to have proper protocol prefix.
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

/// Get the screenshot storage directory.
fn screenshot_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("opendev_web_screenshots");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// Generate an output path from a URL.
fn generate_output_path(url: &str) -> PathBuf {
    // Extract domain for filename
    let domain = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("page")
        .replace([':', '/'], "_");

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    screenshot_dir().join(format!("{domain}_{timestamp}.png"))
}

/// Find a headless browser binary on the system.
fn find_browser() -> Option<String> {
    let candidates = [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];

    for candidate in &candidates {
        if let Ok(output) = std::process::Command::new("which").arg(candidate).output()
            && output.status.success()
        {
            return Some(candidate.to_string());
        }
        // Direct path check
        if Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Capture a screenshot using headless Chrome, falling back to HTML save.
async fn capture_screenshot(
    url: &str,
    output_path: Option<&str>,
    viewport_width: u32,
    viewport_height: u32,
    ctx: &ToolContext,
) -> ToolResult {
    let url = normalize_url(url);

    // Determine output path
    let dest = match output_path {
        Some(p) => {
            let resolved = resolve_file_path(p, &ctx.working_dir);
            if let Err(msg) = validate_path_access(&resolved, &ctx.working_dir) {
                return ToolResult::fail(msg);
            }
            resolved
        }
        None => generate_output_path(&url),
    };

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Try headless Chrome first
    if let Some(browser) = find_browser() {
        let window_size = format!("--window-size={viewport_width},{viewport_height}");
        let screenshot_arg = format!("--screenshot={}", dest.display());

        let result = tokio::process::Command::new(&browser)
            .args([
                "--headless",
                "--disable-gpu",
                "--no-sandbox",
                "--disable-software-rasterizer",
                "--disable-dev-shm-usage",
                &window_size,
                &screenshot_arg,
                &url,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() && dest.exists() => {
                let size_kb = std::fs::metadata(&dest)
                    .map(|m| m.len() as f64 / 1024.0)
                    .unwrap_or(0.0);

                let mut metadata = HashMap::new();
                metadata.insert(
                    "screenshot_path".into(),
                    serde_json::json!(dest.to_string_lossy()),
                );
                metadata.insert("url".into(), serde_json::json!(url));
                metadata.insert(
                    "viewport".into(),
                    serde_json::json!(format!("{viewport_width}x{viewport_height}")),
                );
                metadata.insert(
                    "screenshot_size_kb".into(),
                    serde_json::json!(format!("{size_kb:.1}")),
                );

                return ToolResult::ok_with_metadata(
                    format!(
                        "Screenshot saved: {}\nURL: {url}\nViewport: {viewport_width}x{viewport_height}\nSize: {size_kb:.1} KB",
                        dest.display()
                    ),
                    metadata,
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!(
                    "Headless Chrome failed (status {}): {stderr}",
                    output.status
                );
                // Fall through to HTTP fallback
            }
            Err(e) => {
                tracing::warn!("Failed to launch headless Chrome: {e}");
                // Fall through to HTTP fallback
            }
        }
    }

    // Fallback: save page as HTML
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
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
        Err(e) => return ToolResult::fail(format!("Failed to fetch page: {e}")),
    };

    let body = match response.text().await {
        Ok(t) => t,
        Err(e) => return ToolResult::fail(format!("Failed to read page: {e}")),
    };

    // Save as HTML instead of PNG
    let html_dest = dest.with_extension("html");
    match std::fs::write(&html_dest, &body) {
        Ok(_) => {
            let mut metadata = HashMap::new();
            metadata.insert(
                "screenshot_path".into(),
                serde_json::json!(html_dest.to_string_lossy()),
            );
            metadata.insert("url".into(), serde_json::json!(url));
            metadata.insert("format".into(), serde_json::json!("html"));
            metadata.insert(
                "note".into(),
                serde_json::json!(
                    "Headless Chrome not available. Saved as HTML. \
                     Install Chrome/Chromium for PNG screenshots."
                ),
            );

            ToolResult::ok_with_metadata(
                format!(
                    "Page saved as HTML: {}\nURL: {url}\n\
                     Note: Install Chrome/Chromium for PNG screenshot support.",
                    html_dest.display()
                ),
                metadata,
            )
        }
        Err(e) => ToolResult::fail(format!("Failed to save page: {e}")),
    }
}

/// List recent screenshots.
fn list_screenshots() -> ToolResult {
    let dir = screenshot_dir();
    if !dir.exists() {
        return ToolResult::ok("No screenshots found.");
    }

    let mut entries: Vec<(PathBuf, std::fs::Metadata)> = Vec::new();

    if let Ok(read_dir) = std::fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension()
                && (ext == "png" || ext == "html")
                && let Ok(meta) = entry.metadata()
            {
                entries.push((path, meta));
            }
        }
    }

    // Sort by modification time, newest first
    entries.sort_by(|a, b| {
        b.1.modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(&a.1.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH))
    });

    // Show at most 10
    entries.truncate(10);

    if entries.is_empty() {
        return ToolResult::ok("No screenshots found.");
    }

    let mut output = format!("Screenshots ({}, showing up to 10):\n\n", entries.len());
    for (path, meta) in &entries {
        let size_kb = meta.len() as f64 / 1024.0;
        output.push_str(&format!("  {} ({:.1} KB)\n", path.display(), size_kb));
    }

    let mut metadata = HashMap::new();
    metadata.insert("count".into(), serde_json::json!(entries.len()));
    metadata.insert("directory".into(), serde_json::json!(dir.to_string_lossy()));

    ToolResult::ok_with_metadata(output, metadata)
}

/// Clear old screenshots, keeping the most recent ones.
fn clear_screenshots(keep_recent: usize) -> ToolResult {
    let dir = screenshot_dir();
    if !dir.exists() {
        return ToolResult::ok("No screenshots directory found.");
    }

    let mut entries: Vec<PathBuf> = Vec::new();

    if let Ok(read_dir) = std::fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension()
                && (ext == "png" || ext == "html")
            {
                entries.push(path);
            }
        }
    }

    // Sort by modification time, newest first
    entries.sort_by(|a, b| {
        let a_time = a
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let b_time = b
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        b_time.cmp(&a_time)
    });

    let to_delete = if entries.len() > keep_recent {
        &entries[keep_recent..]
    } else {
        &[]
    };

    let mut deleted = 0;
    for path in to_delete {
        if std::fs::remove_file(path).is_ok() {
            deleted += 1;
        }
    }

    let kept = entries.len().saturating_sub(deleted);

    let mut metadata = HashMap::new();
    metadata.insert("deleted_count".into(), serde_json::json!(deleted));
    metadata.insert("kept_count".into(), serde_json::json!(kept));

    ToolResult::ok_with_metadata(
        format!("Cleared {deleted} screenshots, kept {kept}."),
        metadata,
    )
}

#[cfg(test)]
#[path = "web_screenshot_tests.rs"]
mod tests;
