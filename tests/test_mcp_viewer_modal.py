"""Tests for MCPViewerModal."""
from unittest.mock import Mock, patch
from prompt_toolkit.keys import Keys
from swecli.ui_textual.modals.mcp_viewer_modal import MCPViewerModal
from prompt_toolkit.formatted_text import to_formatted_text

def test_mcp_viewer_modal_initialization():
    """Test initialization of MCPViewerModal."""
    modal = MCPViewerModal(
        server_name="test-server",
        server_config={"command": "npx", "args": ["-y", "server"]},
        is_connected=True,
        tools=[{"name": "tool1", "description": "desc1"}],
        capabilities=["resources", "tools"],
        config_location="/tmp/config.json",
        mcp_manager=Mock()
    )

    assert modal.server_name == "test-server"
    assert modal.is_connected is True
    assert len(modal.tools) == 1
    assert modal.view_mode == "server"
    assert modal.selected_option == 0

def test_render_server_view():
    """Test rendering of server view."""
    modal = MCPViewerModal(
        server_name="test-server",
        server_config={"command": "npx", "args": ["-y", "server"]},
        is_connected=True,
        tools=[{"name": "tool1"}, {"name": "tool2"}],
        capabilities=["tools"],
        config_location="/path/to/config",
    )

    content = modal._render_server_view()
    text = "".join([fragment[1] for fragment in to_formatted_text(content)])

    assert "test-server" in text
    assert "✔ connected" in text
    assert "Command: npx" in text
    assert "Args: -y server" in text
    assert "Capabilities: tools" in text
    assert "Tools: 2 tools" in text
    assert "1. View tools" in text
    assert "2. Reconnect" in text
    assert "3. Disable" in text

def test_render_tools_view():
    """Test rendering of tools view."""
    tools = [{"name": f"tool{i}", "description": f"desc{i}"} for i in range(25)]
    modal = MCPViewerModal(
        server_name="test-server",
        server_config={},
        is_connected=True,
        tools=tools,
        capabilities=[],
        config_location="",
    )

    modal.view_mode = "tools"

    content = modal._render_tools_view()
    text = "".join([fragment[1] for fragment in to_formatted_text(content)])

    assert "Tools for test-server (25 tools)" in text
    assert "1.  tool0" in text
    assert "20.  tool19" in text
    # Should show scroll indicator
    assert "more below" in text

def test_navigation_logic():
    """Test navigation logic via simulated key bindings."""
    modal = MCPViewerModal(
        server_name="test",
        server_config={"enabled": True},
        is_connected=True,
        tools=[{"name": "t1"}],
        capabilities=[],
        config_location="",
    )

    kb = modal.create_key_bindings()

    # Mock event
    mock_event = Mock()
    mock_event.app.exit = Mock()
    mock_event.key_sequence = [Mock(key="enter")]

    # Test moving down in server view
    down_handler = kb.get_bindings_for_keys(("down",))[0].handler

    # Initial: option 0
    assert modal.selected_option == 0

    # Move down -> option 1
    down_handler(mock_event)
    assert modal.selected_option == 1

    # Move down -> option 2
    down_handler(mock_event)
    assert modal.selected_option == 2

    # Move down -> stay at 2 (max options is 2: 0,1,2)
    down_handler(mock_event)
    assert modal.selected_option == 2

    # Test selection (Enter on option 2: Disable)
    # Find handler for 'enter' (ControlM)
    enter_binding = next(b for b in kb.bindings if b.keys == (Keys.ControlM,))
    enter_handler = enter_binding.handler

    enter_handler(mock_event)
    assert modal.result == "disable"
    mock_event.app.exit.assert_called_once()

def test_switch_to_tools_and_back():
    """Test switching between server and tools view."""
    modal = MCPViewerModal(
        server_name="test",
        server_config={},
        is_connected=True,
        tools=[{"name": "t1"}],
        capabilities=[],
        config_location="",
    )

    kb = modal.create_key_bindings()
    mock_event = Mock()
    mock_event.key_sequence = [Mock(key="enter")]

    # Select "View tools" (option 0)
    enter_binding = next(b for b in kb.bindings if b.keys == (Keys.ControlM,))
    enter_handler = enter_binding.handler
    enter_handler(mock_event)

    assert modal.view_mode == "tools"
    assert modal.selected_option == 0

    # Press Escape to go back
    esc_binding = next(b for b in kb.bindings if b.keys == (Keys.Escape,))
    esc_handler = esc_binding.handler
    esc_handler(mock_event)

    assert modal.view_mode == "server"
    assert modal.selected_option == 0
