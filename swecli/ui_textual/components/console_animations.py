"""Console-based spinner and progress helpers used by the REPL."""

from __future__ import annotations

import threading
import time
from typing import Optional

from rich.console import Console
from rich.live import Live
from rich.text import Text

from swecli.ui_textual import style_tokens


class Spinner:
    """Animated spinner for prompt_toolkit REPL loading states."""

    FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    INTERVAL = 0.05  # 50ms per frame for smooth animation

    def __init__(self, console: Console):
        self.console = console
        self.live: Optional[Live] = None
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._message = "Thinking..."
        self._ready = threading.Event()

    def start(self, message: str = "Thinking...") -> None:
        if self._running:
            return

        self._running = True
        self._message = message
        self._ready.clear()
        self._thread = threading.Thread(target=self._animate, daemon=True)
        self._thread.start()
        # Wait for the Live context to be ready before returning
        self._ready.wait(timeout=0.5)

    def stop(self) -> None:
        self._running = False
        if self._thread:
            self._thread.join(timeout=0.2)
            self._thread = None

    def _animate(self) -> None:
        with Live(console=self.console, auto_refresh=False, transient=True) as live:
            self.live = live
            # Signal that the Live context is ready
            self._ready.set()
            frame_idx = 0

            while self._running:
                frame = self.FRAMES[frame_idx % len(self.FRAMES)]
                text = Text(f"{frame} {self._message}", style=style_tokens.SUBTLE)
                live.update(text)
                live.refresh()
                frame_idx += 1
                time.sleep(self.INTERVAL)


class FlashingSymbol:
    """Flashing indicator for active tool execution."""

    FRAMES = ["⏺", "⏵", "▷", "⏵"]
    INTERVAL = 0.25

    def __init__(self, console: Console):
        self.console = console
        self.live: Optional[Live] = None
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._tool_call_text = ""

    def start(self, tool_call_text: str) -> None:
        if self._running:
            return

        self._running = True
        self._tool_call_text = tool_call_text
        self._thread = threading.Thread(target=self._animate, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._running = False
        if self._thread:
            self._thread.join(timeout=0.2)
            self._thread = None

    def _animate(self) -> None:
        with Live(console=self.console, auto_refresh=False, transient=True) as live:
            self.live = live
            frame_idx = 0

            while self._running:
                frame = self.FRAMES[frame_idx % len(self.FRAMES)]
                text = Text()
                text.append(f"\n{frame} ", style=style_tokens.ACCENT)
                text.append(self._tool_call_text, style=style_tokens.ACCENT)
                live.update(text)
                live.refresh()
                frame_idx += 1
                time.sleep(self.INTERVAL)


class ProgressIndicator:
    """Elapsed-time progress indicator for long-running operations."""

    def __init__(self, console: Console):
        self.console = console
        self.live: Optional[Live] = None
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._start_time: float = 0.0
        self._message = ""

    def start(self, message: str) -> None:
        if self._running:
            return

        self._running = True
        self._message = message
        self._start_time = time.time()
        self._thread = threading.Thread(target=self._update_loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._running = False
        if self._thread:
            self._thread.join(timeout=0.2)
            self._thread = None

    def _update_loop(self) -> None:
        spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        frame_idx = 0

        with Live(console=self.console, auto_refresh=False, transient=True) as live:
            self.live = live

            while self._running:
                elapsed = time.time() - self._start_time

                if elapsed >= 2.0:
                    spinner = spinner_frames[frame_idx % len(spinner_frames)]
                    text = Text()
                    text.append(f"  {self._message} ", style=style_tokens.SUBTLE)
                    text.append(f"({elapsed:.0f}s elapsed)", style=f"{style_tokens.SUBTLE} yellow")
                    text.append(f" {spinner}", style=style_tokens.SUBTLE)
                    live.update(text)
                    live.refresh()

                frame_idx += 1
                time.sleep(0.1)
