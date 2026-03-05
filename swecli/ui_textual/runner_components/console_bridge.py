"""Console output bridging for TextualRunner.

This module intercepts stdout/stderr/logs from the REPL console and bridges
them into the Textual conversation UI.
"""

from __future__ import annotations

import asyncio
import queue
from typing import Any, Optional

from rich.ansi import AnsiDecoder
from rich.text import Text


class ConsoleBridge:
    """Bridges REPL console output to the Textual UI."""

    def __init__(self, console: Any) -> None:
        """Initialize the bridge.

        Args:
            console: The Rich Console instance to capture from.
        """
        self._console = console
        self._app: Any | None = None
        self._loop: asyncio.AbstractEventLoop | None = None

        self._console_queue: asyncio.Queue[str] = asyncio.Queue()
        self._console_task: asyncio.Task[None] | None = None
        self._ansi_decoder = AnsiDecoder()

        self._original_console_print: Any = None
        self._original_console_log: Any = None

        # State for deduplication and rendering
        self._last_console_line: str | None = None
        self._last_assistant_message_normalized: str | None = None
        self._suppress_console_duplicate = False

    def set_app(self, app: Any) -> None:
        """Set the Textual app instance."""
        self._app = app

    def install(self) -> None:
        """Install the bridge into the console."""
        self._original_console_print = getattr(self._console, "print")
        self._original_console_log = getattr(self._console, "log", None)

        def bridge_print(*args, **kwargs):
            if hasattr(self._console, "capture"):
                with self._console.capture() as capture:
                    self._original_console_print(*args, **kwargs)
                text = capture.get()
                if text:  # Allow blank lines through for intentional spacing
                    self.enqueue_text(text)
            else:
                self._original_console_print(*args, **kwargs)

        self._console.print = bridge_print  # type: ignore[assignment]

        if self._original_console_log is not None:

            def bridge_log(*args, **kwargs):
                if hasattr(self._console, "capture"):
                    with self._console.capture() as capture:
                        self._original_console_log(*args, **kwargs)
                    text = capture.get()
                    if text:  # Allow blank lines through for intentional spacing
                        self.enqueue_text(text)
                else:
                    self._original_console_log(*args, **kwargs)

            self._console.log = bridge_log  # type: ignore[assignment]

    def uninstall(self) -> None:
        """Restore original console methods."""
        if self._original_console_print:
            self._console.print = self._original_console_print
        if self._original_console_log and self._original_console_log:
            self._console.log = self._original_console_log

    def start(self, loop: asyncio.AbstractEventLoop) -> None:
        """Start the background drain task."""
        self._loop = loop
        self._console_task = loop.create_task(self._drain_console_queue())

    def stop(self) -> None:
        """Stop the background drain task."""
        if self._console_task:
            self._console_task.cancel()
            # We don't await here as this is often called during shutdown cleanup

        self.uninstall()

    def enqueue_text(self, text: str) -> None:
        """Enqueue text from console for rendering."""
        if not text:
            return

        # Attempt to normalize for deduplication check using app logic if available
        if self._app and hasattr(self._app, "_normalize_paragraph"):
            # We need to access app logic, but this might be called from any thread.
            # Ideally we check deduplication at render time, but existing code checks here too?
            # Existing code checks here:
            # if hasattr(self.app, "_normalize_paragraph"): ...
            # BUT app methods are not thread safe.
            # However, enqueue_text is called from `bridge_print` which runs in whatever thread calls print().
            # The existing code did check app attributes. This is risky but we'll replicate it
            # or defer it to render time.
            # Actually, `enqueue_message` in runner puts to queue. `_enqueue_console_text` puts to `_console_queue`.
            pass

        if self._app and hasattr(self._app, "_normalize_paragraph"):
            try:
                normalized = self._app._normalize_paragraph(text)
                last_assistant = getattr(self._app, "_last_assistant_normalized", None)
                if normalized and last_assistant and normalized == last_assistant:
                    return
            except Exception:
                pass

        # Thread-safe queue put
        try:
            running_loop = asyncio.get_running_loop()
        except RuntimeError:
            running_loop = None

        if self._loop and running_loop is self._loop:
            self._console_queue.put_nowait(text)
        elif self._loop and self._loop.is_running():
            self._loop.call_soon_threadsafe(self._console_queue.put_nowait, text)
        else:
            # Fallback for tests or when loop is not fully managed
            self._console_queue.put_nowait(text)

    async def _drain_console_queue(self) -> None:
        """Drain queue and render output."""
        try:
            while True:
                text = await self._console_queue.get()
                self._render_console_output(text)
                self._console_queue.task_done()
        except asyncio.CancelledError:
            return

    def _render_console_output(self, text: str) -> None:
        """Render console output captured from REPL commands/processings."""
        if not self._app:
            return

        normalized = self._normalize_console_text(text)
        renderables = list(self._ansi_decoder.decode(normalized))
        if not renderables:
            return

        for renderable in renderables:
            if isinstance(renderable, Text):
                plain = renderable.plain.strip()
                # Allow blank lines through - they're intentional spacing
                if plain:  # Only filter non-blank lines for duplicates/spinners
                    if self._is_spinner_text(plain) or self._is_spinner_tip(plain):
                        continue

                    normalized_plain = plain.strip()
                    if hasattr(self._app, "_normalize_paragraph"):
                        normalized_plain = self._app._normalize_paragraph(plain)

                    pending = getattr(self._app, "_pending_assistant_normalized", None)
                    # self._last_assistant_message_normalized must be maintained by external setter?
                    # or we just access app state if possible?
                    # The runner maintained `self._last_assistant_message_normalized`.
                    # We can replicate logic or expose a setter.

                    targets = [
                        value
                        for value in (pending, self._last_assistant_message_normalized)
                        if value
                    ]
                    if self._suppress_console_duplicate and normalized_plain and targets:
                        if any(normalized_plain == target for target in targets):
                            continue

                    if plain == self._last_console_line:
                        continue
                    self._last_console_line = plain
                else:
                    # Empty Text from decoded newline - create visible blank line
                    renderable = Text(" ")
            else:
                self._last_console_line = None

            if hasattr(self._app, "render_console_output"):
                self._app.render_console_output(renderable)
            else:
                if hasattr(self._app, "conversation"):
                    self._app.conversation.write(renderable)

        if not isinstance(renderables[-1], Text):
            self._last_console_line = None
        if self._suppress_console_duplicate:
            self._suppress_console_duplicate = False

        if hasattr(self._app, "stop_console_buffer"):
            self._app.stop_console_buffer()

    def set_last_assistant_message(self, text: str | None) -> None:
        """Update last assistant message for deduplication."""
        self._last_assistant_message_normalized = text

    def set_suppress_duplicate(self, suppress: bool) -> None:
        """Enable suppression of duplicate console output."""
        self._suppress_console_duplicate = suppress

    @staticmethod
    def _normalize_console_text(text: str) -> str:
        """Collapse carriage-return spinner updates into a single line."""
        if "\r" not in text:
            return text
        lines = text.split("\n")
        for index, line in enumerate(lines):
            if "\r" in line:
                lines[index] = line.split("\r")[-1]
        return "\n".join(lines)

    @staticmethod
    def _is_spinner_text(plain: str) -> bool:
        """Return True if the console line appears to be a spinner update."""
        if not plain:
            return False
        first = plain[0]
        # Braille spinner characters live in the Unicode Braille block.
        if 0x2800 <= ord(first) <= 0x28FF:
            return True
        return False

    @staticmethod
    def _is_spinner_tip(plain: str) -> bool:
        """Return True if line looks like a spinner tip."""
        if not plain:
            return False
        normalized = plain.replace("⎿", "").strip().lower()
        return normalized.startswith("tip:")
