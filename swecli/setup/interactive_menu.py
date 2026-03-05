"""Interactive menu component with arrow key navigation for setup wizard."""

import sys
import tty
import termios
from typing import List, Tuple, Optional
from rich.console import Console

from swecli.ui_textual.style_tokens import BLUE_BG_ACTIVE, MENU_HINT, RAIL_BAR, ACCENT

console = Console()

# ANSI escape codes for terminal control
CLEAR_LINE = "\033[K"
CURSOR_UP = "\033[A"
HIDE_CURSOR = "\033[?25l"
SHOW_CURSOR = "\033[?25h"


class InteractiveMenu:
    """Arrow-key navigable menu with search support and Rich styling."""

    def __init__(
        self,
        items: List[Tuple[str, str, str]],  # (id, name, description)
        title: str = "Select an option",
        window_size: int = 9,
    ):
        """
        Initialize interactive menu.

        Args:
            items: List of (id, name, description) tuples
            title: Menu title
            window_size: Number of visible items at once
        """
        self.all_items = items
        self.filtered_items = items.copy()
        self.title = title
        self.window_size = window_size
        self.selected_index = 0
        self.search_query = ""
        self.search_mode = False

    def _get_key(self) -> str:
        """Read a single keypress from stdin."""
        fd = sys.stdin.fileno()
        old_settings = termios.tcgetattr(fd)
        try:
            tty.setraw(fd)
            ch = sys.stdin.read(1)
            # Handle escape sequences (arrow keys)
            if ch == "\x1b":
                ch += sys.stdin.read(2)
            return ch
        finally:
            termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)

    def _filter_items(self):
        """Filter items based on search query."""
        if not self.search_query:
            self.filtered_items = self.all_items.copy()
        else:
            query_lower = self.search_query.lower()
            self.filtered_items = [
                item
                for item in self.all_items
                if query_lower in item[1].lower() or query_lower in item[2].lower()
            ]
        # Reset selection if out of bounds
        if self.selected_index >= len(self.filtered_items):
            self.selected_index = max(0, len(self.filtered_items) - 1)

    def _render(self) -> str:
        """Render the current menu state with rail-prefixed lines."""
        from io import StringIO

        B = f"[{ACCENT}]{RAIL_BAR}[/{ACCENT}]"  # styled rail bar
        lines: list[str] = []

        # Search header (only in search mode)
        if self.search_mode:
            lines.append(
                f"  {B}  [bright_yellow]Search:[/bright_yellow] "
                f"[yellow]{self.search_query}_[/yellow]"
            )

        # Items
        total = len(self.filtered_items)
        if total == 0:
            lines.append(f"  {B}  [dim]No matches found[/dim]")
        else:
            half_window = self.window_size // 2
            start = max(0, self.selected_index - half_window)
            end = min(total, start + self.window_size)

            if end - start < self.window_size and total >= self.window_size:
                start = max(0, end - self.window_size)

            if start > 0:
                lines.append(f"  {B}  [dim]... ({start} more above)[/dim]")

            for i in range(start, end):
                item_id, name, description = self.filtered_items[i]
                is_selected = i == self.selected_index

                max_desc_len = 42
                if len(description) > max_desc_len:
                    description = description[: max_desc_len - 3] + "..."

                if is_selected:
                    pointer = "[bold bright_cyan]❯[/bold bright_cyan]"
                    name_s = f"[bold white on {BLUE_BG_ACTIVE}]{name:<24}[/bold white on {BLUE_BG_ACTIVE}]"
                    desc_s = f"[{MENU_HINT} on {BLUE_BG_ACTIVE}]{description}[/{MENU_HINT} on {BLUE_BG_ACTIVE}]"
                    lines.append(f"  {B}  {pointer} {name_s} {desc_s}")
                else:
                    name_s = f"[white]{name:<24}[/white]"
                    desc_s = f"[{MENU_HINT}]{description}[/{MENU_HINT}]"
                    lines.append(f"  {B}    {name_s} {desc_s}")

            if end < total:
                lines.append(f"  {B}  [dim]... ({total - end} more below)[/dim]")

        # Footer: only for large menus or search mode
        is_large = len(self.all_items) > self.window_size
        if is_large or self.search_mode:
            lines.append(f"  {B}")
            if self.search_mode:
                count_msg = (
                    f"  {B}  [dim]{total} result{'s' if total != 1 else ''}"
                    f" (filtered from {len(self.all_items)})[/dim]"
                )
            else:
                count_msg = (
                    f"  {B}  [dim]{total} provider{'s' if total != 1 else ''}"
                    f" · ↑/↓ · / search[/dim]"
                )
            lines.append(count_msg)

        # Render through Rich for markup processing
        string_buffer = StringIO()
        temp_console = Console(file=string_buffer, force_terminal=True, width=100)
        for line in lines:
            temp_console.print(line)

        return string_buffer.getvalue().rstrip()

    def _clear_display(self, num_lines: int):
        """Clear previously rendered lines."""
        for _ in range(num_lines):
            sys.stdout.write(CURSOR_UP + CLEAR_LINE)
        sys.stdout.flush()

    def show(self) -> Optional[str]:
        """
        Display the menu and handle user interaction.

        Returns:
            Selected item ID, or None if cancelled
        """
        if not self.all_items:
            console.print("[red]No items available[/red]")
            return None

        # Hide cursor during menu interaction
        sys.stdout.write(HIDE_CURSOR)
        sys.stdout.flush()

        try:
            # Initial render
            display = self._render()
            num_lines = display.count("\n") + 1
            print(display)

            while True:
                key = self._get_key()

                # Handle search mode
                if self.search_mode:
                    if key == "\x1b":  # Escape - exit search mode
                        self.search_mode = False
                        self.search_query = ""
                        self._filter_items()
                    elif key == "\r":  # Enter - select
                        if self.filtered_items:
                            return self.filtered_items[self.selected_index][0]
                    elif key == "\x7f":  # Backspace
                        self.search_query = self.search_query[:-1]
                        self._filter_items()
                    elif key.isprintable():
                        self.search_query += key
                        self._filter_items()
                else:
                    # Normal navigation mode
                    if key == "\x1b[A":  # Up arrow
                        self.selected_index = (self.selected_index - 1) % len(self.filtered_items)
                    elif key == "\x1b[B":  # Down arrow
                        self.selected_index = (self.selected_index + 1) % len(self.filtered_items)
                    elif key == "\r":  # Enter
                        if self.filtered_items:
                            return self.filtered_items[self.selected_index][0]
                    elif key == "/":  # Start search
                        self.search_mode = True
                        self.search_query = ""
                    elif key == "\x1b":  # Escape - cancel
                        return None
                    elif key == "\x03":  # Ctrl+C
                        raise KeyboardInterrupt

                # Re-render
                self._clear_display(num_lines)
                display = self._render()
                num_lines = display.count("\n") + 1
                print(display)

        except KeyboardInterrupt:
            self._clear_display(num_lines)
            console.print("\n[yellow]Selection cancelled[/yellow]")
            return None
        finally:
            # Show cursor again
            sys.stdout.write(SHOW_CURSOR)
            sys.stdout.flush()
