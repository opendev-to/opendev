#!/usr/bin/env python
"""Unit tests for parallel agent display - no API calls."""
import sys
import time

sys.path.insert(0, "/Users/nghibui/codes/swe-cli")

from rich.text import Text
from swecli.ui_textual.widgets.conversation.tool_renderer import (
    DefaultToolRenderer,
    ParallelAgentGroup,
    AgentInfo,
)


class MockLog:
    """Mock log that tracks writes for testing."""

    def __init__(self):
        self.writes = []
        self.lines = []
        self._pending_spacing_line = None

    def write(self, *args, **kwargs):
        self.writes.append(args)
        # Add to lines for line_number tracking
        self.lines.append(args[0] if args else "")

    def set_timer(self, *args):
        return None

    def refresh_line(self, *args):
        pass

    def refresh(self):
        pass

    @property
    def virtual_size(self):
        class Size:
            width = 100

        return Size()


def test_collapsed_mode_skips_line_writes():
    """Verify collapsed mode only updates stats, doesn't write lines."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    # Activate parallel group in collapsed mode using agent_infos
    renderer._parallel_expanded = False
    tool_call_id = "call_123"
    renderer._parallel_group = ParallelAgentGroup(
        agents={tool_call_id: AgentInfo(
            agent_type="Explore",
            description="Explore documentation",
            tool_call_id=tool_call_id,
            line_number=1,
            status_line=2,
        )},
        header_line=0,
    )

    initial_writes = len(log.writes)

    # This should NOT write a line, only update stats
    # parent=tool_call_id matches the key in parallel group
    renderer.add_nested_tool_call(Text("list_files"), depth=1, parent=tool_call_id)

    assert (
        len(log.writes) == initial_writes
    ), f"Expected {initial_writes} writes in collapsed mode, got {len(log.writes)}"
    assert renderer._parallel_group.agents[tool_call_id].tool_count == 1
    assert "list_files" in renderer._parallel_group.agents[tool_call_id].current_tool
    print("✅ Collapsed mode skips line writes correctly")


def test_expanded_mode_writes_lines():
    """Verify expanded mode writes lines normally."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    # Activate parallel group in expanded mode using agent_infos
    renderer._parallel_expanded = True
    tool_call_id = "call_456"
    renderer._parallel_group = ParallelAgentGroup(
        agents={tool_call_id: AgentInfo(
            agent_type="Explore",
            description="Explore config",
            tool_call_id=tool_call_id,
            line_number=1,
            status_line=2,
        )},
        header_line=0,
    )

    initial_writes = len(log.writes)

    # This should write lines
    renderer.add_nested_tool_call(Text("search_code"), depth=1, parent=tool_call_id)

    assert (
        len(log.writes) > initial_writes
    ), f"Expected writes in expanded mode, got {len(log.writes) - initial_writes} new writes"
    assert renderer._parallel_group.agents[tool_call_id].tool_count == 1
    print("✅ Expanded mode writes lines correctly")


def test_default_is_collapsed():
    """Verify default state is collapsed."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    assert renderer._parallel_expanded is False, "Default should be collapsed (False)"
    print("✅ Default state is collapsed")


def test_on_parallel_agents_start_creates_group():
    """Verify on_parallel_agents_start creates a parallel group with agent infos."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    assert renderer._parallel_group is None

    # react_executor passes agent_infos with tool_call_id for individual tracking
    agent_infos = [
        {"agent_type": "Explore", "description": "Explore docs", "tool_call_id": "call_1"},
        {"agent_type": "Explore", "description": "Explore config", "tool_call_id": "call_2"},
    ]
    renderer.on_parallel_agents_start(agent_infos)

    assert renderer._parallel_group is not None
    assert len(renderer._parallel_group.agents) == 2
    assert "call_1" in renderer._parallel_group.agents
    assert "call_2" in renderer._parallel_group.agents
    assert renderer._parallel_group.agents["call_1"].description == "Explore docs"
    assert renderer._parallel_group.agents["call_2"].description == "Explore config"
    print("✅ on_parallel_agents_start creates parallel group with agent infos")


def test_on_parallel_agents_done_clears_group():
    """Verify on_parallel_agents_done clears the group."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    agent_infos = [
        {"agent_type": "Explore", "description": "Explore docs", "tool_call_id": "call_1"},
        {"agent_type": "Explore", "description": "Explore config", "tool_call_id": "call_2"},
    ]
    renderer.on_parallel_agents_start(agent_infos)
    assert renderer._parallel_group is not None

    # Simulate tool calls
    renderer._parallel_group.agents["call_1"].tool_count = 5
    renderer._parallel_group.agents["call_2"].tool_count = 3

    renderer.on_parallel_agents_done()

    assert renderer._parallel_group is None
    print("✅ on_parallel_agents_done clears parallel group")


def test_parallel_agent_complete_updates_status():
    """Verify on_parallel_agent_complete updates agent status."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    agent_infos = [
        {"agent_type": "Explore", "description": "Explore docs", "tool_call_id": "call_1"},
        {"agent_type": "Explore", "description": "Explore config", "tool_call_id": "call_2"},
    ]
    renderer.on_parallel_agents_start(agent_infos)

    assert renderer._parallel_group.agents["call_1"].status == "running"
    assert renderer._parallel_group.agents["call_2"].status == "running"

    # First agent completes successfully
    renderer.on_parallel_agent_complete("call_1", success=True)
    assert renderer._parallel_group.agents["call_1"].status == "completed"
    assert renderer._parallel_group.agents["call_2"].status == "running"

    # Second agent completes
    renderer.on_parallel_agent_complete("call_2", success=True)
    assert renderer._parallel_group.agents["call_2"].status == "completed"

    print("✅ on_parallel_agent_complete updates agent status")


def test_toggle_parallel_expansion():
    """Verify toggle_parallel_expansion toggles state correctly."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    assert renderer._parallel_expanded is False

    result = renderer.toggle_parallel_expansion()
    assert result is True
    assert renderer._parallel_expanded is True

    result = renderer.toggle_parallel_expansion()
    assert result is False
    assert renderer._parallel_expanded is False

    print("✅ toggle_parallel_expansion works correctly")


def test_has_active_parallel_group():
    """Verify has_active_parallel_group returns correct state."""
    log = MockLog()
    renderer = DefaultToolRenderer(log, app_callback_interface=None)

    assert renderer.has_active_parallel_group() is False

    agent_infos = [{"agent_type": "Explore", "description": "Test", "tool_call_id": "call_1"}]
    renderer.on_parallel_agents_start(agent_infos)
    assert renderer.has_active_parallel_group() is True

    renderer.on_parallel_agents_done()
    assert renderer.has_active_parallel_group() is False

    print("✅ has_active_parallel_group works correctly")


# --- UICallback Tests ---

class MockConversation:
    """Mock conversation log for UICallback testing."""

    def __init__(self):
        self.tool_calls = []
        self.single_agent_starts = []
        self.single_agent_completes = []

    def add_tool_call(self, text):
        self.tool_calls.append(text)

    def start_tool_execution(self):
        pass

    def on_parallel_agents_start(self, agent_names):
        pass

    def on_parallel_agents_done(self):
        pass

    def on_single_agent_start(self, agent_type, description, tool_call_id):
        self.single_agent_starts.append((agent_type, description, tool_call_id))

    def on_single_agent_complete(self, tool_call_id, success):
        self.single_agent_completes.append((tool_call_id, success))


class MockSpinnerService:
    """Mock spinner service for UICallback testing."""

    def __init__(self):
        self.starts = []

    def start(self, text, skip_placeholder=False):
        self.starts.append(text)
        return f"spinner_{len(self.starts)}"

    def stop(self, spinner_id, success, message=""):
        pass


class MockApp:
    """Mock Textual app for UICallback testing."""

    def __init__(self):
        self.spinner_service = MockSpinnerService()
        self.calls = []

    def call_from_thread(self, func, *args, **kwargs):
        self.calls.append((func, args, kwargs))
        # Actually call the function for testing
        func(*args, **kwargs)


def test_ui_callback_suppresses_spawn_subagent_in_parallel_mode():
    """Verify UICallback suppresses spawn_subagent display when parallel group is active."""
    from swecli.ui_textual.ui_callback import TextualUICallback

    conversation = MockConversation()
    app = MockApp()
    callback = TextualUICallback(conversation, chat_app=app)

    # Initially not in parallel mode
    assert callback._in_parallel_agent_group is False

    # Simulate parallel agents starting with agent_infos
    agent_infos = [
        {"agent_type": "Explore", "description": "Explore docs", "tool_call_id": "call_1"},
        {"agent_type": "Explore", "description": "Explore config", "tool_call_id": "call_2"},
    ]
    callback.on_parallel_agents_start(agent_infos)
    assert callback._in_parallel_agent_group is True

    # Now call on_tool_call for spawn_subagent - should be suppressed
    initial_starts = len(app.spinner_service.starts)
    callback.on_tool_call("spawn_subagent", {"subagent_type": "Explore"}, "tool_1")

    # Spinner should NOT have been started (call was suppressed)
    assert len(app.spinner_service.starts) == initial_starts, (
        f"spawn_subagent should be suppressed in parallel mode, "
        f"but spinner was started ({len(app.spinner_service.starts)} starts vs {initial_starts})"
    )

    print("✅ UICallback suppresses spawn_subagent in parallel mode")


def test_ui_callback_allows_spawn_subagent_outside_parallel_mode():
    """Verify UICallback calls on_single_agent_start for spawn_subagent when not in parallel mode."""
    from swecli.ui_textual.ui_callback import TextualUICallback

    conversation = MockConversation()
    app = MockApp()
    callback = TextualUICallback(conversation, chat_app=app)

    # Not in parallel mode
    assert callback._in_parallel_agent_group is False

    # Call on_tool_call for spawn_subagent - should call on_single_agent_start
    initial_starts = len(conversation.single_agent_starts)
    callback.on_tool_call("spawn_subagent", {"subagent_type": "Explore", "description": "Test task"}, "tool_1")

    # on_single_agent_start SHOULD have been called
    assert len(conversation.single_agent_starts) > initial_starts, (
        f"spawn_subagent should call on_single_agent_start when not in parallel mode"
    )

    # Flag should be set to suppress nested tool display
    assert callback._in_parallel_agent_group is True
    assert callback._current_single_agent_id == "tool_1"

    print("✅ UICallback calls on_single_agent_start for spawn_subagent outside parallel mode")


def test_ui_callback_parallel_flag_lifecycle():
    """Verify _in_parallel_agent_group flag is set/cleared correctly."""
    from swecli.ui_textual.ui_callback import TextualUICallback

    conversation = MockConversation()
    app = MockApp()
    callback = TextualUICallback(conversation, chat_app=app)

    # Initial state
    assert callback._in_parallel_agent_group is False

    # Start parallel agents with agent_infos
    agent_infos = [
        {"agent_type": "Agent", "description": "Agent 1", "tool_call_id": "call_1"},
        {"agent_type": "Agent", "description": "Agent 2", "tool_call_id": "call_2"},
    ]
    callback.on_parallel_agents_start(agent_infos)
    assert callback._in_parallel_agent_group is True

    # Complete parallel agents
    callback.on_parallel_agents_done()
    assert callback._in_parallel_agent_group is False

    print("✅ UICallback parallel flag lifecycle works correctly")


def test_ui_callback_suppresses_nested_tool_result_in_parallel_mode():
    """Verify UICallback suppresses nested tool result display when parallel group is active."""
    from swecli.ui_textual.ui_callback import TextualUICallback

    conversation = MockConversation()
    app = MockApp()
    callback = TextualUICallback(conversation, chat_app=app)

    # Start parallel agents with agent_infos - sets _in_parallel_agent_group = True
    agent_infos = [
        {"agent_type": "Explore", "description": "Explore docs", "tool_call_id": "call_1"},
        {"agent_type": "Explore", "description": "Explore config", "tool_call_id": "call_2"},
    ]
    callback.on_parallel_agents_start(agent_infos)
    assert callback._in_parallel_agent_group is True

    # Call on_nested_tool_result - should skip display but still collect
    initial_calls = len(app.calls)
    callback.on_nested_tool_result(
        "list_files",
        {"path": "."},
        {"success": True, "output": "file1.py\nfile2.py"},
        depth=1,
        parent="call_1",  # Use tool_call_id as parent
    )

    # Should have collected the call for session storage
    assert len(callback._pending_nested_calls) == 1
    assert callback._pending_nested_calls[0].name == "list_files"

    # But NO UI calls should have been made (display suppressed)
    # The only calls should be from on_parallel_agents_start
    assert len([c for c in app.calls if 'complete_nested_tool_call' in str(c)]) == 0, (
        "complete_nested_tool_call should not be called in parallel mode"
    )

    print("✅ UICallback suppresses nested tool result in parallel mode")


def run_all_tests():
    """Run all unit tests."""
    print("=== Unit Tests: Parallel Agent Display ===\n")

    # Tool renderer tests
    test_default_is_collapsed()
    test_collapsed_mode_skips_line_writes()
    test_expanded_mode_writes_lines()
    test_on_parallel_agents_start_creates_group()
    test_on_parallel_agents_done_clears_group()
    test_parallel_agent_complete_updates_status()
    test_toggle_parallel_expansion()
    test_has_active_parallel_group()

    # UICallback tests
    print("\n--- UICallback Tests ---")
    test_ui_callback_parallel_flag_lifecycle()
    test_ui_callback_suppresses_spawn_subagent_in_parallel_mode()
    test_ui_callback_allows_spawn_subagent_outside_parallel_mode()
    test_ui_callback_suppresses_nested_tool_result_in_parallel_mode()

    print("\n=== All tests passed! ===")


if __name__ == "__main__":
    run_all_tests()
