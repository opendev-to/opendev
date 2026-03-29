//! Custom tool loaded from `.opendev/tools/` directory.
//!
//! Users can define custom tools by placing a JSON manifest file alongside
//! an executable script in `.opendev/tools/` (or `.opencode/tool/`).
//!
//! ## Manifest format (`<name>.tool.json`)
//!
//! ```json
//! {
//!   "name": "github_triage",
//!   "description": "Assign and label GitHub issues",
//!   "command": "./github-triage.sh",
//!   "parameters": {
//!     "type": "object",
//!     "properties": {
//!       "issue": { "type": "string", "description": "Issue number" }
//!     },
//!     "required": ["issue"]
//!   },
//!   "timeout_secs": 30
//! }
//! ```
//!
//! The tool receives arguments as JSON on stdin and should write its
//! result to stdout. Exit code 0 = success, non-zero = failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, warn};

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// JSON manifest describing a custom tool.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomToolManifest {
    /// Tool name (used for dispatch). Must be unique.
    pub name: String,
    /// Human-readable description shown to the LLM.
    pub description: String,
    /// Command to execute (relative to the manifest directory, or absolute).
    pub command: String,
    /// JSON Schema for tool parameters.
    #[serde(default = "default_params_schema")]
    pub parameters: serde_json::Value,
    /// Optional timeout in seconds (default: 30).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_params_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "input": {
                "type": "string",
                "description": "Input to the tool"
            }
        }
    })
}

fn default_timeout() -> u64 {
    30
}

/// A tool backed by an external script/executable.
#[derive(Debug)]
pub struct CustomTool {
    manifest: CustomToolManifest,
    /// Directory containing the manifest (for resolving relative command paths).
    base_dir: PathBuf,
}

impl CustomTool {
    /// Create a custom tool from a manifest and its containing directory.
    pub fn new(manifest: CustomToolManifest, base_dir: PathBuf) -> Self {
        Self { manifest, base_dir }
    }

    /// Resolve the command path (relative to base_dir if not absolute).
    fn resolve_command(&self) -> PathBuf {
        let cmd = Path::new(&self.manifest.command);
        if cmd.is_absolute() {
            cmd.to_path_buf()
        } else {
            self.base_dir.join(cmd)
        }
    }
}

#[async_trait]
impl BaseTool for CustomTool {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn description(&self) -> &str {
        &self.manifest.description
    }

    fn parameter_schema(&self) -> serde_json::Value {
        self.manifest.parameters.clone()
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let cmd_path = self.resolve_command();

        if !cmd_path.exists() {
            return ToolResult::fail(format!(
                "Custom tool command not found: {}",
                cmd_path.display()
            ));
        }

        // Serialize args as JSON for stdin.
        let input_json = match serde_json::to_string(&args) {
            Ok(j) => j,
            Err(e) => return ToolResult::fail(format!("Failed to serialize args: {e}")),
        };

        // Execute the command.
        let timeout = std::time::Duration::from_secs(self.manifest.timeout_secs);
        let result = tokio::time::timeout(timeout, async {
            let mut child = match tokio::process::Command::new(cmd_path.as_os_str())
                .current_dir(&ctx.working_dir)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => return Err(e),
            };

            // Write input to stdin.
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(input_json.as_bytes()).await;
                drop(stdin);
            }

            child.wait_with_output().await
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    debug!(
                        tool = self.manifest.name,
                        exit_code = 0,
                        "Custom tool executed successfully"
                    );
                    if stdout.is_empty() {
                        ToolResult::ok("(no output)")
                    } else {
                        ToolResult::ok(stdout)
                    }
                } else {
                    let code = output.status.code().unwrap_or(-1);
                    let error_msg = if stderr.is_empty() {
                        format!("Custom tool exited with code {code}")
                    } else {
                        format!("Exit code {code}: {stderr}")
                    };
                    ToolResult::fail(error_msg)
                }
            }
            Ok(Err(e)) => ToolResult::fail(format!("Failed to execute custom tool: {e}")),
            Err(_) => ToolResult::fail(format!(
                "Custom tool timed out after {}s",
                self.manifest.timeout_secs
            )),
        }
    }
}

/// Discover custom tools from standard directories.
///
/// Scans these directories for `*.tool.json` manifest files:
/// - `<working_dir>/.opendev/tools/`
/// - `<working_dir>/.opencode/tool/`
///
/// Returns a list of `(manifest, base_dir)` tuples for each valid tool found.
pub fn discover_custom_tools(working_dir: &Path) -> Vec<CustomTool> {
    let search_dirs = [
        working_dir.join(".opendev").join("tools"),
        working_dir.join(".opencode").join("tool"),
    ];

    let mut tools = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    for dir in &search_dirs {
        if !dir.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(dir = %dir.display(), error = %e, "Failed to read custom tools directory");
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Only process *.tool.json manifests.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".tool.json") {
                continue;
            }

            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<CustomToolManifest>(&content) {
                    Ok(manifest) => {
                        if seen_names.contains(&manifest.name) {
                            warn!(
                                name = manifest.name,
                                path = %path.display(),
                                "Duplicate custom tool name, skipping"
                            );
                            continue;
                        }
                        debug!(
                            name = manifest.name,
                            path = %path.display(),
                            "Discovered custom tool"
                        );
                        seen_names.insert(manifest.name.clone());
                        tools.push(CustomTool::new(manifest, dir.clone()));
                    }
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to parse custom tool manifest"
                        );
                    }
                },
                Err(e) => {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to read custom tool manifest"
                    );
                }
            }
        }
    }

    tools
}

#[cfg(test)]
#[path = "custom_tool_tests.rs"]
mod tests;
