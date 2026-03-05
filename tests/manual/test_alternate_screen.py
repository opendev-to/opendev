#!/usr/bin/env python3
"""Test alternate screen buffer functionality.

This script tests if your terminal properly supports alternate screen buffer.
"""

import sys
import time


def test_ansi_alternate_screen():
    """Test raw ANSI escape codes for alternate screen."""
    print("=" * 60)
    print("TESTING ALTERNATE SCREEN BUFFER")
    print("=" * 60)
    print()
    print("You should see this text...")
    print("...and then it should disappear when we switch to alternate screen.")
    print()
    print("Press Enter to continue...")
    input()

    # Enter alternate screen buffer
    print("\033[?1049h", end="", flush=True)

    # Clear screen and move cursor to top
    print("\033[2J\033[H", end="", flush=True)

    # Draw content in alternate screen
    print("=" * 60)
    print("THIS IS THE ALTERNATE SCREEN BUFFER")
    print("=" * 60)
    print()
    print("If this is working correctly:")
    print("  ✓ You should NOT see the previous text")
    print("  ✓ This should fill the ENTIRE screen")
    print("  ✓ Previous terminal history should be hidden")
    print()
    print("Press Enter to exit alternate screen...")
    input()

    # Exit alternate screen buffer
    print("\033[?1049l", end="", flush=True)

    print("\nBack to normal screen.")
    print("The previous text should be visible again.")
    print()
    print("Did it work correctly? (y/n): ", end="", flush=True)
    response = input().lower()

    if response == 'y':
        print("\n✓ Great! Your terminal supports alternate screen buffer.")
        print("  Textual should work properly for full-screen display.")
    else:
        print("\n✗ Your terminal might not support alternate screen properly.")
        print("  Terminal tested: $TERM =", sys.platform)
        print("\n  Recommendations:")
        print("  - Try iTerm2 (macOS)")
        print("  - Try Alacritty (cross-platform)")
        print("  - Try modern Terminal.app (macOS)")
        print("  - Check your terminal's alternate screen buffer settings")


def test_textual_alternate_screen():
    """Test Textual's alternate screen handling."""
    print("\n" + "=" * 60)
    print("TESTING TEXTUAL ALTERNATE SCREEN")
    print("=" * 60)
    print()
    print("Now we'll test Textual's alternate screen mode...")
    print("Press Enter to launch Textual app...")
    input()

    # Import here to avoid issues if textual isn't installed
    try:
        from textual.app import App
        from textual.widgets import Static
    except ImportError:
        print("✗ Textual not installed. Install with: pip install textual")
        return

    class TestApp(App):
        """Minimal test app."""

        def compose(self):
            yield Static("Textual Alternate Screen Test\n\nIf this is full-screen, you should NOT see any previous terminal content.\n\nPress Ctrl+C to exit.")

    app = TestApp()

    try:
        app.run()
    except KeyboardInterrupt:
        pass

    print("\nDid Textual app take over the full screen? (y/n): ", end="", flush=True)
    response = input().lower()

    if response == 'y':
        print("\n✓ Excellent! Textual full-screen works correctly.")
        print("  The OpenDev UI should work perfectly!")
    else:
        print("\n✗ Textual might not be using alternate screen properly.")
        print("  This could be a terminal or configuration issue.")


def main():
    """Run all tests."""
    print("\n\nALTERNATE SCREEN BUFFER DIAGNOSTIC\n")
    print("This will help identify if your terminal supports full-screen apps.")
    print()

    # Test 1: Raw ANSI codes
    test_ansi_alternate_screen()

    # Test 2: Textual
    print("\nWould you like to test Textual as well? (y/n): ", end="", flush=True)
    if input().lower() == 'y':
        test_textual_alternate_screen()

    print("\n" + "=" * 60)
    print("DIAGNOSTIC COMPLETE")
    print("=" * 60)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n\nTest interrupted.")
        sys.exit(0)
