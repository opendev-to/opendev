#!/usr/bin/env python3
"""Test the full message flow including rendering."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from swecli.ui_textual.runner import TextualRunner
from swecli.models.message import Role

def test_full_flow():
    """Test the complete message processing flow."""
    print("=" * 80, file=sys.stdout)
    print("Testing Full Message Flow with Debug Logging", file=sys.stdout)
    print("=" * 80, file=sys.stdout)
    print("(Debug logs will appear in stderr)", file=sys.stdout)
    print("=" * 80, file=sys.stdout)

    # Create runner
    runner = TextualRunner(working_dir=Path.cwd())

    # Process query through the FULL pipeline (not just REPL)
    test_query = "hello"
    print(f"\n📝 Processing query: '{test_query}'", file=sys.stdout)

    # This is what the UI does
    new_messages = runner._run_query(test_query)

    print(f"\n📊 Got {len(new_messages)} new messages from session", file=sys.stdout)

    # Now render them (this is what _process_messages does)
    print(f"\n🎨 Calling _render_responses...", file=sys.stdout)
    runner._render_responses(new_messages)

    print(f"\n✅ Test complete! Check stderr for debug logs.", file=sys.stdout)

if __name__ == "__main__":
    test_full_flow()
