#!/usr/bin/env python3
"""Automated test for Textual UI POC - verifies structure without running UI."""

import sys
from pathlib import Path

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent))


def test_imports():
    """Test that all imports work."""
    print("Testing imports...")
    try:
        from swecli.ui_textual.chat_app import (
            SWECLIChatApp,
            ConversationLog,
            StatusBar,
            create_chat_app,
        )
        print("✓ All imports successful")
        return True
    except Exception as e:
        print(f"✗ Import failed: {e}")
        return False


def test_app_creation():
    """Test that app can be created."""
    print("\nTesting app creation...")
    try:
        from swecli.ui_textual.chat_app import create_chat_app

        app = create_chat_app()
        print(f"✓ App created: {app.__class__.__name__}")
        print(f"  - Title: {app.title}")
        print(f"  - Has bindings: {len(app.BINDINGS)} shortcuts")
        return True
    except Exception as e:
        print(f"✗ App creation failed: {e}")
        import traceback
        traceback.print_exc()
        return False


def test_widgets():
    """Test that widgets can be created."""
    print("\nTesting widgets...")
    try:
        from swecli.ui_textual.chat_app import ConversationLog, StatusBar

        # Create conversation log
        conv = ConversationLog(id="test-conv")
        print(f"✓ ConversationLog created")

        # Create status bar
        status = StatusBar()
        print(f"✓ StatusBar created")

        return True
    except Exception as e:
        print(f"✗ Widget creation failed: {e}")
        return False


def test_message_formatting():
    """Test message formatting methods."""
    print("\nTesting message formatting...")
    try:
        from swecli.ui_textual.chat_app import ConversationLog

        conv = ConversationLog(id="test")

        # These methods should exist and not crash
        print("  - User message formatting")
        conv.add_user_message("Test message")

        print("  - Assistant message formatting")
        conv.add_assistant_message("Test response")

        print("  - System message formatting")
        conv.add_system_message("Test system msg")

        print("  - Tool call formatting")
        conv.add_tool_call("Shell", "command='ls'")

        print("  - Tool result formatting")
        conv.add_tool_result("Success")

        print("  - Error formatting")
        conv.add_error("Test error")

        print("✓ All message formatting methods work")
        return True
    except Exception as e:
        print(f"✗ Message formatting failed: {e}")
        return False


def main():
    """Run all tests."""
    print("=" * 60)
    print("TEXTUAL UI POC - AUTOMATED TESTS")
    print("=" * 60)

    tests = [
        test_imports,
        test_app_creation,
        test_widgets,
        test_message_formatting,
    ]

    results = []
    for test in tests:
        result = test()
        results.append(result)

    print("\n" + "=" * 60)
    print("TEST RESULTS")
    print("=" * 60)

    passed = sum(results)
    total = len(results)

    for i, (test, result) in enumerate(zip(tests, results), 1):
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"{status} - {test.__name__}")

    print(f"\nTotal: {passed}/{total} tests passed")

    if passed == total:
        print("\n✓ All tests passed! The POC structure is correct.")
        print("\nNext step: Run the interactive test:")
        print("  python test_textual_ui.py")
        print("\nThis will launch the full-screen UI.")
        print("Use Ctrl+C to exit when done.")
        return 0
    else:
        print("\n✗ Some tests failed. Fix issues before running interactive test.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
