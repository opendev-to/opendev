#!/usr/bin/env python3
"""Test launcher for Textual UI with explicit screen clearing."""

import os
import sys
from pathlib import Path

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent))

from swecli.ui_textual.chat_app import create_chat_app


def clear_terminal():
    """Explicitly clear terminal and move to top."""
    # Try multiple methods to ensure terminal is cleared

    # Method 1: os.system clear
    os.system('clear' if os.name != 'nt' else 'cls')

    # Method 2: ANSI escape codes
    # Clear screen and move cursor to top-left
    sys.stdout.write('\033[2J')  # Clear entire screen
    sys.stdout.write('\033[H')   # Move cursor to top-left
    sys.stdout.flush()

    # Method 3: Enter alternate screen buffer explicitly
    sys.stdout.write('\033[?1049h')  # Enter alternate screen
    sys.stdout.write('\033[2J')      # Clear it
    sys.stdout.write('\033[H')       # Move cursor to top
    sys.stdout.flush()


def restore_terminal():
    """Restore terminal to normal mode."""
    # Exit alternate screen buffer
    sys.stdout.write('\033[?1049l')
    sys.stdout.flush()


def main():
    """Run the Textual UI with explicit terminal clearing."""

    # Optional: Add a callback
    def on_message(text: str):
        pass

    try:
        # Clear terminal before starting
        clear_terminal()

        # Create and run the app
        app = create_chat_app(on_message=on_message)
        app.run()

        # Textual handles screen restoration automatically
        # Don't restore manually in finally block

    except KeyboardInterrupt:
        # User pressed Ctrl+C - this is normal
        pass
    except Exception as e:
        # Restore terminal first so we can see the error
        restore_terminal()
        print(f"\nError occurred: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)

    # Normal exit - Textual already restored the screen
    print("Exited cleanly.")


if __name__ == "__main__":
    main()
