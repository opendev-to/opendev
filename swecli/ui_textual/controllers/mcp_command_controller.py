"""MCP command handling for the Textual chat app."""

from __future__ import annotations

import shlex
import threading
import time
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # pragma: no cover
    from swecli.ui_textual.chat_app import SWECLIChatApp
    from swecli.repl.repl import REPL


class MCPCommandController:
    """Handle MCP-related slash commands and auto-connection."""

    def __init__(self, app: "SWECLIChatApp", repl: "REPL") -> None:
        """Initialize the MCP command controller.

        Args:
            app: The Textual chat application
            repl: The REPL instance with MCP manager
        """
        self.app = app
        self.repl = repl
        self._loop = getattr(app, '_loop', None)

    def handle_connect(self, command: str) -> None:
        """Handle /mcp connect command asynchronously with spinner.

        Args:
            command: The full command (e.g., "/mcp connect github")
        """
        try:
            parts = shlex.split(command)
        except ValueError:
            parts = command.strip().split()

        if len(parts) < 3:
            self.app.conversation.add_error("Usage: /mcp connect <server_name>")
            return

        server_name = parts[2]
        mcp_manager = getattr(self.repl, "mcp_manager", None)

        if not mcp_manager:
            self.app.conversation.add_error("MCP manager not available")
            return

        # Get SpinnerService for unified spinner management
        spinner_service = getattr(self.app, 'spinner_service', None)
        if spinner_service is None:
            self.app.conversation.add_error("Spinner service not available")
            return

        # Check if already connected
        if mcp_manager.is_connected(server_name):
            tools = mcp_manager.get_server_tools(server_name)
            spinner_id = spinner_service.start(f"MCP ({server_name})")
            spinner_service.stop(spinner_id, success=True, result_message=f"Already connected ({len(tools)} tools)")
            return

        # Start spinner IMMEDIATELY on UI thread for instant feedback
        spinner_id = spinner_service.start(f"MCP ({server_name})")

        # Run connection in background thread to avoid blocking UI
        def _do_connect():
            try:
                success = mcp_manager.connect_sync(server_name)
                error_msg = None
            except Exception as e:
                success = False
                error_msg = f"{type(e).__name__}: {e}"

            # Stop spinner when done (spinner_service handles thread dispatch)
            if error_msg:
                spinner_service.stop(spinner_id, success=False, result_message=error_msg)
            elif success:
                tools = mcp_manager.get_server_tools(server_name)
                spinner_service.stop(spinner_id, success=True, result_message=f"Connected ({len(tools)} tools)")
            else:
                spinner_service.stop(spinner_id, success=False, result_message="Connection failed")

            # Refresh tools after connection
            if success and hasattr(self.repl, '_refresh_runtime_tooling'):
                self.repl._refresh_runtime_tooling()

        thread = threading.Thread(target=_do_connect, daemon=True)
        thread.start()

    def handle_view(self, command: str) -> None:
        """Handle /mcp view command to show MCP server modal.

        Args:
            command: The full command (e.g., "/mcp view")
        """
        # Import here to avoid circular dependency
        from swecli.ui_textual.modals.mcp_viewer_modal import MCPViewerModal

        mcp_manager = getattr(self.repl, "mcp_manager", None)
        if not mcp_manager:
            if hasattr(self.app, "conversation"):
                self.app.conversation.add_error("MCP manager not available")
            return

        # Get MCP servers data
        mcp_data = self._get_mcp_servers_data(mcp_manager)

        # Show modal
        modal = MCPViewerModal(mcp_data)
        self.app.push_screen(modal)

    def _get_mcp_servers_data(self, mcp_manager) -> list[dict]:
        """Get MCP servers data for the viewer modal.

        Args:
            mcp_manager: The MCP manager instance

        Returns:
            List of server data dictionaries
        """
        servers = []
        if hasattr(mcp_manager, "list_servers"):
            for server_name in mcp_manager.list_servers():
                connected = mcp_manager.is_connected(server_name)
                tools = mcp_manager.get_server_tools(server_name) if connected else []
                servers.append({
                    "name": server_name,
                    "connected": connected,
                    "tool_count": len(tools),
                    "tools": tools
                })
        return servers

    def notify_manual_connect(self, enqueue_console_text_callback) -> None:
        """Notify user about manual MCP connection.

        Args:
            enqueue_console_text_callback: Callback to enqueue console text
        """
        mcp_manager = getattr(self.repl, "mcp_manager", None)
        if not mcp_manager:
            return

        if hasattr(mcp_manager, "list_servers"):
            server_names = mcp_manager.list_servers()
            if server_names:
                enqueue_console_text_callback(
                    f"⏺ MCP servers configured: {', '.join(server_names)}\n"
                    f"  ⎿  Use [bold cyan]/mcp connect <server>[/bold cyan] to connect"
                )
        else:
            enqueue_console_text_callback(
                "⏺ MCP manager available\n"
                "  ⎿  Use [bold cyan]/mcp connect <server>[/bold cyan] to connect"
            )

    def start_auto_connect_thread(self, force: bool = False) -> None:
        """Start MCP auto-connection in a background thread.

        Args:
            force: Whether to force connection even if already attempted
        """
        if not force and hasattr(self, "_mcp_connect_attempted"):
            return
        self._mcp_connect_attempted = True

        thread = threading.Thread(target=self._launch_auto_connect, daemon=True)
        thread.start()

    def _launch_auto_connect(self) -> None:
        """Launch MCP auto-connection for all configured servers."""
        import time

        mcp_manager = getattr(self.repl, "mcp_manager", None)
        if not mcp_manager:
            return

        if not hasattr(mcp_manager, "list_servers"):
            return

        server_names = mcp_manager.list_servers()
        if not server_names:
            return

        for server_name in server_names:
            if mcp_manager.is_connected(server_name):
                continue

            start = time.monotonic()
            success = mcp_manager.connect_sync(server_name)
            elapsed = int(time.monotonic() - start)

            if success:
                tools = mcp_manager.get_server_tools(server_name)
                message = f"[green]⏺[/green] MCP ({server_name}) ({elapsed}s)\n  ⎿  Connected ({len(tools)} tools)"
            else:
                message = f"[red]⏺[/red] MCP ({server_name}) ({elapsed}s)\n  ⎿  Connection failed"

            # Use _enqueue_console_text from runner if available
            if hasattr(self, "_enqueue_console_text_callback"):
                self._enqueue_console_text_callback(message)

        # Refresh tools after all connections
        if hasattr(self.repl, "_refresh_runtime_tooling"):
            self.repl._refresh_runtime_tooling()


__all__ = ["MCPCommandController"]
