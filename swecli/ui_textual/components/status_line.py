"""Status line component for OpenDev."""

import subprocess
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Optional, Sequence, Tuple

from rich.console import Console
from rich.text import Text


def _run_git_branch_in_thread(working_dir: str) -> str | None:
    """Run git command in a thread to avoid Textual FD issues."""
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
            timeout=1,
            **kwargs,
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except Exception:
        # Catch ALL exceptions including ValueError for FD issues
        pass
    return None


class StatusLine:
    """Bottom status line showing context info."""

    def __init__(self, console: Console):
        """Initialize status line.

        Args:
            console: Rich console for output
        """
        self.console = console
        self._detailed = False

    def render(
        self,
        model: str,
        working_dir: Path,
        tokens_used: int,
        tokens_limit: int,
        git_branch: Optional[str] = None,
        mode: Optional[str] = None,
        latency_ms: Optional[int] = None,
        key_hints: Optional[Sequence[Tuple[str, str]]] = None,
        notifications: Optional[Sequence[str]] = None,
    ) -> None:
        """Render status line at bottom.

        Args:
            model: Model name
            working_dir: Current working directory
            tokens_used: Tokens used in session
            tokens_limit: Token limit
            git_branch: Git branch name (if in repo)
            mode: Current operation mode label
            latency_ms: Milliseconds elapsed on last model call
            key_hints: Shortcut hints to surface inline
            notifications: Recent notification summaries to surface inline
        """
        return

    def _truncate_model(self, model: str, max_len: int = 60) -> str:
        """Smart model name truncation that preserves important parts.

        Args:
            model: Full model name
            max_len: Maximum length

        Returns:
            Truncated model name with important parts preserved
        """
        # For fireworks models, always simplify to show provider + actual model name
        if "accounts/fireworks/models/" in model:
            try:
                # Extract: "accounts/fireworks/models/actual-model-name" -> "fireworks/actual-model-name"
                parts = model.split("accounts/fireworks/models/")
                if len(parts) == 2:
                    provider = "fireworks"
                    model_name = parts[1]
                    simplified = f"{provider}/{model_name}"
                    if len(simplified) <= max_len:
                        return simplified
                    # If still too long, truncate the model name part
                    available = max_len - len(provider) - 1  # -1 for "/"
                    if available > 10:  # Ensure meaningful truncation
                        return f"{provider}/{model_name[:available-3]}..."
            except Exception:
                pass
                
        # If no truncation needed and no special processing applied
        if len(model) <= max_len:
            return model
        
        # For other providers with slashes, try to keep provider/model format
        if "/" in model:
            parts = model.split("/")
            if len(parts) >= 2:
                provider = parts[0]
                model_name = parts[-1]  # Last part is usually the model name
                simplified = f"{provider}/{model_name}"
                if len(simplified) <= max_len:
                    return simplified
                # If still too long, truncate model name
                available = max_len - len(provider) - 1
                if available > 10:
                    return f"{provider}/{model_name[:available-3]}..."
        
        # Fallback: simple truncation
        return model[:max_len-3] + "..."

    def _smart_path(self, path: Path, max_len: int = 30) -> str:
        """Smart path display.

        Args:
            path: Path to display
            max_len: Maximum length

        Returns:
            Formatted path
        """
        path_str = str(path)

        # Replace home with ~
        home = str(Path.home())
        if path_str.startswith(home):
            path_str = "~" + path_str[len(home):]

        # Truncate if too long
        if len(path_str) > max_len:
            # Show start and end
            start_len = max_len // 2 - 2
            end_len = max_len // 2 - 2
            path_str = path_str[:start_len] + "..." + path_str[-end_len:]

        return path_str

    def _get_git_branch(self, working_dir: Path) -> Optional[str]:
        """Get current git branch.

        Args:
            working_dir: Working directory

        Returns:
            Branch name or None
        """
        import asyncio

        # Check if we're in an async context (Textual)
        try:
            asyncio.get_running_loop()
            # We're in Textual - skip subprocess to avoid FD errors
            return None
        except RuntimeError:
            pass

        # Use thread pool to avoid FD issues
        try:
            with ThreadPoolExecutor(max_workers=1) as executor:
                future = executor.submit(
                    _run_git_branch_in_thread, str(working_dir)
                )
                return future.result(timeout=2)
        except Exception:
            return None

    def _format_tokens(self, used: int, limit: int) -> Text:
        """Format token usage with color coding.

        Args:
            used: Tokens used
            limit: Token limit

        Returns:
            Formatted token string
        """
        # Format numbers with K suffix
        def format_num(n: int) -> str:
            if n >= 1000:
                return f"{n/1000:.1f}k"
            return str(n)

        usage_pct = (used / limit * 100) if limit > 0 else 0

        # Color code based on usage
        if usage_pct > 90:
            status = "critical"
        elif usage_pct > 80:
            status = "warning"
        else:
            status = "normal"

        label = f"{format_num(used)}/{format_num(limit)}"
        text = Text(label)
        if status == "critical":
            text.stylize("bold red")
        elif status == "warning":
            text.stylize("yellow")
        else:
            text.stylize("green")
        return text

    def toggle_detailed(self) -> bool:
        """Toggle detailed mode.

        Returns:
            bool: True when detailed mode enabled
        """
        self._detailed = not self._detailed
        return self._detailed
