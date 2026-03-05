"""Status bar and footer widgets for the OpenDev Textual UI."""

from __future__ import annotations

import subprocess
from concurrent.futures import ThreadPoolExecutor
from typing import Mapping

from rich.text import Text
from textual.app import ComposeResult
from textual.widgets import Footer, Static

from swecli.ui_textual.style_tokens import (
    BLUE_BRIGHT,
    BLUE_TASK,
    CYAN,
    GREEN_BRIGHT,
    GREEN_LIGHT,
    GREY,
    ORANGE,
    ORANGE_CAUTION,
    GOLD,
)


def _get_git_branch_in_thread(working_dir: str) -> str | None:
    """Get git branch in a separate thread to avoid Textual FD issues.

    Returns:
        Branch name or None if not a git repo
    """
    import os

    kwargs: dict = {
        "cwd": working_dir,
        "stdin": subprocess.DEVNULL,
        "stdout": subprocess.PIPE,
        "stderr": subprocess.PIPE,
        "text": True,
        "close_fds": True,
    }

    if os.name != "nt":
        kwargs["start_new_session"] = True

    try:
        result = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            timeout=2,
            **kwargs,
        )
        if result.returncode == 0:
            branch = result.stdout.strip()
            if branch and branch != "HEAD":
                return branch

        # Try symbolic-ref as fallback
        result = subprocess.run(
            ["git", "symbolic-ref", "--short", "HEAD"],
            timeout=2,
            **kwargs,
        )
        if result.returncode == 0:
            return result.stdout.strip() or None

    except Exception:
        # Catch ALL exceptions including ValueError for FD issues
        pass

    return None


def _get_git_branch_safe(working_dir: str) -> str | None:
    """Get git branch using thread pool to avoid Textual FD issues.

    This function is designed to be safe to call from Textual's event loop.
    It runs subprocess in a separate thread with isolated file descriptors.
    If anything goes wrong, it returns None rather than raising.
    """
    import asyncio

    # Check if we're in an async context
    try:
        asyncio.get_running_loop()
        # We're in an async context (Textual) - subprocess may cause FD errors
        # Skip git branch detection entirely to avoid issues
        return None
    except RuntimeError:
        # No running loop - safe to call subprocess
        pass

    try:
        with ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(_get_git_branch_in_thread, working_dir)
            return future.result(timeout=3)
    except Exception:
        # Catch ANY error - git branch display is not critical
        return None


class StatusBar(Static):
    """Custom status bar showing mode, repo info, and hints."""

    def __init__(self, model: str = "claude-sonnet-4", working_dir: str = "", **kwargs):
        super().__init__(**kwargs)
        self.mode = "normal"
        self.model = model
        self.autonomy = "Manual"  # Autonomy level: Manual, Semi-Auto, Auto
        self.thinking_level = "Medium"  # Thinking level: Off, Low, Medium, High
        self.spinner_text: str | None = None
        self.spinner_tip: str | None = None
        self.working_dir = working_dir or ""
        self.context_usage_pct: float = 0.0  # 0% = no context used at startup
        self.session_cost: float = 0.0  # Running session cost in USD
        self._git_branch = None

    def on_mount(self) -> None:
        """Update status on mount."""
        self.update_status()

    def on_resize(self) -> None:
        """Re-render when widget gets sized or terminal resizes."""
        self.update_status()

    def set_mode(self, mode: str) -> None:
        """Update mode display."""
        self.mode = mode
        self.update_status()

    def set_model_name(self, model: str) -> None:
        """Update the displayed model name."""
        self.model = model
        self.update_status()

    def set_autonomy(self, level: str) -> None:
        """Update autonomy level display.

        Args:
            level: One of "Manual", "Semi-Auto", or "Auto"
        """
        self.autonomy = level
        self.update_status()

    def set_thinking_level(self, level: str) -> None:
        """Update thinking level display.

        Args:
            level: One of "Off", "Low", "Medium", "High"
        """
        self.thinking_level = level
        self.update_status()

    @property
    def thinking_enabled(self) -> bool:
        """Legacy property - True if thinking level is not Off."""
        return self.thinking_level != "Off"

    # Legacy compatibility
    def set_thinking_enabled(self, enabled: bool) -> None:
        """Legacy method - sets level to Medium if enabled, Off if disabled."""
        self.thinking_level = "Medium" if enabled else "Off"
        self.update_status()

    def set_critique_enabled(self, enabled: bool) -> None:
        """Legacy method - sets level to High if enabled."""
        if enabled:
            self.thinking_level = "High"
        self.update_status()

    def set_spinner(self, text: str, tip: str | None = None) -> None:
        """Display spinner status."""
        self.spinner_text = text
        if tip is not None:
            self.spinner_tip = tip
        self.update_status()

    def clear_spinner(self) -> None:
        """Clear spinner status."""
        self.spinner_text = None
        self.spinner_tip = None
        self.update_status()

    def set_context_usage(self, pct: float) -> None:
        """Update context usage percentage display.

        Args:
            pct: Percentage of context window used (0-100+).
        """
        import logging as _log

        _log.getLogger("swecli.context_debug").info(
            "set_context_usage: old=%.2f new=%.2f", self.context_usage_pct, pct
        )
        self.context_usage_pct = pct
        self.update_status()

    def set_session_cost(self, cost: float) -> None:
        """Update session cost display.

        Args:
            cost: Total session cost in USD.
        """
        self.session_cost = cost
        self.update_status()

    def update_status(self) -> None:
        """Update status bar text with mode hint, autonomy level, thinking status, repo info, and spinner."""
        mode_color = ORANGE if self.mode == "normal" else GREEN_LIGHT
        status = Text()

        # Mode with cycling hint
        status.append("Mode: ", style=GREY)
        status.append(f"{self.mode.upper()}", style=f"bold {mode_color}")
        status.append(" (Shift+Tab)", style=GREY)

        # Autonomy level with color coding
        status.append("  │  ", style=GREY)
        status.append("Autonomy: ", style=GREY)
        autonomy_colors = {
            "Manual": ORANGE_CAUTION,
            "Semi-Auto": CYAN,
            "Auto": GREEN_BRIGHT,
        }
        autonomy_color = autonomy_colors.get(self.autonomy, GREY)
        status.append(self.autonomy, style=f"bold {autonomy_color}")
        status.append(" (Ctrl+Shift+A)", style=GREY)

        # Thinking level status
        status.append("  │  ", style=GREY)
        status.append("Thinking: ", style=GREY)
        thinking_colors = {
            "Off": GREY,
            "Low": CYAN,
            "Medium": GREEN_BRIGHT,
            "High": GOLD,
        }
        thinking_color = thinking_colors.get(self.thinking_level, GREEN_BRIGHT)
        status.append(self.thinking_level, style=f"bold {thinking_color}")
        status.append(" (Ctrl+Shift+T)", style=GREY)

        # Repo info
        repo_display = self._get_repo_display()
        if repo_display:
            status.append("  │  ", style=GREY)
            status.append(repo_display, style=BLUE_BRIGHT)

        if self.spinner_text:
            status.append("  │  ", style=GREY)
            status.append(self.spinner_text, style=BLUE_BRIGHT)

        # Right-aligned section: cost + context left
        context_left = max(0.0, 100.0 - self.context_usage_pct)
        pct_str = f"{context_left:.1f}"
        if context_left > 50:
            pct_color = GREEN_BRIGHT
        elif context_left > 25:
            pct_color = GOLD
        else:
            pct_color = ORANGE

        # Build right-side text for width calculation
        cost_str = ""
        if self.session_cost > 0:
            if self.session_cost < 0.01:
                cost_str = f"${self.session_cost:.4f}"
            else:
                cost_str = f"${self.session_cost:.2f}"

        right_text = f"Context left {pct_str}%"
        if cost_str:
            right_text = f"Cost {cost_str}  │  {right_text}"

        left_len = len(status.plain)
        right_len = len(right_text)
        try:
            available_width = self.size.width
        except Exception:
            available_width = 0

        if available_width < 40:
            available_width = left_len + right_len + 4  # Minimal gap before sizing

        gap = available_width - left_len - right_len
        if gap >= 2:
            status.append(" " * gap)
        else:
            status.append("  │  ", style=GREY)

        if cost_str:
            status.append("Cost ", style=GREY)
            status.append(cost_str, style=f"bold {CYAN}")
            status.append("  │  ", style=GREY)

        status.append(f"Context left {pct_str}%", style=f"bold {pct_color}")

        self.update(status)

    def _get_repo_display(self) -> str:
        """Get a formatted repo display with path and git branch."""
        if not self.working_dir:
            return ""

        try:
            from pathlib import Path

            # Convert to Path for easier manipulation
            work_dir = Path(self.working_dir)

            # Get relative path from home directory
            home_dir = Path.home()
            if work_dir.is_relative_to(home_dir):
                # Show as ~/relative/path
                rel_path = work_dir.relative_to(home_dir)
                path_display = f"~/{rel_path}"
            else:
                # Use absolute path but shorten if too long
                path_display = str(work_dir)
                if len(path_display) > 30:
                    parts = path_display.split("/")
                    if len(parts) > 2:
                        # Show .../last/two/parts
                        path_display = f".../{'/'.join(parts[-2:])}"

            # Get git branch info using thread-safe helper
            branch = _get_git_branch_safe(self.working_dir)
            if branch:
                return f"{path_display} ({branch})"

            # Return just path if no git branch found
            return path_display

        except Exception:
            # Fallback to just showing the working directory
            return str(self.working_dir) if self.working_dir else ""

    def _get_short_model_name(self) -> str:
        """Get a very short model name for display."""
        if not self.model:
            return ""

        # If model contains a slash, take the last part
        if "/" in self.model:
            parts = self.model.split("/")
            model_part = parts[-1]
        else:
            model_part = self.model

        # If it's too long, truncate intelligently
        if len(model_part) > 15:
            # Remove common prefixes/suffixes
            model_part = model_part.replace("accounts/", "").replace("-instruct", "").replace("-latest", "")

            # Still too long? Take first 15 chars
            if len(model_part) > 15:
                model_part = model_part[:15] + "..."

        return model_part

    def _smart_truncate_model(self, model: str, max_len: int) -> str:
        """Smart model name truncation that preserves important parts."""
        if len(model) <= max_len:
            return model

        if "/" in model:
            parts = model.split("/")
            if len(parts) >= 2:
                provider = parts[0]
                model_name = parts[-1]
                simplified = f"{provider}/{model_name}"
                if len(simplified) <= max_len:
                    return simplified
                available = max_len - len(provider) - 1
                if available > 10:
                    return f"{provider}/{model_name[: available - 3]}..."

        return model[: max_len - 3] + "..."


class ModelFooter(Footer):
    """Footer variant that shows configured model slots alongside key hints."""

    def __init__(
        self,
        model_slots: Mapping[str, tuple[str, str]] | None = None,
        normal_model: str = "",
        **kwargs,
    ) -> None:
        super().__init__(**kwargs)
        self._model_slots: dict[str, tuple[str, str]] = dict(model_slots or {})
        self._normal_model = normal_model
        self._models_label: Static | None = None
        self._background_task_count: int = 0

    def compose(self) -> ComposeResult:
        """Compose footer with model display prefix and inherited key hints."""
        self._models_label = Static(
            self._build_model_text(),
            classes="footer--models",
            expand=True,
        )
        yield self._models_label

        parent_compose = super().compose()
        if parent_compose is not None:
            yield from parent_compose

    def update_models(self, model_slots: Mapping[str, tuple[str, str]] | None) -> None:
        """Refresh displayed models (no-op, model display removed from footer)."""
        self._model_slots = dict(model_slots or {})

    def set_normal_model(self, model: str) -> None:
        """Update normal model display (no-op, model display removed from footer)."""
        self._normal_model = model

    def set_background_task_count(self, count: int) -> None:
        """Update background task count indicator.

        Args:
            count: Number of running background tasks
        """
        self._background_task_count = count
        if self._models_label is not None:
            self._models_label.update(self._build_model_text())

    def _build_model_text(self) -> Text:
        """Build footer text showing only background task indicator."""
        text = Text(no_wrap=True)
        if self._background_task_count > 0:
            task_word = "task" if self._background_task_count == 1 else "tasks"
            text.append(
                f"{self._background_task_count} background {task_word}",
                style=BLUE_TASK,
            )
            text.append(" (Ctrl+B)", style=GREY)
        return text


__all__ = ["StatusBar", "ModelFooter"]
