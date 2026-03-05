"""MCP Controller for TextualRunner.

This module handles Model Context Protocol (MCP) connection management, including
auto-connection logic and background connection threads.
"""

from __future__ import annotations

import asyncio
from typing import Any, Optional


class MCPController:
    """Manages MCP server connections and auto-connect workflows."""

    def __init__(
        self,
        repl: Any,
        callbacks: dict[str, Any],
    ) -> None:
        """Initialize the controller.

        Args:
            repl: The REPL instance containing the MCP manager.
            callbacks: Dictionary containing handler functions:
                - enqueue_console_text: Callable[[str], None]
                - refresh_ui_config: Callable[[], None]
        """
        self._repl = repl
        self._callbacks = callbacks
        self._app: Any | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        
        self._connect_inflight = False
        self._auto_connect_enabled = False

        # Check config for auto-connect
        if hasattr(repl, "config"):
             # Logic mirrored from runner.__init__
             pass # We'll set this via a property or method
        
    def set_app(self, app: Any) -> None:
        """Set the Textual app instance."""
        self._app = app

    def set_auto_connect(self, enabled: bool) -> None:
        """Enable or disable auto-connect."""
        self._auto_connect_enabled = enabled and hasattr(self._repl, "mcp_manager")

    def notify_manual_connect(self) -> None:
        """Inform users how to connect MCP servers when auto-connect is disabled."""
        manager = getattr(self._repl, "mcp_manager", None)
        has_servers = False
        if manager is not None:
            try:
                has_servers = bool(manager.list_servers())
            except Exception:
                has_servers = False
        if not has_servers:
            return

        message = (
            "Tip: MCP servers are not auto-connected. "
            "Run /mcp autoconnect to connect in the background "
            "or /mcp connect <name> for a specific server."
        )
        self._enqueue_text(message)

    def start_autoconnect_thread(self, loop: asyncio.AbstractEventLoop, force: bool = False) -> None:
        """Queue MCP auto-connect after the UI has rendered."""
        self._loop = loop
        
        if (not self._auto_connect_enabled and not force) or self._connect_inflight:
            return

        self._connect_inflight = True

        delay = 0.5 if not force else 0.0
        try:
            if delay > 0:
                loop.call_later(delay, self._launch_autoconnect)
            else:
                loop.call_soon(self._launch_autoconnect)
        except RuntimeError:
            self._launch_autoconnect()

    def _launch_autoconnect(self) -> None:
        """Trigger MCP auto-connect using the manager's background loop."""
        manager = getattr(self._repl, "mcp_manager", None)
        if manager is None:
            self._connect_inflight = False
            return

        def handle_completion(result: Optional[dict[str, bool]]) -> None:
            def finalize() -> None:
                self._connect_inflight = False
                if result is None:
                    self._enqueue_text(
                        "[yellow]Warning: MCP auto-connect failed.[/yellow]"
                    )
                    return
                if not result:
                    self._enqueue_text(
                        "[dim]MCP auto-connect completed; no enabled servers were found.[/dim]"
                    )
                    return

                if result:
                    successes = [name for name, ok in result.items() if ok]
                    failures = [name for name, ok in result.items() if not ok]
                    lines: list[str] = []
                    if successes:
                        lines.append(
                            "[green]✓ Connected MCP servers:[/green] "
                            + ", ".join(successes)
                        )
                    if failures:
                        lines.append(
                            "[red]✗ Failed MCP servers:[/red] "
                            + ", ".join(failures)
                        )
                    if lines:
                        self._enqueue_text("\n".join(lines))

                    # Refresh runtime tooling in REPL
                    refresh_cb = getattr(self._repl, "_refresh_runtime_tooling", None)
                    if callable(refresh_cb):
                        refresh_cb()
                    
                    # Refresh UI config (model slots etc)
                    refresh_ui = self._callbacks.get("refresh_ui_config")
                    if refresh_ui:
                        refresh_ui()

            if self._loop:
                self._loop.call_soon_threadsafe(finalize)

        try:
            manager.connect_enabled_servers_background(on_complete=handle_completion)
        except Exception as exc:  # pragma: no cover - defensive
            self._connect_inflight = False
            self._enqueue_text(
                f"[yellow]Warning: MCP auto-connect could not start ({exc}).[/yellow]"
            )

    def _enqueue_text(self, text: str) -> None:
        """Helper to enqueue console text via callback."""
        handler = self._callbacks.get("enqueue_console_text")
        if handler:
            handler(text)
