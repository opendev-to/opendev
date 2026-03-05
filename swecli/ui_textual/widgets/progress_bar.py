"""Pearl string progress bar - weight and size with color shift.

A minimalist string of centered dots where the wave is defined by mass.
Tiny pinprick dots swell into heavy spheres and shrink back down.
Like a snake swallowing a meal or a pearl sliding down a string.
Colors shift smoothly over time through a gradient.

The vibe: Precious, focused, poised.
Visual: · · ∘ ○ ● ○ ∘ · ·
"""

from __future__ import annotations

import colorsys
import math
from typing import TYPE_CHECKING, Any, Optional

from rich.text import Text
from textual.widgets import Static

from swecli.ui_textual.style_tokens import DIM_GREY

if TYPE_CHECKING:
    from textual.timer import Timer

# Configuration
BAR_WIDTH = 36  # Bar width
FRAME_INTERVAL_MS = 50  # ~20 fps
WAVE_SPEED = 0.8  # Pearl movement speed
WAVE_WIDTH = 3.5  # Width of the swell (lower = tighter)
COLOR_CYCLE_SPEED = 0.006  # How fast colors shift (lower = slower)


def hue_to_hex(hue: float, saturation: float = 0.7, value: float = 1.0) -> str:
    """Convert HSV to hex color string."""
    r, g, b = colorsys.hsv_to_rgb(hue, saturation, value)
    return f"#{int(r * 255):02x}{int(g * 255):02x}{int(b * 255):02x}"


class ProgressBar(Static):
    """Pearl string progress bar.

    A minimalist string of dots where a wave of "mass" travels
    through, causing dots to swell into spheres and shrink back.
    """

    def __init__(self, **kwargs) -> None:
        super().__init__("", **kwargs)
        self._animation_timer: Optional["Timer"] = None
        self._poll_timer: Optional["Timer"] = None
        self._wave_pos: float = 0.0
        self._hue: float = 0.0  # Current hue for color cycling
        self._visible: bool = False
        self._app: Any = None

    def set_app(self, app: Any) -> None:
        """Set the app reference to poll processing state."""
        self._app = app

    def on_mount(self) -> None:
        """Start polling for processing state."""
        self.display = False
        self._poll_timer = self.set_interval(0.1, self._poll_processing_state)

    def _poll_processing_state(self) -> None:
        """Check if app is processing and show/hide accordingly."""
        if self._app is None:
            return

        is_processing = getattr(self._app, "_is_processing", False)

        if is_processing and not self._visible:
            self._show()
        elif not is_processing and self._visible:
            self._hide()

    def _show(self) -> None:
        """Start the pearl animation."""
        if self._visible:
            return

        self._visible = True
        self._wave_pos = 0.0
        self.display = True

        self._animation_timer = self.set_interval(
            FRAME_INTERVAL_MS / 1000,
            self._on_frame,
            pause=False,
        )

        self._render_pearls()
        self.refresh()

    def _hide(self) -> None:
        """Stop the animation and hide."""
        if not self._visible:
            return

        self._visible = False

        if self._animation_timer is not None:
            self._animation_timer.stop()
            self._animation_timer = None

        self._wave_pos = 0.0
        self.update(" ")
        self.display = False

    def _on_frame(self) -> None:
        """Advance pearl wave position and color hue."""
        if not self._visible:
            return

        self._wave_pos = (self._wave_pos + WAVE_SPEED) % BAR_WIDTH
        self._hue = (self._hue + COLOR_CYCLE_SPEED) % 1.0
        self._render_pearls()

    def _render_pearls(self) -> None:
        """Render the pearl string with traveling swell."""
        result = Text()

        for i in range(BAR_WIDTH):
            char, color = self._get_pearl(i)
            result.append(char, style=color)

        self.update(result)

    def _get_pearl(self, position: int) -> tuple[str, str]:
        """Get pearl character and color based on distance from wave.

        Returns (character, color) representing the pearl size at this position.
        """
        # Distance from wave center (with wrapping)
        dist = min(
            abs(position - self._wave_pos),
            abs(position - self._wave_pos + BAR_WIDTH),
            abs(position - self._wave_pos - BAR_WIDTH),
        )

        # Smooth bell curve for the swell
        intensity = math.exp(-(dist**2) / WAVE_WIDTH)

        # Dynamic colors based on current hue
        bright_color = hue_to_hex(self._hue, saturation=0.75, value=1.0)
        mid_color = hue_to_hex(self._hue, saturation=0.5, value=0.8)

        # Map intensity to pearl size and color
        if intensity > 0.85:
            # Peak - largest, brightest
            return "●", bright_color
        elif intensity > 0.6:
            # Large hollow
            return "○", bright_color
        elif intensity > 0.35:
            # Medium hollow
            return "∘", mid_color
        elif intensity > 0.15:
            # Small dot, slightly visible
            return "·", mid_color
        else:
            # Resting state - tiny, dim
            return "·", DIM_GREY

    def on_unmount(self) -> None:
        """Clean up when widget is removed."""
        if self._poll_timer is not None:
            self._poll_timer.stop()
            self._poll_timer = None
        self._hide()


__all__ = ["ProgressBar"]
