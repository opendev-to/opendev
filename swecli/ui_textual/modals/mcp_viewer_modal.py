"""MCP Server Viewer Modal - Interactive MCP server inspection."""

from typing import Dict, List, Any
from prompt_toolkit.layout import (
    Layout, HSplit, Window, FormattedTextControl, Dimension
)
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.styles import Style
from prompt_toolkit.formatted_text import FormattedText

from swecli.ui_textual.modals.base import BaseModal


class MCPViewerModal(BaseModal):
    """Interactive modal for viewing MCP server details and tools."""

    def __init__(
        self,
        server_name: str,
        server_config: Dict[str, Any],
        is_connected: bool,
        tools: List[Dict[str, str]],
        capabilities: List[str],
        config_location: str,
        mcp_manager: Any = None,
    ):
        """Initialize MCP viewer modal.

        Args:
            server_name: Name of the MCP server
            server_config: Server configuration dict
            is_connected: Whether server is currently connected
            tools: List of available tools
            capabilities: Server capabilities (tools, resources, prompts)
            config_location: Path to config file
            mcp_manager: MCP manager instance for actions
        """
        super().__init__()
        self.server_name = server_name
        self.server_config = server_config
        self.is_connected = is_connected
        self.tools = tools
        self.capabilities = capabilities
        self.config_location = config_location
        self.mcp_manager = mcp_manager

        # View state
        self.view_mode = "server"  # "server" or "tools"
        self.selected_option = 0  # Selected menu option
        self.tools_scroll_offset = 0  # Scroll position in tools list
        self.tools_per_page = 20  # Number of tools to show per page

    def create_layout(self) -> Layout:
        """Create the modal layout."""
        return Layout(
            HSplit([
                Window(
                    content=FormattedTextControl(self._get_content),
                    height=Dimension(min=10, max=40),
                    wrap_lines=True,
                ),
            ])
        )

    def _get_content(self) -> FormattedText:
        """Get formatted content based on current view mode."""
        if self.view_mode == "server":
            return self._render_server_view()
        else:
            return self._render_tools_view()

    def _render_server_view(self) -> FormattedText:
        """Render the server details view."""
        lines = []

        # Title
        lines.append(("class:title", f"│ {self.server_name}"))
        lines.append(("", "\n"))
        lines.append(("", "│\n"))

        # Status
        status_text = "✔ connected" if self.is_connected else "✗ disconnected"
        status_class = "class:success" if self.is_connected else "class:error"
        lines.append(("", "│ Status: "))
        lines.append((status_class, status_text))
        lines.append(("", "\n"))

        # Command
        command = self.server_config.get("command", "unknown")
        lines.append(("", f"│ Command: {command}\n"))

        # Args
        args = self.server_config.get("args", [])
        if args:
            args_str = " ".join(args)
            if len(args_str) > 80:
                args_str = args_str[:77] + "..."
            lines.append(("", f"│ Args: {args_str}\n"))

        # Config location
        config_str = self.config_location
        if len(config_str) > 100:
            config_str = config_str[:97] + "..."
        lines.append(("class:dim", f"│ Config location: {config_str}\n"))

        # Capabilities
        if self.capabilities:
            cap_str = " · ".join(self.capabilities)
            lines.append(("", f"│ Capabilities: {cap_str}\n"))

        # Tools count
        tools_count = len(self.tools)
        lines.append(("", f"│ Tools: {tools_count} tool{'s' if tools_count != 1 else ''}\n"))

        lines.append(("", "│\n"))

        # Menu options
        options = [
            "View tools",
            "Reconnect" if self.is_connected else "Connect",
            "Disable" if self.server_config.get("enabled", True) else "Enable",
        ]

        for i, option in enumerate(options):
            if i == self.selected_option:
                lines.append(("class:selected", f"│ ❯ {i + 1}. {option}\n"))
            else:
                lines.append(("", f"│   {i + 1}. {option}\n"))

        return FormattedText(lines)

    def _render_tools_view(self) -> FormattedText:
        """Render the tools list view."""
        lines = []

        # Title
        tools_count = len(self.tools)
        lines.append(("class:title", f"│ Tools for {self.server_name} ({tools_count} tools)"))
        lines.append(("", "\n"))
        lines.append(("", "│\n"))

        # Show tools with pagination
        end_idx = min(self.tools_scroll_offset + self.tools_per_page, len(self.tools))
        visible_tools = self.tools[self.tools_scroll_offset:end_idx]

        for i, tool in enumerate(visible_tools):
            actual_idx = self.tools_scroll_offset + i
            tool_name = tool.get("name", "unknown")
            tool_desc = tool.get("description", "")

            # Highlight selected tool
            if actual_idx == self.selected_option:
                lines.append(("class:selected", f"│ ❯ {actual_idx + 1}.  {tool_name}\n"))
            else:
                lines.append(("", f"│   {actual_idx + 1}.  {tool_name}\n"))

            # Show description (truncated if too long)
            if tool_desc:
                if len(tool_desc) > 90:
                    tool_desc = tool_desc[:87] + "..."
                lines.append(("class:dim", f"│       {tool_desc}\n"))

        # Show scroll indicator
        if self.tools_scroll_offset > 0:
            lines.append(("class:dim", f"│ ↑ {self.tools_scroll_offset} more above\n"))

        remaining = len(self.tools) - end_idx
        if remaining > 0:
            lines.append(("class:dim", f"│ ↓ {remaining} more below\n"))

        lines.append(("", "│\n"))
        lines.append(("class:dim", "│ Press ESC to go back\n"))

        return FormattedText(lines)

    def create_style(self) -> Style:
        """Create the modal style."""
        return Style.from_dict({
            "title": "bold cyan",
            "success": "green",
            "error": "red",
            "dim": "dim",
            "selected": "reverse cyan",
        })

    def create_key_bindings(self) -> KeyBindings:
        """Create key bindings for navigation."""
        kb = KeyBindings()

        @kb.add("escape")
        def _(event):
            """Handle ESC - go back or close."""
            if self.view_mode == "tools":
                # Go back to server view
                self.view_mode = "server"
                self.selected_option = 0
            else:
                # Close modal
                self.result = "close"
                event.app.exit()

        @kb.add("q")
        def _(event):
            """Handle Q - quit."""
            self.result = "close"
            event.app.exit()

        @kb.add("up")
        @kb.add("k")
        def _(event):
            """Navigate up."""
            if self.view_mode == "server":
                if self.selected_option > 0:
                    self.selected_option -= 1
            else:
                # Tools view - scroll up
                if self.selected_option > 0:
                    self.selected_option -= 1
                    # Adjust scroll offset if needed
                    if self.selected_option < self.tools_scroll_offset:
                        self.tools_scroll_offset = max(0, self.tools_scroll_offset - 1)

        @kb.add("down")
        @kb.add("j")
        def _(event):
            """Navigate down."""
            if self.view_mode == "server":
                max_options = 2  # View tools, Reconnect, Disable
                if self.selected_option < max_options:
                    self.selected_option += 1
            else:
                # Tools view - scroll down
                if self.selected_option < len(self.tools) - 1:
                    self.selected_option += 1
                    # Adjust scroll offset if needed
                    if self.selected_option >= self.tools_scroll_offset + self.tools_per_page:
                        self.tools_scroll_offset = min(
                            len(self.tools) - self.tools_per_page,
                            self.tools_scroll_offset + 1
                        )

        @kb.add("enter")
        @kb.add("1")
        @kb.add("2")
        @kb.add("3")
        def _(event):
            """Handle Enter or number keys - select option."""
            if self.view_mode == "server":
                # Handle menu selection
                key = event.key_sequence[0].key
                if key in ("1", "2", "3"):
                    option_idx = int(key) - 1
                elif key == "enter":
                    option_idx = self.selected_option
                else:
                    return

                if option_idx == 0:
                    # View tools
                    self.view_mode = "tools"
                    self.selected_option = 0
                    self.tools_scroll_offset = 0
                elif option_idx == 1:
                    # Reconnect/Connect
                    self.result = "reconnect" if self.is_connected else "connect"
                    event.app.exit()
                elif option_idx == 2:
                    # Disable/Enable
                    self.result = "disable" if self.server_config.get("enabled", True) else "enable"
                    event.app.exit()

        @kb.add("pageup")
        def _(event):
            """Page up in tools list."""
            if self.view_mode == "tools":
                self.selected_option = max(0, self.selected_option - self.tools_per_page)
                self.tools_scroll_offset = max(0, self.tools_scroll_offset - self.tools_per_page)

        @kb.add("pagedown")
        def _(event):
            """Page down in tools list."""
            if self.view_mode == "tools":
                self.selected_option = min(
                    len(self.tools) - 1,
                    self.selected_option + self.tools_per_page
                )
                self.tools_scroll_offset = min(
                    len(self.tools) - self.tools_per_page,
                    self.tools_scroll_offset + self.tools_per_page
                )

        return kb

    def get_default_result(self) -> str:
        """Get default result if modal is closed."""
        return "close"


def show_mcp_server(
    server_name: str,
    server_config: Dict[str, Any],
    is_connected: bool,
    tools: List[Dict[str, str]],
    capabilities: List[str],
    config_location: str,
    mcp_manager: Any = None,
) -> str:
    """Show MCP server details in an interactive modal.

    Args:
        server_name: Name of the MCP server
        server_config: Server configuration
        is_connected: Whether server is connected
        tools: List of available tools
        capabilities: Server capabilities
        config_location: Path to config file
        mcp_manager: MCP manager instance

    Returns:
        Action result: "close", "reconnect", "connect", "disable", or "enable"
    """
    modal = MCPViewerModal(
        server_name=server_name,
        server_config=server_config,
        is_connected=is_connected,
        tools=tools,
        capabilities=capabilities,
        config_location=config_location,
        mcp_manager=mcp_manager,
    )
    return modal.show()
