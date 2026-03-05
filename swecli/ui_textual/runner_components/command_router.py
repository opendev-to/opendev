"""Command router for TextualRunner.

This module handles the routing and execution of slash commands within the
Textual UI, including MCP commands and special tool modes.
"""

from __future__ import annotations

import shlex
from io import StringIO
from pathlib import Path
from typing import Any

from rich import box
from rich.console import Console as RichConsole
from rich.panel import Panel
from rich.table import Table
from rich.text import Text

from swecli.repl.repl import REPL
from swecli.ui_textual.style_tokens import ERROR, PRIMARY, SUBTLE, CYAN, GREEN_BRIGHT, BLUE_BRIGHT


class CommandRouter:
    """Routes and executes slash commands in the Textual UI."""

    def __init__(
        self,
        repl: REPL,
        working_dir: Path,
        callbacks: dict[str, Any],
    ) -> None:
        """Initialize the CommandRouter.

        Args:
            repl: The REPL instance.
            working_dir: Current working directory.
            callbacks: Dictionary of callback functions:
                - enqueue_console_text: Callable[[str], None]
                - start_mcp_connect_thread: Callable[[bool], None]
                - refresh_ui_config: Callable[[], None]
        """
        self._repl = repl
        self._working_dir = working_dir
        self._callbacks = callbacks
        self._app: Any | None = None

    def set_app(self, app: Any) -> None:
        """Set the Textual app instance."""
        self._app = app

    def route_command(self, command: str) -> bool:
        """Route and execute a command if it matches any handler.

        Returns:
            True if the command was handled, False otherwise.
        """
        stripped = command.strip()
        lowered = stripped.lower()

        # Background auto-connect trigger
        if lowered.startswith("/mcp autoconnect"):
            self._handle_mcp_autoconnect()
            return True

        # /mcp view command - use Textual modal
        if lowered.startswith("/mcp view "):
            self._handle_mcp_view_command(command)
            return True

        # /mcp connect - use Textual spinner
        if lowered.startswith("/mcp connect "):
            self._handle_mcp_connect_command(command)
            return True

        # /init command - needs ui_callback for proper display
        if lowered.startswith("/init"):
            self._handle_init_command(command)
            return True

        return False

    def run_generic_command(self, command: str) -> None:
        """Run a standard REPL command and capture output."""
        with self._repl.console.capture() as capture:
            self._repl._handle_command(command)
        output = capture.get()
        if output.strip():
            self._enqueue_console_text(output)
        
        refresh_cb = self._callbacks.get("refresh_ui_config")
        if refresh_cb:
            refresh_cb()

    def _enqueue_console_text(self, text: str) -> None:
        """Helper to call enqueue callback."""
        cb = self._callbacks.get("enqueue_console_text")
        if cb:
            cb(text)

    def _handle_mcp_autoconnect(self) -> None:
        """Handle /mcp autoconnect command."""
        # We need to know if connect is inflight...
        # Runner tracks inflight status. We might need a check callback?
        # For now, let's assume runner handles the inflight check or we pass 'force=True'
        # The runner.py logic checked _connect_inflight.
        # We should ideally move that state here or trust the callback.
        # Since running autoconnect manually is a user action, we can try to start it.
        start_cb = self._callbacks.get("start_mcp_connect_thread")
        if start_cb:
            self._enqueue_console_text(
                f"[{CYAN}]Starting MCP auto-connect in the background...[/{CYAN}]"
            )
            # force=True
            start_cb(True)

    def _handle_mcp_view_command(self, command: str) -> None:
        """Handle /mcp view command with Textual-native modal."""
        def _emit_error(message: str) -> None:
            string_io = StringIO()
            temp_console = RichConsole(file=string_io, force_terminal=True)
            temp_console.print(message)
            self._enqueue_console_text(string_io.getvalue())

        try:
            raw_parts = shlex.split(command)
        except ValueError:
            raw_parts = command.strip().split()

        if len(raw_parts) < 3:
            _emit_error("[red]Error: Server name required for /mcp view[/red]")
            return

        server_name = " ".join(raw_parts[2:]).strip()
        if not server_name:
            _emit_error("[red]Error: Server name required for /mcp view[/red]")
            return

        mcp_manager = getattr(self._repl, "mcp_manager", None)
        if mcp_manager is None:
            _emit_error("[red]Error: MCP manager is not available in this session[/red]")
            return

        try:
            servers = mcp_manager.list_servers()
        except Exception as exc:
            _emit_error(f"[red]Error: Unable to load MCP servers ({exc})[/red]")
            return

        if server_name not in servers:
            _emit_error(f"[red]Error: Server '{server_name}' not found in configuration[/red]")
            return

        server_config = servers[server_name]
        is_connected = mcp_manager.is_connected(server_name)
        tools = mcp_manager.get_server_tools(server_name) if is_connected else []

        # Build elegant panel content
        info_table = Table(show_header=False, box=None, padding=(0, 1))
        info_table.add_column("Property", style="cyan", no_wrap=True)
        info_table.add_column("Value")

        status_text = (
            Text("Connected", style=f"bold {GREEN_BRIGHT}")
            if is_connected
            else Text("Disconnected", style=SUBTLE)
        )
        info_table.add_row("Status", status_text)

        cmd_text = server_config.command or "unknown"
        info_table.add_row("Command", cmd_text)

        if server_config.args:
            args_text = " ".join(server_config.args)
            if len(args_text) > 80:
                args_text = args_text[:77] + "..."
            info_table.add_row("Args", args_text)

        transport_text = server_config.transport or "stdio"
        info_table.add_row("Transport", transport_text)

        from swecli.core.context_engineering.mcp.config import get_config_path, get_project_config_path

        config_location = ""
        try:
            project_config = get_project_config_path(getattr(mcp_manager, "working_dir", None))
        except Exception:
            project_config = None

        if project_config:
            config_location = f"{project_config} [project]"
        else:
            try:
                config_location = str(get_config_path())
            except Exception:
                config_location = "Unknown"

        info_table.add_row("Config", Text(config_location, style="dim"))

        capabilities: list[str] = []
        if is_connected and tools:
            capabilities.append("tools")
        if capabilities:
            info_table.add_row("Capabilities", " · ".join(capabilities))

        enabled_text = (
            Text("Yes", style=GREEN_BRIGHT)
            if server_config.enabled
            else Text("No", style=ERROR)
        )
        info_table.add_row("Enabled", enabled_text)

        auto_start_text = (
            Text("Yes", style=GREEN_BRIGHT)
            if server_config.auto_start
            else Text("No", style=SUBTLE)
        )
        info_table.add_row("Auto-start", auto_start_text)

        if server_config.env:
            env_lines = "\n".join(f"{key}={value}" for key, value in server_config.env.items())
            info_table.add_row("Environment", env_lines)

        if is_connected:
            info_table.add_row("Tools", f"{len(tools)} available")

        tools_content = None
        if is_connected and tools:
            tools_table = Table(show_header=True, box=box.SIMPLE, padding=(0, 1))
            tools_table.add_column("Tool Name", style=CYAN)
            tools_table.add_column("Description", style=PRIMARY)

            for tool in tools[:10]:
                tool_name = tool.get("name", "unknown")
                tool_desc = tool.get("description", "")
                if len(tool_desc) > 60:
                    tool_desc = tool_desc[:57] + "..."
                tools_table.add_row(tool_name, tool_desc)

            if len(tools) > 10:
                tools_table.add_row(f"... and {len(tools) - 10} more", "", style=SUBTLE)

            tools_content = tools_table

        title = f"MCP Server: {server_name}"
        main_panel = Panel(
            info_table,
            title=title,
            title_align="left",
            border_style=BLUE_BRIGHT,
            box=box.ROUNDED,
            padding=(1, 2),
        )

        self._enqueue_console_text("\n")

        string_io = StringIO()
        temp_console = RichConsole(file=string_io, force_terminal=True, width=100)
        temp_console.print(main_panel)

        if tools_content:
            temp_console.print("\n")
            tools_panel = Panel(
                tools_content,
                title="Available Tools",
                title_align="left",
                border_style=GREEN_BRIGHT,
                box=box.ROUNDED,
                padding=(1, 2),
            )
            temp_console.print(tools_panel)

        temp_console.print("\n[dim]Available actions:[/dim]")
        if is_connected:
            temp_console.print(f"  [cyan]/mcp disconnect {server_name}[/cyan] - Disconnect from server")
            temp_console.print(f"  [cyan]/mcp tools {server_name}[/cyan] - List all tools")
        else:
            temp_console.print(f"  [cyan]/mcp connect {server_name}[/cyan] - Connect to server")

        if server_config.enabled:
            temp_console.print(f"  [cyan]/mcp disable {server_name}[/cyan] - Disable auto-start")
        else:
            temp_console.print(f"  [cyan]/mcp enable {server_name}[/cyan] - Enable auto-start")

        output = string_io.getvalue()
        self._enqueue_console_text(output)

    def _handle_mcp_connect_command(self, command: str) -> None:
        """Handle /mcp connect with Textual spinner."""
        if not self._app:
            return
        from swecli.ui_textual.controllers.mcp_command_controller import MCPCommandController

        controller = MCPCommandController(self._app, self._repl)
        controller.handle_connect(command)

    def _handle_init_command(self, command: str) -> None:
        """Handle /init command with proper UI callback for collapsed agent display."""
        if not self._app:
            # Fallback to generic command if no app
            self.run_generic_command(command)
            return

        # Create UI callback for proper spinner/collapsed agent display
        from swecli.ui_textual.ui_callback import TextualUICallback

        conversation_widget = getattr(self._app, "conversation", None)
        if conversation_widget is None:
            self.run_generic_command(command)
            return

        ui_callback = TextualUICallback(conversation_widget, self._app, self._working_dir)

        # Set ui_callback on tool_commands so it uses proper display
        if hasattr(self._repl, "tool_commands"):
            self._repl.tool_commands.ui_callback = ui_callback

        # Run the command (which will now use ui_callback)
        try:
            self._repl.tool_commands.init_codebase(command)
        finally:
            # Clear ui_callback after command completes
            if hasattr(self._repl, "tool_commands"):
                self._repl.tool_commands.ui_callback = None

        # Refresh UI config after command
        refresh_cb = self._callbacks.get("refresh_ui_config")
        if refresh_cb:
            refresh_cb()
