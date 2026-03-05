"""Debug logger that writes to a file for debugging Textual apps.

Textual captures stderr, so we need to write to a file directly.
"""

import os
from datetime import datetime
from pathlib import Path

# Debug log file path
DEBUG_LOG_PATH = Path("/tmp/swecli-interrupt-debug.log")

def debug_log(component: str, message: str) -> None:
    """Write a debug message to the log file.

    Args:
        component: Name of the component (e.g., "ChatApp", "InterruptManager")
        message: The debug message
    """
    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
    line = f"[{timestamp}] [{component}] {message}\n"

    # Append to file
    with open(DEBUG_LOG_PATH, "a") as f:
        f.write(line)

def clear_debug_log() -> None:
    """Clear the debug log file."""
    if DEBUG_LOG_PATH.exists():
        DEBUG_LOG_PATH.unlink()
    # Create empty file
    DEBUG_LOG_PATH.touch()
