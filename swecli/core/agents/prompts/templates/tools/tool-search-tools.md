<!--
name: 'Tool Description: search_tools'
description: Search for available MCP tools
version: 2.0.0
-->

Search for available MCP tools from connected servers. Use this to discover tools before using them.

## Usage notes

- MCP tool schemas are NOT loaded by default to save context tokens — you MUST search for them first before attempting to use any MCP tool
- Returns matching tool schemas with their names, descriptions, and parameter definitions
- Search by keyword or tool name (e.g., "database", "slack", "github")
- If you're unsure which MCP tools are available, search with a broad term related to what you need
- After finding a tool, you can use it directly with its full name as returned by the search
