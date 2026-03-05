"""Screenshot capture tool handler."""

from __future__ import annotations

import tempfile
from datetime import datetime
from pathlib import Path
from typing import Any, Optional

try:
    import mss
    MSS_AVAILABLE = True
except ImportError:
    MSS_AVAILABLE = False


class ScreenshotToolHandler:
    """Handles screenshot capture functionality."""

    def __init__(self, temp_dir: Optional[Path] = None):
        """Initialize screenshot handler.

        Args:
            temp_dir: Optional temporary directory for screenshots
        """
        self.temp_dir = temp_dir or Path(tempfile.gettempdir()) / "swecli_screenshots"
        self.temp_dir.mkdir(parents=True, exist_ok=True)

    def capture_screenshot(self, args: dict[str, Any]) -> dict[str, Any]:
        """Capture a screenshot and save to temporary location.

        Args:
            args: Dictionary containing optional parameters:
                - monitor: Monitor number to capture (default: 1 for primary)
                - region: Optional dict with x, y, width, height for partial capture

        Returns:
            Dictionary with success status, file path, and any errors
        """
        if not MSS_AVAILABLE:
            return {
                "success": False,
                "error": "Screenshot functionality requires 'mss' package. Install with: pip install mss",
                "path": None,
            }

        try:
            # Get parameters
            monitor_num = args.get("monitor", 1)
            region = args.get("region")

            # Generate filename with timestamp
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filename = f"screenshot_{timestamp}.png"
            filepath = self.temp_dir / filename

            # Capture screenshot
            with mss.mss() as sct:
                if region:
                    # Capture specific region
                    monitor = {
                        "left": region["x"],
                        "top": region["y"],
                        "width": region["width"],
                        "height": region["height"],
                    }
                else:
                    # Capture full monitor
                    monitor = sct.monitors[monitor_num]

                # Save screenshot
                screenshot = sct.grab(monitor)
                mss.tools.to_png(screenshot.rgb, screenshot.size, output=str(filepath))

            return {
                "success": True,
                "path": str(filepath),
                "output": f"Screenshot saved to: {filepath}\n\nYou can now reference this image in your queries by mentioning the path:\n@{filepath}",
                "error": None,
            }

        except IndexError:
            return {
                "success": False,
                "error": f"Monitor {monitor_num} not found. Available monitors: {len(mss.mss().monitors) - 1}",
                "path": None,
            }
        except Exception as exc:
            return {
                "success": False,
                "error": f"Failed to capture screenshot: {str(exc)}",
                "path": None,
            }
