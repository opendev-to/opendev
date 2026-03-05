"""Animated welcome panel widget with Matrix rain OpenDev animation."""

from __future__ import annotations

import colorsys
import math
import os
import random
from pathlib import Path
from typing import Callable, Optional, TYPE_CHECKING

from rich.align import Align
from rich.console import RenderableType
from rich.panel import Panel
from rich.text import Text
from textual.reactive import reactive
from textual.timer import Timer
from textual.widget import Widget

from swecli.core.runtime import OperationMode

if TYPE_CHECKING:
    pass

__all__ = ["AnimatedWelcomePanel"]


def hsl_to_ansi256(hue: float, saturation: float = 0.7, lightness: float = 0.6) -> int:
    """Convert HSL to closest ANSI-256 color code.

    Args:
        hue: Hue value (0-360)
        saturation: Saturation (0-1)
        lightness: Lightness (0-1)

    Returns:
        ANSI-256 color code (16-231 for color cube)
    """
    # Normalize hue to 0-1
    h = (hue % 360) / 360.0

    # Convert HSL to RGB
    r, g, b = colorsys.hls_to_rgb(h, lightness, saturation)

    # Convert RGB (0-1) to 6-level values (0-5) for ANSI color cube
    r6 = int(round(r * 5))
    g6 = int(round(g * 5))
    b6 = int(round(b * 5))

    # ANSI color cube: 16 + 36*r + 6*g + b
    return 16 + 36 * r6 + 6 * g6 + b6


BRAILLE_FRAMES: tuple[str, ...] = ("⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏")


class AnimatedWelcomePanel(Widget):
    """Animated welcome panel with gradient color wave effect.

    Features:
    - Rainbow gradient that waves across the text
    - Smooth fade-out animation on dismiss
    - Responsive centering
    """

    DEFAULT_CSS = """
    AnimatedWelcomePanel {
        width: 100%;
        height: auto;
        max-height: 60%;
        align: center middle;
        content-align: center middle;
        overflow-y: hidden;
    }
    """

    # Reactive properties for animation state
    gradient_offset: reactive[int] = reactive(0)
    fade_progress: reactive[float] = reactive(1.0)

    def __init__(
        self,
        current_mode: OperationMode = OperationMode.NORMAL,
        working_dir: Optional[Path] = None,
        username: Optional[str] = None,
        on_fade_complete: Optional[Callable[[], None]] = None,
        **kwargs,
    ):
        """Initialize the animated welcome panel.

        Args:
            current_mode: Current operation mode (NORMAL/PLAN)
            working_dir: Working directory path
            username: User's name for greeting
            on_fade_complete: Callback when fade-out animation completes
        """
        super().__init__(**kwargs)
        self._current_mode = current_mode
        self._working_dir = working_dir or Path.cwd()
        self._username = username or os.getenv("USER", "Developer")
        self._on_fade_complete = on_fade_complete
        self._animation_timer: Optional[Timer] = None
        self._fade_timer: Optional[Timer] = None
        self._is_fading = False

        # Matrix rain state
        self._rain_columns: list[dict] = []
        self._rain_field_width: int = 0
        self._rain_field_height: int = 0
        self._braille_offset: int = 0
        self._braille_tick: int = 0
        self._breathe_phase: float = 0.0

        # Cache the plain text content for gradient coloring
        self._content_lines = self._generate_content()

    @staticmethod
    def get_version() -> str:
        """Get OpenDev version."""
        try:
            from importlib.metadata import version

            return f"v{version('opendev')}"
        except Exception:
            return "v0.1.7"

    def _generate_content(self) -> list[str]:
        """Generate horizontally spread welcome content.

        Returns:
            List of strings for welcome text section (spread horizontally)
        """
        version = self.get_version()
        mode = self._current_mode.value.upper()

        # Horizontal spread layout - 3 lines max
        lines = [
            f"═══  O P E N D E V  {version}  ═══  Mode: {mode}  ═══",
            "",
            "/help  │  /models  │  Shift+Tab plan mode  │  @file context",
        ]

        return lines

    def on_mount(self) -> None:
        """Start gradient animation on mount."""
        self._animation_timer = self.set_interval(0.05, self._update_gradient)

    def on_unmount(self) -> None:
        """Clean up timers on unmount."""
        if self._animation_timer:
            self._animation_timer.stop()
            self._animation_timer = None
        if self._fade_timer:
            self._fade_timer.stop()
            self._fade_timer = None

    def on_resize(self, event) -> None:
        """Hide on very small terminals, restore when large enough."""
        try:
            terminal_height = self.app.size.height
        except Exception:
            return
        if terminal_height < 15 and self.styles.display != "none":
            self.styles.display = "none"
        elif terminal_height >= 15 and self.styles.display == "none" and not self._is_fading:
            self.styles.display = "block"

    def _update_gradient(self) -> None:
        """Advance gradient wave and shape rotation animations."""
        if self._is_fading:
            return
        # Shift gradient offset (5 degrees per frame = full cycle in ~3.6s)
        self.gradient_offset = (self.gradient_offset + 5) % 360

        # Braille frame cycling: every 2 ticks (~100ms)
        self._braille_tick += 1
        if self._braille_tick >= 2:
            self._braille_tick = 0
            self._braille_offset = (self._braille_offset + 1) % len(BRAILLE_FRAMES)

        # Breathing: full sine cycle in 4s (80 ticks at 50ms)
        self._breathe_phase += (2.0 * math.pi) / 80.0
        if self._breathe_phase >= 2.0 * math.pi:
            self._breathe_phase -= 2.0 * math.pi

        # Advance rain columns
        if self._rain_columns:
            self._step_rain()

    def _init_rain_column(self, height: int) -> dict:
        """Create one rain column with randomized params."""
        return {
            "y": random.uniform(-height, 0),
            "speed": random.uniform(0.15, 0.45),
            "trail_len": random.randint(3, 8),
            "char_offset": random.randint(0, 9),
        }

    def _ensure_rain_field(self, width: int, height: int) -> None:
        """Lazily initialize or resize the rain column list."""
        if width != self._rain_field_width or height != self._rain_field_height:
            self._rain_field_width = width
            self._rain_field_height = height
            self._rain_columns = [self._init_rain_column(height) for _ in range(width)]

    def _step_rain(self) -> None:
        """Advance all rain columns; reset ones that fell off-screen."""
        for col in self._rain_columns:
            col["y"] += col["speed"]
            if col["y"] - col["trail_len"] > self._rain_field_height:
                col["y"] = random.uniform(-col["trail_len"] - 5, -1)
                col["speed"] = random.uniform(0.15, 0.45)
                col["trail_len"] = random.randint(3, 8)
                col["char_offset"] = random.randint(0, 9)

    def _do_fade(self) -> None:
        """Execute one fade animation step."""
        new_progress = self.fade_progress - 0.08  # ~12 frames to fully fade

        if new_progress <= 0:
            self.fade_progress = 0
            if self._fade_timer:
                self._fade_timer.stop()
                self._fade_timer = None
            if self._on_fade_complete:
                self._on_fade_complete()
        else:
            self.fade_progress = new_progress

    def fade_out(self, callback: Optional[Callable[[], None]] = None) -> None:
        """Start fade-out animation.

        Args:
            callback: Optional callback to invoke when fade completes
        """
        if self._is_fading:
            return

        self._is_fading = True
        if callback:
            self._on_fade_complete = callback

        # Stop gradient animation
        if self._animation_timer:
            self._animation_timer.stop()
            self._animation_timer = None

        # Start fade animation
        self._fade_timer = self.set_interval(0.025, self._do_fade)

    def _apply_gradient(self, text: str, line_offset: int = 0) -> Text:
        """Apply gradient coloring to text.

        Args:
            text: Plain text to colorize
            line_offset: Vertical offset for wave effect

        Returns:
            Rich Text object with gradient colors
        """
        result = Text()

        for i, char in enumerate(text):
            if char.isspace():
                result.append(char)
                continue

            # Calculate hue based on character position and animation offset
            # Add line_offset to create vertical wave effect
            hue = (i * 8 + line_offset * 20 + self.gradient_offset) % 360

            # Apply fade by reducing saturation and moving toward gray
            saturation = 0.8 * self.fade_progress
            lightness = 0.6 * self.fade_progress + 0.1 * (1 - self.fade_progress)

            color_code = hsl_to_ansi256(hue, saturation, lightness)
            result.append(char, style=f"color({color_code})")

        return result

    def _render_welcome_text(self) -> Text:
        """Render welcome content with gradient coloring.

        Returns:
            Rich Text object with gradient-colored multi-line welcome text
        """
        result = Text(justify="center")

        for line_idx, line in enumerate(self._content_lines):
            if line_idx > 0:
                result.append("\n")
            # Apply gradient with vertical wave offset for each line
            result.append_text(self._apply_gradient(line, line_idx))

        return result

    def _calculate_rain_size(self) -> tuple[int, int]:
        """Calculate rain field dimensions based on available widget size."""
        widget_width = self.size.width if self.size.width > 0 else 100
        widget_height = self.size.height if self.size.height > 0 else 30
        width = max(30, min(90, int(widget_width * 0.7)))
        # Reserve ~7 rows for welcome text (3), panel border (2), blank line (1), margin (1)
        available_for_rain = widget_height - 7
        if available_for_rain < 4:
            height = 0  # Signal to render(): skip rain entirely
        else:
            height = max(4, min(20, available_for_rain))
        return width, height

    def _render_rain_field(self) -> Text:
        """Render the matrix rain field with 'OpenDev' as negative space.

        Returns:
            Rich Text object containing the braille rain field
        """
        width = self._rain_field_width
        height = self._rain_field_height
        fade = self.fade_progress

        # Build empty grid: each cell is (char, hue, saturation, lightness) or None
        grid: list[list[tuple[str, float, float, float] | None]] = [
            [None for _ in range(width)] for _ in range(height)
        ]

        # Define "OpenDev" exclusion zone (centered)
        label = "O p e n D e v"
        label_len = len(label)
        label_col_start = (width - label_len) // 2
        label_col_end = label_col_start + label_len
        label_row = height // 2
        # Exclusion zone: label row +/- 1 row padding
        excl_row_start = max(0, label_row - 1)
        excl_row_end = min(height, label_row + 2)

        # Fill rain drops into grid
        for col_idx, col in enumerate(self._rain_columns):
            head_y = col["y"]
            trail_len = col["trail_len"]
            char_offset = col["char_offset"]

            for t in range(trail_len + 1):
                row_y = int(head_y) - t
                if row_y < 0 or row_y >= height:
                    continue

                # Skip cells inside the exclusion zone
                if (
                    excl_row_start <= row_y < excl_row_end
                    and label_col_start - 1 <= col_idx < label_col_end + 1
                ):
                    continue

                # Brightness: head is brightest, trail fades
                if t == 0:
                    lightness = 0.55
                else:
                    lightness = max(0.15, 0.55 - (t / trail_len) * 0.40)

                char = BRAILLE_FRAMES[
                    (char_offset + t + self._braille_offset) % len(BRAILLE_FRAMES)
                ]
                grid[row_y][col_idx] = (char, 160.0, 0.8, lightness)

        # Place "OpenDev" text with breathing effect
        breathe_lightness = 0.15 + 0.075 * (1.0 + math.sin(self._breathe_phase))
        for i, ch in enumerate(label):
            col_pos = label_col_start + i
            if 0 <= col_pos < width:
                if ch != " ":
                    grid[label_row][col_pos] = (ch, 160.0, 0.6, breathe_lightness)
                else:
                    grid[label_row][col_pos] = None  # Keep spaces empty

        # Convert grid to Rich Text
        result = Text()
        for row_idx, row in enumerate(grid):
            if row_idx > 0:
                result.append("\n")
            for cell in row:
                if cell is None:
                    result.append(" ")
                else:
                    ch, hue, sat, lit = cell
                    color_code = hsl_to_ansi256(hue, sat * fade, lit * fade)
                    result.append(ch, style=f"color({color_code})")

        return result

    def _get_border_style(self) -> str:
        """Get border style based on fade progress."""
        # Cycle border color with gradient
        hue = self.gradient_offset % 360
        saturation = 0.6 * self.fade_progress
        lightness = 0.5 * self.fade_progress + 0.15 * (1 - self.fade_progress)

        color_code = hsl_to_ansi256(hue, saturation, lightness)
        return f"color({color_code})"

    def render(self) -> RenderableType:
        """Render the animated welcome panel, adapting to available space."""
        from rich.console import Group

        available_height = self.size.height if self.size.height > 0 else 30

        # Tier 1: Extremely small -- bare text only
        if available_height < 5:
            return Align.center(self._render_welcome_text())

        welcome_text = self._render_welcome_text()

        # Tier 2: Small -- panel with text, no rain
        rain_width, rain_height = self._calculate_rain_size()
        if rain_height == 0 or available_height < 11:
            panel = Panel(
                Align.center(welcome_text),
                border_style=self._get_border_style(),
                padding=(0, 2),
            )
            return Align.center(panel)

        # Tier 3: Normal -- full rain + text
        self._ensure_rain_field(rain_width, rain_height)
        rain = self._render_rain_field()

        content = Group(
            Align.center(rain),
            Text(""),
            Align.center(welcome_text),
        )

        panel = Panel(
            content,
            border_style=self._get_border_style(),
            padding=(0, 2),
        )

        # Minimal vertical centering -- only when genuinely spare space exists
        content_height = rain_height + len(self._content_lines) + 4
        remaining = available_height - content_height
        if remaining > 2:
            vertical_padding = remaining // 3
            from rich.text import Text as RichText

            top_padding = RichText("\n" * vertical_padding)
            return Align.center(Group(top_padding, panel))

        return Align.center(panel)

    def watch_gradient_offset(self, _: int) -> None:
        """React to gradient offset changes."""
        self.refresh()

    def watch_fade_progress(self, _: float) -> None:
        """React to fade progress changes."""
        self.refresh()
