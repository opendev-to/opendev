"""MCP server viewer message component for chat interface."""

from typing import List, Dict, Any


def create_mcp_viewer_message(
    server_name: str,
    server_config: Dict[str, Any],
    is_connected: bool,
    tools: List[Dict[str, str]],
    capabilities: List[str],
    config_location: str,
    selected_index: int = 0,
    view_mode: str = "server",
    tools_scroll_offset: int = 0,
    tools_per_page: int = 20,
    selected_tool: Dict[str, str] = None,
) -> str:
    """Create formatted MCP viewer message.

    Args:
        server_name: Name of the MCP server
        server_config: Server configuration dict
        is_connected: Whether server is currently connected
        tools: List of available tools
        capabilities: Server capabilities
        config_location: Path to config file
        selected_index: Currently selected option
        view_mode: "server", "tools", or "tool_detail"
        tools_scroll_offset: Scroll position in tools list
        tools_per_page: Number of tools to show per page
        selected_tool: Currently selected tool for detail view

    Returns:
        Formatted message string
    """
    lines = []

    if view_mode == "server":
        # Server details view
        lines.append(f"┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓")
        lines.append(f"┃ MCP Server: {server_name:<45}┃")
        lines.append(f"┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛")
        lines.append("")

        # Status
        status_text = "✔ connected" if is_connected else "✗ disconnected"
        lines.append(f"  Status: {status_text}")

        # Command
        command = server_config.get("command", "unknown")
        lines.append(f"  Command: {command}")

        # Args
        args = server_config.get("args", [])
        if args:
            args_str = " ".join(args)
            if len(args_str) > 70:
                args_str = args_str[:67] + "..."
            lines.append(f"  Args: {args_str}")

        # Config location
        if len(config_location) > 70:
            config_str = config_location[:67] + "..."
        else:
            config_str = config_location
        lines.append(f"  Config: {config_str}")

        # Capabilities
        if capabilities:
            cap_str = " · ".join(capabilities)
            lines.append(f"  Capabilities: {cap_str}")

        # Tools count
        tools_count = len(tools)
        lines.append(f"  Tools: {tools_count} tool{'s' if tools_count != 1 else ''}")

        lines.append("")
        lines.append("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓")
        lines.append("┃ Options                                                 ┃")
        lines.append("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛")

        # Menu options
        options = [
            "View tools",
            "Reconnect" if is_connected else "Connect",
            "Disable" if server_config.get("enabled", True) else "Enable",
        ]

        for i, option in enumerate(options):
            cursor = "❯" if i == selected_index else " "
            lines.append(f"  {cursor} {i + 1}. {option}")

        lines.append("")
        lines.append("  Use ↑↓ to navigate, Enter to select, Esc to cancel")

    elif view_mode == "tools":
        # Tools list view - only show names
        lines.append(f"┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓")
        lines.append(f"┃ Tools for {server_name} ({len(tools)} tools){' ' * (38 - len(server_name))}┃")
        lines.append(f"┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛")
        lines.append("")

        # Show tools with pagination - only names
        end_idx = min(tools_scroll_offset + tools_per_page, len(tools))
        visible_tools = tools[tools_scroll_offset:end_idx]

        for i, tool in enumerate(visible_tools):
            actual_idx = tools_scroll_offset + i
            tool_name = tool.get("name", "unknown")

            # Highlight selected tool
            cursor = "❯" if actual_idx == selected_index else " "
            lines.append(f"  {cursor} {actual_idx + 1}. {tool_name}")

        # Show scroll indicators
        if tools_scroll_offset > 0:
            lines.append("")
            lines.append(f"  ↑ {tools_scroll_offset} more above")

        remaining = len(tools) - end_idx
        if remaining > 0:
            if tools_scroll_offset == 0:
                lines.append("")
            lines.append(f"  ↓ {remaining} more below")

        lines.append("")
        lines.append("  Use ↑↓ to navigate, Enter to view details, Esc to go back")

    else:
        # Tool detail view
        if selected_tool:
            tool_name = selected_tool.get("name", "unknown")
            tool_desc = selected_tool.get("description", "No description available")

            # Generate full name with mcp__ prefix
            full_name = f"mcp__{server_name}__{tool_name}"

            lines.append(f"┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓")
            lines.append(f"┃ {tool_name} ({server_name}){' ' * (56 - len(tool_name) - len(server_name))}┃")
            lines.append(f"┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛")
            lines.append("")
            lines.append(f"  Tool name: {tool_name}")
            lines.append(f"  Full name: {full_name}")
            lines.append("")
            lines.append("  Description:")

            # Wrap description to fit within width
            desc_lines = []
            words = tool_desc.split()
            current_line = ""
            for word in words:
                if len(current_line) + len(word) + 1 <= 58:  # Leave room for "  "
                    current_line += (" " if current_line else "") + word
                else:
                    if current_line:
                        desc_lines.append(current_line)
                    current_line = word
            if current_line:
                desc_lines.append(current_line)

            for desc_line in desc_lines:
                lines.append(f"  {desc_line}")

            lines.append("")
            lines.append("  Press Esc to go back to tools list")

    return "\n".join(lines)
