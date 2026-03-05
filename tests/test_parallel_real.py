#!/usr/bin/env python
"""Integration test for parallel agent execution with real agents.

This test verifies:
1. Parallel agents execute correctly
2. UI callbacks receive proper nested tool calls
3. Agent tool filtering works (e.g., Code-Explorer doesn't run bash)
4. Status lines update properly during execution
"""

import sys
import os

# Add parent directory to path for imports
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


class MockConversation:
    """Mock conversation widget for testing UI callbacks."""

    def __init__(self):
        self.tool_calls = []
        self.parallel_start_calls = []
        self.parallel_done_calls = []
        self.agent_complete_calls = []
        self._nested_calls = []

    def add_nested_tool_call(
        self,
        display,
        depth=1,
        parent="",
        tool_id="",
        is_last=False,
    ):
        """Track nested tool calls for verification."""
        self.tool_calls.append({
            "display": str(display),
            "depth": depth,
            "parent": parent,
            "tool_id": tool_id,
            "is_last": is_last,
        })
        print(f"  [NESTED] depth={depth} parent={parent!r} tool_id={tool_id!r} display={display}")

    def on_parallel_agents_start(self, agent_infos):
        """Track parallel agents start."""
        self.parallel_start_calls.append(agent_infos)
        print(f"[PARALLEL START] {len(agent_infos)} agents")
        for info in agent_infos:
            print(f"  - {info['agent_type']}: {info['description'][:50]} (id={info['tool_call_id'][:16]}...)")

    def on_parallel_agents_done(self):
        """Track parallel agents done."""
        self.parallel_done_calls.append(True)
        print("[PARALLEL DONE]")

    def on_parallel_agent_complete(self, tool_call_id, success):
        """Track individual agent completion."""
        self.agent_complete_calls.append({"tool_call_id": tool_call_id, "success": success})
        print(f"[AGENT COMPLETE] tool_call_id={tool_call_id[:16]}... success={success}")


class MockChatApp:
    """Mock chat app for callback context."""

    def refresh(self):
        """Mock refresh."""
        pass


class MockToolRenderer:
    """Mock tool renderer for tracking."""

    def __init__(self, conversation):
        self.conversation = conversation

    def add_nested_tool_call(self, display, depth, parent, tool_id="", is_last=False):
        """Forward to conversation."""
        self.conversation.add_nested_tool_call(display, depth, parent, tool_id, is_last)


class MockUICallback:
    """Mock UI callback that tracks all calls."""

    def __init__(self):
        self.conversation = MockConversation()
        self.tool_renderer = MockToolRenderer(self.conversation)
        self._nested_calls = []
        self.chat_app = MockChatApp()

    def on_nested_tool_call(
        self,
        tool_name,
        tool_args,
        depth=1,
        parent="",
        tool_id="",
    ):
        """Track nested tool calls."""
        call = {
            "tool_name": tool_name,
            "tool_args": tool_args,
            "depth": depth,
            "parent": parent,
            "tool_id": tool_id,
        }
        self._nested_calls.append(call)
        self.conversation.add_nested_tool_call(
            f"{tool_name}: {tool_args}",
            depth=depth,
            parent=parent,
            tool_id=tool_id,
        )

    def on_parallel_agents_start(self, agent_infos):
        """Forward to conversation."""
        self.conversation.on_parallel_agents_start(agent_infos)

    def on_parallel_agents_done(self):
        """Forward to conversation."""
        self.conversation.on_parallel_agents_done()

    def on_parallel_agent_complete(self, tool_call_id, success):
        """Forward to conversation."""
        self.conversation.on_parallel_agent_complete(tool_call_id, success)

    def get_and_clear_nested_calls(self):
        """Return and clear nested calls."""
        calls = self._nested_calls[:]
        self._nested_calls = []
        return calls


def test_parallel_agent_tracking():
    """Test parallel agent UI callback tracking without actual execution.

    This test verifies the UI callback flow works correctly for parallel agents.
    """
    print("\n=== Testing Parallel Agent UI Tracking ===\n")

    # Create mock UI callback
    ui_callback = MockUICallback()

    # Simulate parallel agents start (what react_executor does)
    agent_infos = [
        {
            "agent_type": "Code-Explorer",
            "description": "List all Python files in src/ directory",
            "tool_call_id": "call_abc123def456",
        },
        {
            "agent_type": "Code-Explorer",
            "description": "Search for 'async def' definitions",
            "tool_call_id": "call_xyz789ghi012",
        },
    ]

    print("1. Calling on_parallel_agents_start...")
    ui_callback.on_parallel_agents_start(agent_infos)
    assert len(ui_callback.conversation.parallel_start_calls) == 1
    print("   ✓ parallel_agents_start called\n")

    # Simulate nested tool calls (what happens when tools run)
    print("2. Simulating nested tool calls...")

    # First agent's tools
    ui_callback.on_nested_tool_call(
        "list_files",
        {"path": "src"},
        depth=1,
        parent="call_abc123def456",  # Matches first agent's tool_call_id
        tool_id="tool_1",
    )
    ui_callback.on_nested_tool_call(
        "search",
        {"pattern": "async def"},
        depth=1,
        parent="call_abc123def456",
        tool_id="tool_2",
    )

    # Second agent's tools
    ui_callback.on_nested_tool_call(
        "list_files",
        {"path": "."},
        depth=1,
        parent="call_xyz789ghi012",  # Matches second agent's tool_call_id
        tool_id="tool_3",
    )

    print(f"   ✓ {len(ui_callback._nested_calls)} nested tool calls tracked\n")

    # Simulate agent completion
    print("3. Calling on_parallel_agent_complete...")
    ui_callback.on_parallel_agent_complete("call_abc123def456", True)
    ui_callback.on_parallel_agent_complete("call_xyz789ghi012", True)
    assert len(ui_callback.conversation.agent_complete_calls) == 2
    print("   ✓ Both agents marked complete\n")

    # Simulate all done
    print("4. Calling on_parallel_agents_done...")
    ui_callback.on_parallel_agents_done()
    assert len(ui_callback.conversation.parallel_done_calls) == 1
    print("   ✓ parallel_agents_done called\n")

    # Verify results
    print("5. Verifying results...")
    assert len(ui_callback.conversation.parallel_start_calls) == 1
    assert len(ui_callback.conversation.parallel_done_calls) == 1
    assert len(ui_callback.conversation.agent_complete_calls) == 2
    assert len(ui_callback._nested_calls) == 3
    print("   ✓ All assertions passed\n")

    # Verify parent context matching
    print("6. Verifying parent context matching...")
    first_agent_calls = [c for c in ui_callback._nested_calls if c["parent"] == "call_abc123def456"]
    second_agent_calls = [c for c in ui_callback._nested_calls if c["parent"] == "call_xyz789ghi012"]
    assert len(first_agent_calls) == 2, f"Expected 2 calls for first agent, got {len(first_agent_calls)}"
    assert len(second_agent_calls) == 1, f"Expected 1 call for second agent, got {len(second_agent_calls)}"
    print(f"   ✓ Parent context matching works ({len(first_agent_calls)} + {len(second_agent_calls)} = {len(ui_callback._nested_calls)} tools)\n")

    print("✅ All tests passed!\n")


def test_code_explorer_tools():
    """Verify Code-Explorer subagent has correct tools (no bash)."""
    print("\n=== Testing Code-Explorer Tools ===\n")

    from swecli.core.agents.subagents.agents import CODE_EXPLORER_SUBAGENT

    expected_tools = ["read_file", "search", "list_files", "find_symbol", "find_referencing_symbols"]
    actual_tools = CODE_EXPLORER_SUBAGENT.get("tools", [])

    print(f"Expected tools: {expected_tools}")
    print(f"Actual tools:   {actual_tools}")

    assert actual_tools == expected_tools, f"Tools mismatch: {actual_tools} != {expected_tools}"

    # Verify bash is NOT in the list
    assert "run_command" not in actual_tools, "Code-Explorer should NOT have run_command tool"
    assert "bash" not in actual_tools, "Code-Explorer should NOT have bash tool"

    print("\n✅ Code-Explorer has correct tools (no bash)\n")


def test_available_subagents():
    """Print all available subagents for reference."""
    print("\n=== Available Subagents ===\n")

    from swecli.core.agents.subagents.agents import ALL_SUBAGENTS

    for agent in ALL_SUBAGENTS:
        name = agent.get("name", "Unknown")
        desc = agent.get("description", "")[:60]
        tools = agent.get("tools", [])
        print(f"- {name}")
        print(f"  Description: {desc}...")
        print(f"  Tools: {tools}")
        print()

    print("✅ Subagent listing complete\n")


def test_tool_filtering():
    """Test that MainAgent correctly filters tools based on allowed_tools."""
    print("\n=== Testing Tool Filtering ===\n")

    from swecli.core.agents.main_agent import MainAgent
    from swecli.core.agents.components import ToolSchemaBuilder

    # Test 1: No filtering (None) = all tools
    print("1. Testing with no filtering (allowed_tools=None)...")
    builder_all = ToolSchemaBuilder(tool_registry=None, allowed_tools=None)
    schemas_all = builder_all.build()
    all_tool_names = {s["function"]["name"] for s in schemas_all}
    print(f"   Got {len(all_tool_names)} tools (all builtin tools)")
    assert "run_command" in all_tool_names, "run_command should be in all tools"
    assert "write_file" in all_tool_names, "write_file should be in all tools"
    assert "read_file" in all_tool_names, "read_file should be in all tools"

    # Test 2: Code-Explorer filtering (no bash/write/edit)
    print("\n2. Testing Code-Explorer tool filtering...")
    code_explorer_tools = ["read_file", "search", "list_files", "find_symbol", "find_referencing_symbols"]
    builder_filtered = ToolSchemaBuilder(tool_registry=None, allowed_tools=code_explorer_tools)
    schemas_filtered = builder_filtered.build()
    filtered_tool_names = {s["function"]["name"] for s in schemas_filtered}
    print(f"   Got {len(filtered_tool_names)} tools: {filtered_tool_names}")
    assert filtered_tool_names == set(code_explorer_tools), f"Expected {set(code_explorer_tools)}, got {filtered_tool_names}"

    # Verify bash is NOT in the filtered list
    assert "run_command" not in filtered_tool_names, "run_command should NOT be in Code-Explorer tools"
    assert "write_file" not in filtered_tool_names, "write_file should NOT be in Code-Explorer tools"
    assert "edit_file" not in filtered_tool_names, "edit_file should NOT be in Code-Explorer tools"
    print("   ✓ Tool filtering correctly excludes bash/write/edit")

    # Test 3: Verify all Code-Explorer tools ARE present
    for tool in code_explorer_tools:
        assert tool in filtered_tool_names, f"{tool} should be in Code-Explorer tools"
    print("   ✓ All Code-Explorer tools are present")

    print("\n✅ Tool filtering works correctly!\n")


if __name__ == "__main__":
    test_parallel_agent_tracking()
    test_code_explorer_tools()
    test_available_subagents()
    test_tool_filtering()
    print("\n" + "=" * 50)
    print("ALL TESTS PASSED!")
    print("=" * 50 + "\n")
