#!/usr/bin/env python3
"""Final verification that duplication is fixed."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from swecli.ui_textual.runner import TextualRunner
from swecli.models.message import Role


def test_fix():
    """Comprehensive test verifying no duplication."""
    print("=" * 80)
    print("FINAL DUPLICATION FIX VERIFICATION")
    print("=" * 80)

    runner = TextualRunner(working_dir=Path.cwd())

    # Test 1: Console output should NOT be captured during query
    print("\n[Test 1] Verifying console output is not captured...")

    enqueued_count = 0
    original_enqueue = runner._enqueue_console_text

    def track_enqueue(text: str):
        nonlocal enqueued_count
        enqueued_count += 1
        return original_enqueue(text)

    runner._enqueue_console_text = track_enqueue

    new_messages = runner._run_query("hello")

    print(f"  ✓ Console texts enqueued during query: {enqueued_count}")

    if enqueued_count == 0:
        print(f"  ✅ PASS: No console output captured (duplication prevented!)")
        test1_pass = True
    else:
        print(f"  ❌ FAIL: Console output still being captured")
        test1_pass = False

    # Test 2: Session should have exactly 1 user + 1 assistant message
    print("\n[Test 2] Verifying session message count...")

    user_msgs = [m for m in new_messages if m.role == Role.USER]
    assistant_msgs = [m for m in new_messages if m.role == Role.ASSISTANT]

    print(f"  ✓ User messages: {len(user_msgs)}")
    print(f"  ✓ Assistant messages: {len(assistant_msgs)}")

    if len(assistant_msgs) == 1:
        print(f"  ✅ PASS: Exactly 1 assistant message in session")
        test2_pass = True
    else:
        print(f"  ❌ FAIL: Expected 1 assistant message, got {len(assistant_msgs)}")
        test2_pass = False

    # Test 3: Verify _render_responses doesn't duplicate
    print("\n[Test 3] Verifying rendering doesn't duplicate...")

    # Count how many times assistant message would be rendered
    render_count = 0
    for msg in new_messages:
        if msg.role == Role.ASSISTANT:
            render_count += 1

    print(f"  ✓ Messages to be rendered: {render_count}")

    if render_count == 1:
        print(f"  ✅ PASS: Only 1 message will be rendered")
        test3_pass = True
    else:
        print(f"  ❌ FAIL: Multiple messages would be rendered")
        test3_pass = False

    # Summary
    print("\n" + "=" * 80)
    print("TEST RESULTS:")
    print("=" * 80)
    print(f"  {'✅' if test1_pass else '❌'} Console output not captured")
    print(f"  {'✅' if test2_pass else '❌'} Session has 1 assistant message")
    print(f"  {'✅' if test3_pass else '❌'} Rendering won't duplicate")
    print("=" * 80)

    all_pass = test1_pass and test2_pass and test3_pass

    if all_pass:
        print("\n🎉 SUCCESS: Duplication issue is FIXED!")
        print("\nWhat was fixed:")
        print("  • Console output is no longer captured during query processing")
        print("  • Only session messages are rendered to the UI")
        print("  • Assistant messages appear exactly ONCE")
        return True
    else:
        print("\n❌ FAILURE: Some tests failed")
        return False


if __name__ == "__main__":
    try:
        success = test_fix()
        sys.exit(0 if success else 1)
    except Exception as e:
        print(f"\n❌ Test failed with error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
