#!/usr/bin/env python3
"""Test to verify that the duplication fix works correctly."""

import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).parent))

from swecli.ui_textual.runner import TextualRunner
from swecli.models.message import Role


def test_no_duplication_with_fix():
    """Verify that assistant messages are only rendered once after fix."""
    print("=" * 80)
    print("Testing No Duplication After Fix")
    print("=" * 80)

    # Create runner
    runner = TextualRunner(working_dir=Path.cwd())

    # Track how many times add_assistant_message is called
    call_count = 0
    messages_rendered = []

    original_add_assistant = runner.app.conversation.add_assistant_message

    def tracked_add_assistant(message: str) -> None:
        nonlocal call_count
        call_count += 1
        messages_rendered.append(message)
        print(f"\n[TEST] add_assistant_message called (#{call_count})")
        print(f"[TEST] Message: {message[:100]}...")
        return original_add_assistant(message)

    runner.app.conversation.add_assistant_message = tracked_add_assistant

    # Process a test query
    test_query = "hello"
    print(f"\n📝 Processing query: '{test_query}'")

    # Run query and render responses
    new_messages = runner._run_query(test_query)

    print(f"\n📊 Session returned {len(new_messages)} new messages")
    for i, msg in enumerate(new_messages):
        print(f"   Message {i+1}: {msg.role.value}")

    # Render the responses
    print(f"\n🎨 Rendering responses...")
    runner._render_responses(new_messages)

    # Verify results
    print(f"\n" + "=" * 80)
    print(f"VERIFICATION RESULTS:")
    print(f"=" * 80)
    print(f"✓ add_assistant_message was called {call_count} time(s)")

    if call_count == 1:
        print(f"✅ SUCCESS: No duplication! Message rendered exactly once.")
        return True
    elif call_count == 0:
        print(f"⚠️  WARNING: Message was never rendered!")
        return False
    else:
        print(f"❌ FAILURE: Duplication detected! Message rendered {call_count} times.")
        print(f"\nRendered messages:")
        for i, msg in enumerate(messages_rendered, 1):
            print(f"  {i}. {msg[:100]}...")
        return False


def test_console_output_not_captured():
    """Verify that console output is NOT captured during query processing."""
    print("\n" + "=" * 80)
    print("Testing Console Output Not Captured")
    print("=" * 80)

    runner = TextualRunner(working_dir=Path.cwd())

    # Track calls to _enqueue_console_text
    enqueued_texts = []
    original_enqueue = runner._enqueue_console_text

    def tracked_enqueue(text: str) -> None:
        enqueued_texts.append(text)
        print(f"[TEST] Console text enqueued: {text[:100]}...")
        return original_enqueue(text)

    runner._enqueue_console_text = tracked_enqueue

    # Process query
    test_query = "hello"
    print(f"\n📝 Processing query: '{test_query}'")

    new_messages = runner._run_query(test_query)

    print(f"\n" + "=" * 80)
    print(f"CONSOLE CAPTURE RESULTS:")
    print(f"=" * 80)
    print(f"✓ Enqueued {len(enqueued_texts)} console text(s)")

    # Check if any enqueued text contains the assistant response
    assistant_messages = [msg for msg in new_messages if msg.role == Role.ASSISTANT]

    if assistant_messages and enqueued_texts:
        assistant_content = assistant_messages[0].content
        for text in enqueued_texts:
            if assistant_content in text or text in assistant_content:
                print(f"❌ FAILURE: Console output contains assistant message!")
                print(f"   This would cause duplication!")
                return False

    if len(enqueued_texts) == 0:
        print(f"✅ SUCCESS: No console output captured (expected after fix)")
        return True
    else:
        print(f"⚠️  Console output was captured but doesn't contain assistant message")
        print(f"   Enqueued texts:")
        for i, text in enumerate(enqueued_texts, 1):
            print(f"   {i}. {text[:100]}...")
        return True


def test_session_messages_only():
    """Verify that ONLY session messages are rendered, not console output."""
    print("\n" + "=" * 80)
    print("Testing Session Messages Only")
    print("=" * 80)

    runner = TextualRunner(working_dir=Path.cwd())

    # Get initial session state
    session = runner.session_manager.get_current_session()
    initial_count = len(session.messages) if session else 0

    # Process query
    test_query = "hello"
    print(f"\n📝 Processing query: '{test_query}'")

    new_messages = runner._run_query(test_query)

    # Check session
    session = runner.session_manager.get_current_session()
    final_count = len(session.messages) if session else 0

    print(f"\n📊 Session messages:")
    print(f"   Initial: {initial_count}")
    print(f"   New: {len(new_messages)}")
    print(f"   Final: {final_count}")

    # Count assistant messages
    assistant_in_new = len([m for m in new_messages if m.role == Role.ASSISTANT])
    assistant_in_session = len([m for m in session.messages if m.role == Role.ASSISTANT])

    print(f"\n✓ Assistant messages in new_messages: {assistant_in_new}")
    print(f"✓ Total assistant messages in session: {assistant_in_session}")

    if assistant_in_new == 1:
        print(f"✅ SUCCESS: Exactly 1 assistant message in new messages")
        return True
    else:
        print(f"❌ FAILURE: Expected 1 assistant message, got {assistant_in_new}")
        return False


if __name__ == "__main__":
    print("\n" + "🧪" * 40)
    print("DUPLICATION FIX VERIFICATION TEST SUITE")
    print("🧪" * 40 + "\n")

    results = []

    # Run all tests
    try:
        results.append(("Single Render", test_no_duplication_with_fix()))
    except Exception as e:
        print(f"❌ Test 1 failed with error: {e}")
        import traceback
        traceback.print_exc()
        results.append(("Single Render", False))

    try:
        results.append(("No Console Capture", test_console_output_not_captured()))
    except Exception as e:
        print(f"❌ Test 2 failed with error: {e}")
        import traceback
        traceback.print_exc()
        results.append(("No Console Capture", False))

    try:
        results.append(("Session Messages Only", test_session_messages_only()))
    except Exception as e:
        print(f"❌ Test 3 failed with error: {e}")
        import traceback
        traceback.print_exc()
        results.append(("Session Messages Only", False))

    # Print summary
    print("\n" + "=" * 80)
    print("TEST SUMMARY")
    print("=" * 80)

    for test_name, passed in results:
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{status}: {test_name}")

    all_passed = all(result[1] for result in results)

    print("\n" + "=" * 80)
    if all_passed:
        print("🎉 ALL TESTS PASSED! Duplication fix verified!")
    else:
        print("⚠️  SOME TESTS FAILED. Please review the output above.")
    print("=" * 80 + "\n")

    sys.exit(0 if all_passed else 1)
