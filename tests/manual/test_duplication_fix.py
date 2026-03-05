#!/usr/bin/env python3
"""Test to verify assistant message duplication is fixed."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from swecli.ui_textual.runner import TextualRunner
from swecli.models.message import Role

def test_no_duplication():
    """Test that assistant messages are not duplicated in Textual UI."""
    print("=" * 80)
    print("Testing Assistant Message Duplication Fix")
    print("=" * 80)

    # Create runner
    runner = TextualRunner(working_dir=Path.cwd())

    # Process a test query
    test_query = "hello"
    print(f"\n📝 Processing query: '{test_query}'")

    # Get session before
    session_before = runner.session_manager.get_current_session()
    messages_before = len(session_before.messages) if session_before else 0

    # Process query
    new_messages = runner._run_query(test_query)

    # Get session after
    session_after = runner.session_manager.get_current_session()
    messages_after = len(session_after.messages) if session_after else 0

    print(f"\n📊 Results:")
    print(f"   Messages before: {messages_before}")
    print(f"   Messages after: {messages_after}")
    print(f"   New messages: {len(new_messages)}")

    # Count assistant messages
    assistant_messages = [msg for msg in new_messages if msg.role == Role.ASSISTANT]
    user_messages = [msg for msg in new_messages if msg.role == Role.USER]

    print(f"\n📨 Message breakdown:")
    print(f"   User messages: {len(user_messages)}")
    print(f"   Assistant messages: {len(assistant_messages)}")

    # Verify expectations
    print(f"\n✅ Verification:")

    # Should have exactly 1 user message
    if len(user_messages) == 1:
        print(f"   ✅ Correct: 1 user message")
    else:
        print(f"   ❌ FAIL: Expected 1 user message, got {len(user_messages)}")
        return False

    # Should have exactly 1 assistant message
    if len(assistant_messages) == 1:
        print(f"   ✅ Correct: 1 assistant message")
    else:
        print(f"   ❌ FAIL: Expected 1 assistant message, got {len(assistant_messages)}")
        return False

    # Show the assistant message
    if assistant_messages:
        print(f"\n💬 Assistant message:")
        print(f"   {assistant_messages[0].content}")

    # Now simulate what _render_responses does
    print(f"\n🎨 Simulating UI rendering:")
    assistant_count = 0
    for msg in new_messages:
        if msg.role == Role.ASSISTANT:
            assistant_count += 1
            print(f"   Rendering assistant message #{assistant_count}")

    if assistant_count == 1:
        print(f"   ✅ Would render exactly 1 assistant message (no duplication!)")
    else:
        print(f"   ❌ FAIL: Would render {assistant_count} assistant messages (DUPLICATION!)")
        return False

    print(f"\n✅ ALL TESTS PASSED - No duplication detected!")
    return True

if __name__ == "__main__":
    success = test_no_duplication()
    print("\n" + "=" * 80)
    if success:
        print("SUCCESS: Duplication fix verified! ✅")
    else:
        print("FAILURE: Duplication still exists! ❌")
    print("=" * 80)
    sys.exit(0 if success else 1)
