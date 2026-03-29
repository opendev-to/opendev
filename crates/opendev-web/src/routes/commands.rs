//! Commands API routes.
//!
//! Exposes the list of available slash commands to the web UI,
//! matching the Python `opendev.web.routes.commands` module.

use axum::routing::get;
use axum::{Json, Router};

use crate::state::AppState;

/// Build the commands router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/commands", get(list_commands))
        .route("/api/commands/help", get(get_help))
}

/// Get the static list of available commands grouped by category.
fn command_list() -> serde_json::Value {
    serde_json::json!([
        {
            "category": "Operations",
            "commands": [
                {
                    "name": "/mode",
                    "args": "<name>",
                    "description": "Switch mode: normal or plan"
                },
                {
                    "name": "/init",
                    "args": "[path]",
                    "description":
                        "Analyze codebase and generate AGENTS.md with repository guidelines"
                }
            ]
        },
        {
            "category": "Session Management",
            "commands": [
                {
                    "name": "/clear",
                    "args": "",
                    "description": "Clear current session context"
                }
            ]
        },
        {
            "category": "Configuration",
            "commands": [
                {
                    "name": "/models",
                    "args": "",
                    "description":
                        "Interactive model/provider selector (use \u{2191}/\u{2193} arrows to choose)"
                }
            ]
        },
        {
            "category": "MCP (Model Context Protocol)",
            "commands": [
                {
                    "name": "/mcp list",
                    "args": "",
                    "description": "List configured MCP servers"
                },
                {
                    "name": "/mcp connect",
                    "args": "<name>",
                    "description": "Connect to an MCP server"
                },
                {
                    "name": "/mcp disconnect",
                    "args": "<name>",
                    "description": "Disconnect from a server"
                },
                {
                    "name": "/mcp tools",
                    "args": "[<name>]",
                    "description": "Show available tools from server(s)"
                },
                {
                    "name": "/mcp test",
                    "args": "<name>",
                    "description": "Test connection to a server"
                }
            ]
        },
        {
            "category": "General",
            "commands": [
                {
                    "name": "/help",
                    "args": "",
                    "description": "Show help message"
                },
                {
                    "name": "/exit",
                    "args": "",
                    "description": "Exit OpenDev"
                }
            ]
        }
    ])
}

/// List available commands.
async fn list_commands() -> Json<serde_json::Value> {
    Json(command_list())
}

/// Get help text with commands.
async fn get_help() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "title": "Available Commands",
        "commands": command_list(),
        "note": "Type commands in the chat input to execute them",
    }))
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
