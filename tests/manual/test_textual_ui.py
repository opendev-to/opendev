#!/usr/bin/env python3
"""Test launcher for Textual UI POC.

This script demonstrates the new Textual-based full-screen UI for OpenDev.

Usage:
    python test_textual_ui.py

Features tested:
    - Full-screen terminal takeover (like Crush)
    - Scrollable conversation log
    - Input area at bottom
    - Color-coded messages
    - Tool call formatting
    - Keyboard shortcuts (Ctrl+L, Ctrl+C, ESC)
    - Status bar with mode/context info
"""

import sys
from pathlib import Path

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent))

from swecli.ui_textual.chat_app import create_chat_app


def main():
    """Run the Textual UI POC."""
    # Don't print anything before running the app - it interferes with full-screen mode!
    # The app will take over the entire terminal using alternate screen buffer.

    # Optional: Add a callback to see message processing
    def on_message(text: str):
        # In the real integration, this would call the REPL agent
        pass

    # Create and run the app
    app = create_chat_app(on_message=on_message)

    try:
        # Run in application mode (full screen with alternate screen buffer)
        # This is the default - do NOT pass inline=True
        app.run()
    except KeyboardInterrupt:
        # Clean exit message will only show after app closes
        print("Exited cleanly.")
    except Exception as e:
        print(f"Error running app: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
