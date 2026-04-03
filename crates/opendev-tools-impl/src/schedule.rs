//! Schedule tool — create and manage scheduled tasks persisted to disk.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use opendev_tools_core::{BaseTool, ToolContext, ToolDisplayMeta, ToolResult};

/// Tool for creating and listing scheduled tasks.
#[derive(Debug)]
pub struct ScheduleTool;

impl ScheduleTool {
    fn schedules_path() -> Option<PathBuf> {
        Some(
            opendev_config::Paths::default()
                .data_dir()
                .join("schedules.json"),
        )
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ScheduleEntry {
    id: String,
    description: String,
    command: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    run_at: Option<DateTime<Utc>>,
    interval_secs: Option<u64>,
    enabled: bool,
}

#[async_trait::async_trait]
impl BaseTool for ScheduleTool {
    fn name(&self) -> &str {
        "CronCreate"
    }

    fn description(&self) -> &str {
        "Create, list, or remove scheduled tasks. Tasks are persisted to ~/.opendev/schedules.json."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "remove", "enable", "disable"],
                    "description": "Action to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Schedule ID (for remove/enable/disable)"
                },
                "description": {
                    "type": "string",
                    "description": "Human-readable description (for create)"
                },
                "command": {
                    "type": "string",
                    "description": "Command to run (for create)"
                },
                "interval_secs": {
                    "type": "integer",
                    "description": "Repeat interval in seconds (for create)"
                },
                "delay_secs": {
                    "type": "integer",
                    "description": "Delay before first run in seconds (for create)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        let path = match Self::schedules_path() {
            Some(p) => p,
            None => return ToolResult::fail("Cannot determine home directory"),
        };

        match action {
            "create" => {
                let desc = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled schedule");
                let command = match args.get("command").and_then(|v| v.as_str()) {
                    Some(c) => c,
                    None => return ToolResult::fail("command is required for create"),
                };
                let interval = args.get("interval_secs").and_then(|v| v.as_u64());
                let delay = args.get("delay_secs").and_then(|v| v.as_u64()).unwrap_or(0);

                let run_at = if delay > 0 {
                    Some(Utc::now() + chrono::Duration::seconds(delay as i64))
                } else {
                    None
                };

                let entry = ScheduleEntry {
                    id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
                    description: desc.to_string(),
                    command: command.to_string(),
                    created_at: Utc::now(),
                    run_at,
                    interval_secs: interval,
                    enabled: true,
                };

                let mut schedules = load_schedules(&path);
                let id = entry.id.clone();
                schedules.push(entry);
                if let Err(e) = save_schedules(&path, &schedules) {
                    return ToolResult::fail(format!("Failed to save: {e}"));
                }

                ToolResult::ok(format!("Created schedule '{id}': {desc}"))
            }
            "list" => {
                let schedules = load_schedules(&path);
                if schedules.is_empty() {
                    return ToolResult::ok("No scheduled tasks".to_string());
                }

                let mut output = format!("Scheduled tasks ({}):\n", schedules.len());
                for s in &schedules {
                    let status = if s.enabled { "enabled" } else { "disabled" };
                    output.push_str(&format!(
                        "  [{}] {} — '{}' ({})\n",
                        s.id, s.description, s.command, status
                    ));
                }
                ToolResult::ok(output)
            }
            "remove" => {
                let id = match args.get("id").and_then(|v| v.as_str()) {
                    Some(i) => i,
                    None => return ToolResult::fail("id is required for remove"),
                };
                let mut schedules = load_schedules(&path);
                let before = schedules.len();
                schedules.retain(|s| s.id != id);
                if schedules.len() == before {
                    return ToolResult::fail(format!("Schedule '{id}' not found"));
                }
                if let Err(e) = save_schedules(&path, &schedules) {
                    return ToolResult::fail(format!("Failed to save: {e}"));
                }
                ToolResult::ok(format!("Removed schedule '{id}'"))
            }
            "enable" | "disable" => {
                let id = match args.get("id").and_then(|v| v.as_str()) {
                    Some(i) => i,
                    None => return ToolResult::fail("id is required"),
                };
                let mut schedules = load_schedules(&path);
                let enabled = action == "enable";
                let mut found = false;
                for s in &mut schedules {
                    if s.id == id {
                        s.enabled = enabled;
                        found = true;
                        break;
                    }
                }
                if !found {
                    return ToolResult::fail(format!("Schedule '{id}' not found"));
                }
                if let Err(e) = save_schedules(&path, &schedules) {
                    return ToolResult::fail(format!("Failed to save: {e}"));
                }
                ToolResult::ok(format!("Schedule '{id}' {action}d"))
            }
            _ => ToolResult::fail(format!(
                "Unknown action: {action}. Available: create, list, remove, enable, disable"
            )),
        }
    }

    fn display_meta(&self) -> Option<ToolDisplayMeta> {
        Some(ToolDisplayMeta {
            verb: "Schedule",
            label: "task",
            category: "Other",
            primary_arg_keys: &["action", "description", "command"],
        })
    }
}

fn load_schedules(path: &std::path::Path) -> Vec<ScheduleEntry> {
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_schedules(path: &std::path::Path, schedules: &[ScheduleEntry]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create directory: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(schedules).map_err(|e| format!("Serialization error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Write error: {e}"))
}

#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;
