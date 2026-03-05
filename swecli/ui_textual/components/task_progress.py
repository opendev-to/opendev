"""Task progress display with live updates, timer, and ESC interrupt."""

import threading
import time
from typing import Optional

from rich.console import Console
from rich.live import Live
from rich.text import Text

try:
    from pynput import keyboard
    PYNPUT_AVAILABLE = True
except ImportError:
    PYNPUT_AVAILABLE = False

from swecli.core.runtime.monitoring import TaskMonitor
from swecli.ui_textual import style_tokens
from swecli.ui_textual.formatters_internal.markdown_formatter import markdown_to_plain_text


class TaskProgressDisplay:
    """Display task progress with live timer, token counter, and ESC interrupt."""

    UPDATE_INTERVAL = 0.5  # Update display every 0.5 seconds (more responsive)
    SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]  # Braille dots

    def __init__(self, console: Console, task_monitor: TaskMonitor):
        """Initialize task progress display.

        Args:
            console: Rich console for output
            task_monitor: TaskMonitor instance tracking the task
        """
        self.console = console
        self.task_monitor = task_monitor
        self.live: Optional[Live] = None
        self._running = False
        self._update_thread: Optional[threading.Thread] = None
        self._keyboard_listener: Optional[keyboard.Listener] = None
        self._frame_index = 0  # Track current spinner frame

    def start(self) -> None:
        """Start displaying task progress with live updates."""
        if self._running:
            return

        self._running = True

        # ESC is now handled centrally by InterruptManager via Textual key bindings.
        # pynput listener is disabled to avoid competing input systems.

        # Start update loop in background thread
        self._update_thread = threading.Thread(target=self._update_loop, daemon=True)
        self._update_thread.start()

    def stop(self) -> None:
        """Stop displaying task progress."""
        self._running = False

        # Stop keyboard listener
        if self._keyboard_listener:
            try:
                self._keyboard_listener.stop()
            except Exception:
                pass
            self._keyboard_listener = None

        # Wait for update thread to finish
        if self._update_thread:
            self._update_thread.join(timeout=0.5)
            self._update_thread = None

    def _start_keyboard_listener(self) -> None:
        """Start listening for ESC key press.

        DEPRECATED: ESC is now handled centrally by InterruptManager via
        Textual key bindings. This pynput listener is disabled to avoid
        competing input systems.
        """
        return

    def _update_loop(self) -> None:
        """Update loop running in background thread."""
        with Live(console=self.console, auto_refresh=False, transient=True) as live:
            self.live = live

            # Show first update IMMEDIATELY (no delay)
            text = self._format_display()
            live.update(text)
            live.refresh()

            # Continue updating at regular intervals
            while self._running and self.task_monitor.is_running():
                time.sleep(self.UPDATE_INTERVAL)

                # Format and update display
                text = self._format_display()
                live.update(text)
                live.refresh()

    def _format_display(self) -> Text:
        """Format the task display.

        Returns:
            Rich Text object with formatted display
        """
        text = Text()

        # Animated spinner using Braille dots
        spinner_char = self.SPINNER_FRAMES[self._frame_index % len(self.SPINNER_FRAMES)]
        text.append(f"{spinner_char} ", style=style_tokens.SUBTLE)
        self._frame_index += 1  # Advance to next frame

        # Task description
        task_desc = self.task_monitor.get_task_description()
        text.append(f"{task_desc}… ", style=style_tokens.SUBTLE)

        # Build info section: (esc to interrupt · XXs · ↓/↑ XXk tokens)
        info_parts = []

        # ESC hint (only if keyboard listener is available)
        if PYNPUT_AVAILABLE:
            info_parts.append("esc to interrupt")

        # Elapsed time
        elapsed = self.task_monitor.get_elapsed_seconds()
        info_parts.append(f"{elapsed}s")

        # Token display (only if there's a change)
        token_display = self.task_monitor.get_formatted_token_display()
        if token_display:
            info_parts.append(token_display)

        # Combine info parts
        if info_parts:
            info_str = " · ".join(info_parts)
            text.append(f"({info_str})", style=f"{style_tokens.SUBTLE} yellow")

        return text

    def print_final_status(self, replacement_message: Optional[str] = None) -> None:
        """Print final status after task completes.

        Args:
            replacement_message: If provided, use this instead of task description
                                (useful for replacing spinner with actual LLM response)
        """
        stats = self.task_monitor.stop()

        # Determine symbol
        if stats["interrupted"]:
            symbol = "⏹"
            status = "interrupted"
        else:
            symbol = "⏺"
            status = "completed"

        elapsed = stats["elapsed_seconds"]
        info_parts = [f"{status} in {elapsed}s"]

        token_display = self.task_monitor.get_formatted_token_display()
        if token_display:
            info_parts.append(token_display)

        cleaned_message = (replacement_message or "").strip()

        if cleaned_message:
            plain = markdown_to_plain_text(cleaned_message)
            lines = plain.splitlines() if plain else []
            if lines:
                lines[0] = f"{symbol} {lines[0]}"
            else:
                lines = [symbol]
            message = "\n".join(lines)
            self.console.print(Text(message))
            status_line = Text(f"{symbol} {', '.join(info_parts)}", style=style_tokens.SUBTLE)
            self.console.print(status_line)
        else:
            message_text = stats["task_description"] or status
            status_msg = Text(f"{symbol} {message_text}", style=style_tokens.SUBTLE)
            status_msg.append(f" ({', '.join(info_parts)})", style=style_tokens.SUBTLE)
            self.console.print(status_msg)
