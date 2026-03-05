#!/usr/bin/env python3
"""Test script to verify runner integration without launching interactive UI."""

import os
import sys
from pathlib import Path

# Add the project to path
sys.path.insert(0, str(Path(__file__).parent))

from swecli.ui_textual.runner import TextualRunner
from swecli.core.runtime import ConfigManager
from swecli.core.context_engineering.history.session_manager import SessionManager

def test_runner_initialization():
    """Test that the runner initializes properly."""
    print("=" * 80)
    print("Testing TextualRunner Initialization")
    print("=" * 80)

    try:
        # Create runner
        runner = TextualRunner(working_dir=Path.cwd())

        print("\n✅ Runner created successfully")
        print(f"   Config loaded: {runner.config is not None}")
        print(f"   REPL initialized: {runner.repl is not None}")
        print(f"   Agent initialized: {hasattr(runner.repl, 'agent') and runner.repl.agent is not None}")
        print(f"   Query processor initialized: {hasattr(runner.repl, 'query_processor') and runner.repl.query_processor is not None}")
        print(f"   Tool registry initialized: {hasattr(runner.repl, 'tool_registry') and runner.repl.tool_registry is not None}")

        # Check configuration
        print("\n📋 Configuration:")
        print(f"   Model Provider: {runner.config.model_provider}")
        print(f"   Model: {runner.config.model}")
        print(f"   Working Directory: {runner.working_dir}")

        # Check agent
        if hasattr(runner.repl, 'agent') and runner.repl.agent:
            print(f"\n🤖 Agent:")
            print(f"   Type: {type(runner.repl.agent).__name__}")
            print(f"   Has call_llm method: {hasattr(runner.repl.agent, 'call_llm')}")

        # Check query processor
        if hasattr(runner.repl, 'query_processor') and runner.repl.query_processor:
            print(f"\n🔄 Query Processor:")
            print(f"   Type: {type(runner.repl.query_processor).__name__}")
            print(f"   Has process_query method: {hasattr(runner.repl.query_processor, 'process_query')}")

        # Check session
        session = runner.session_manager.get_current_session()
        if session:
            print(f"\n💾 Session:")
            print(f"   Session ID: {session.id}")
            print(f"   Message count: {len(session.messages)}")
            print(f"   Working directory: {session.working_directory}")

        # Test processing a simple query (without UI)
        print("\n" + "=" * 80)
        print("Testing Query Processing")
        print("=" * 80)

        test_query = "hello"
        print(f"\n📝 Processing query: '{test_query}'")

        # Get session before
        session_before = runner.session_manager.get_current_session()
        message_count_before = len(session_before.messages) if session_before else 0
        print(f"   Messages before: {message_count_before}")

        try:
            # This should trigger the REPL's _process_query
            new_messages = runner._run_query(test_query)

            # Get session after
            session_after = runner.session_manager.get_current_session()
            message_count_after = len(session_after.messages) if session_after else 0
            print(f"   Messages after: {message_count_after}")
            print(f"   New messages: {len(new_messages)}")

            if new_messages:
                print("\n✅ Query processed successfully!")
                print("\n📨 New messages:")
                for i, msg in enumerate(new_messages, 1):
                    print(f"   {i}. [{msg.role.value}] {msg.content[:100]}..." if len(msg.content) > 100 else f"   {i}. [{msg.role.value}] {msg.content}")
            else:
                print("\n⚠️  No new messages added to session")
                print("   This suggests the query processing may have failed")

        except Exception as e:
            print(f"\n❌ Error processing query: {e}")
            import traceback
            traceback.print_exc()

        print("\n" + "=" * 80)
        print("Test Complete")
        print("=" * 80)

    except Exception as e:
        print(f"\n❌ Error initializing runner: {e}")
        import traceback
        traceback.print_exc()
        return False

    return True

if __name__ == "__main__":
    success = test_runner_initialization()
    sys.exit(0 if success else 1)
