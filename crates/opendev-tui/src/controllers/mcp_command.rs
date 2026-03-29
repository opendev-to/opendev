//! MCP command controller for managing MCP servers via slash commands.
//!
//! Mirrors the `/mcp list|add|remove|enable|disable` commands from
//! `opendev/ui_textual/controllers/mcp_command_controller.py`.

/// Information about an MCP server.
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    /// Server name/alias.
    pub name: String,
    /// Command to launch the server.
    pub command: String,
    /// Whether the server is currently enabled.
    pub enabled: bool,
}

/// Controller for handling MCP server management commands.
pub struct McpCommandController {
    servers: Vec<McpServerInfo>,
}

impl McpCommandController {
    /// Create a new controller with the given server list.
    pub fn new(servers: Vec<McpServerInfo>) -> Self {
        Self { servers }
    }

    /// Get the current server list.
    pub fn servers(&self) -> &[McpServerInfo] {
        &self.servers
    }

    /// Handle an MCP subcommand string (e.g. "list", "add myserver uvx cmd").
    ///
    /// Returns a human-readable response string.
    pub fn handle_command(&mut self, args: &str) -> String {
        let parts: Vec<&str> = args.trim().splitn(2, char::is_whitespace).collect();
        let subcommand = parts.first().copied().unwrap_or("");
        let rest = parts.get(1).copied().unwrap_or("").trim();

        match subcommand {
            "list" | "" => self.list_servers(),
            "add" => self.add_server(rest),
            "remove" => self.remove_server(rest),
            "enable" => self.enable_server(rest),
            "disable" => self.disable_server(rest),
            other => format!(
                "Unknown MCP subcommand: '{}'. Use list/add/remove/enable/disable.",
                other
            ),
        }
    }

    fn list_servers(&self) -> String {
        if self.servers.is_empty() {
            return "No MCP servers configured.".into();
        }
        let mut lines = vec!["MCP Servers:".to_string()];
        for (i, server) in self.servers.iter().enumerate() {
            let status = if server.enabled {
                "enabled"
            } else {
                "disabled"
            };
            lines.push(format!(
                "  {}. {} ({}) [{}]",
                i + 1,
                server.name,
                server.command,
                status,
            ));
        }
        lines.join("\n")
    }

    fn add_server(&mut self, rest: &str) -> String {
        let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
        let name = parts.first().copied().unwrap_or("").trim();
        let command = parts.get(1).copied().unwrap_or("").trim();

        if name.is_empty() || command.is_empty() {
            return "Usage: mcp add <name> <command>".into();
        }

        if self.servers.iter().any(|s| s.name == name) {
            return format!("Server '{}' already exists.", name);
        }

        self.servers.push(McpServerInfo {
            name: name.to_string(),
            command: command.to_string(),
            enabled: true,
        });
        format!("Added MCP server '{}'.", name)
    }

    fn remove_server(&mut self, name: &str) -> String {
        let name = name.trim();
        if name.is_empty() {
            return "Usage: mcp remove <name>".into();
        }
        let before = self.servers.len();
        self.servers.retain(|s| s.name != name);
        if self.servers.len() < before {
            format!("Removed MCP server '{}'.", name)
        } else {
            format!("Server '{}' not found.", name)
        }
    }

    fn enable_server(&mut self, name: &str) -> String {
        let name = name.trim();
        if let Some(server) = self.servers.iter_mut().find(|s| s.name == name) {
            server.enabled = true;
            format!("Enabled MCP server '{}'.", name)
        } else {
            format!("Server '{}' not found.", name)
        }
    }

    fn disable_server(&mut self, name: &str) -> String {
        let name = name.trim();
        if let Some(server) = self.servers.iter_mut().find(|s| s.name == name) {
            server.enabled = false;
            format!("Disabled MCP server '{}'.", name)
        } else {
            format!("Server '{}' not found.", name)
        }
    }
}

#[cfg(test)]
#[path = "mcp_command_tests.rs"]
mod tests;
