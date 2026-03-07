"""Subagent detail viewer — read-only overlay for subagent conversations."""

from __future__ import annotations

from typing import Optional

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Label, RichLog

from opendev.models.message import ChatMessage


class SubagentDetailScreen(ModalScreen[None]):
    """Modal screen that displays a subagent's conversation messages.

    Shows a read-only scrollable view of all messages from a child session.
    Press Escape to dismiss.
    """

    BINDINGS = [
        Binding("escape", "dismiss_screen", "Close", show=False),
    ]

    DEFAULT_CSS = """
    SubagentDetailScreen {
        align: center middle;
    }
    #subagent-detail-container {
        width: 90%;
        height: 85%;
        background: $surface;
        border: solid $accent;
        padding: 1;
    }
    #subagent-detail-header {
        height: 3;
        padding: 0 1;
        background: $accent;
        color: $text;
    }
    #subagent-detail-log {
        height: 1fr;
        border: solid $surface-lighten-2;
        padding: 0 1;
    }
    #subagent-detail-footer {
        height: 1;
        padding: 0 1;
        color: $text-muted;
    }
    """

    def __init__(
        self,
        session_id: str,
        messages: list[ChatMessage],
        title: str = "Subagent Conversation",
    ) -> None:
        super().__init__()
        self._session_id = session_id
        self._messages = messages
        self._title = title

    def compose(self) -> ComposeResult:
        with Vertical(id="subagent-detail-container"):
            yield Label(
                f"[bold]{self._title}[/bold]  [dim]session: {self._session_id[:12]}[/dim]",
                id="subagent-detail-header",
            )
            yield RichLog(
                highlight=True,
                markup=True,
                wrap=True,
                id="subagent-detail-log",
            )
            yield Label("[dim]Press Escape to close[/dim]", id="subagent-detail-footer")

    def on_mount(self) -> None:
        log = self.query_one("#subagent-detail-log", RichLog)
        if not self._messages:
            log.write("[dim]No messages in this subagent session.[/dim]")
            return

        for msg in self._messages:
            role = msg.role.value
            content = msg.content or ""

            if role == "user":
                log.write(f"[bold cyan]User:[/bold cyan]")
                log.write(content)
                log.write("")
            elif role == "assistant":
                log.write(f"[bold green]Assistant:[/bold green]")
                # Show tool calls if any
                if msg.tool_calls:
                    for tc in msg.tool_calls:
                        log.write(f"  [yellow]Tool: {tc.name}[/yellow]")
                        if tc.result_summary:
                            log.write(f"  [dim]{tc.result_summary}[/dim]")
                if content:
                    log.write(content)
                log.write("")
            elif role == "system":
                log.write(f"[bold magenta]System:[/bold magenta]")
                log.write(f"[dim]{content[:500]}[/dim]")
                log.write("")
            elif role == "tool":
                # Tool results are usually shown inline with assistant tool_calls
                log.write(f"[dim]Tool result: {content[:200]}[/dim]")
                log.write("")

    def action_dismiss_screen(self) -> None:
        self.dismiss(None)
