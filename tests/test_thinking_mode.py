"""Tests for Thinking Mode feature.

Tests the think tool and related components for capturing and displaying
model reasoning content.
"""

import pytest
from unittest.mock import MagicMock, patch

from swecli.core.context_engineering.tools.handlers.thinking_handler import (
    ThinkingHandler,
    ThinkingBlock,
)


class TestThinkingHandler:
    """Tests for ThinkingHandler state management."""

    def test_init_creates_empty_state(self):
        """Test handler initializes with empty state."""
        handler = ThinkingHandler()
        assert handler.block_count == 0
        assert handler.is_visible is True
        assert handler.get_all_thinking() == []

    def test_add_thinking_returns_success(self):
        """Test adding thinking content."""
        handler = ThinkingHandler()
        result = handler.add_thinking("Step 1: analyze...")

        assert result["success"] is True
        assert result["_thinking_content"] == "Step 1: analyze..."
        assert result["thinking_id"] == "think-1"
        assert result["output"] == "Step 1: analyze..."  # Included in message history for next LLM call

    def test_add_thinking_strips_whitespace(self):
        """Test that thinking content is stripped."""
        handler = ThinkingHandler()
        result = handler.add_thinking("  content with spaces  \n")

        assert result["_thinking_content"] == "content with spaces"

    def test_add_thinking_empty_fails(self):
        """Test that empty content fails."""
        handler = ThinkingHandler()

        result = handler.add_thinking("")
        assert result["success"] is False
        assert "empty" in result["error"].lower()

        result = handler.add_thinking("   ")
        assert result["success"] is False

        result = handler.add_thinking(None)
        assert result["success"] is False

    def test_add_thinking_increments_id(self):
        """Test that thinking IDs increment."""
        handler = ThinkingHandler()

        r1 = handler.add_thinking("First")
        r2 = handler.add_thinking("Second")
        r3 = handler.add_thinking("Third")

        assert r1["thinking_id"] == "think-1"
        assert r2["thinking_id"] == "think-2"
        assert r3["thinking_id"] == "think-3"

    def test_get_all_thinking_returns_blocks(self):
        """Test getting all thinking blocks."""
        handler = ThinkingHandler()
        handler.add_thinking("First thought")
        handler.add_thinking("Second thought")

        blocks = handler.get_all_thinking()

        assert len(blocks) == 2
        assert isinstance(blocks[0], ThinkingBlock)
        assert blocks[0].content == "First thought"
        assert blocks[1].content == "Second thought"

    def test_get_latest_thinking(self):
        """Test getting the most recent thinking block."""
        handler = ThinkingHandler()

        assert handler.get_latest_thinking() is None

        handler.add_thinking("First")
        handler.add_thinking("Second")

        latest = handler.get_latest_thinking()
        assert latest is not None
        assert latest.content == "Second"

    def test_clear_resets_blocks(self):
        """Test clearing thinking blocks."""
        handler = ThinkingHandler()
        handler.add_thinking("content")
        handler.add_thinking("more content")

        handler.clear()

        assert handler.block_count == 0
        assert handler.get_all_thinking() == []

    def test_clear_resets_id_counter(self):
        """Test that clear resets ID counter."""
        handler = ThinkingHandler()
        handler.add_thinking("First")
        handler.add_thinking("Second")
        handler.clear()

        result = handler.add_thinking("After clear")
        assert result["thinking_id"] == "think-1"  # ID reset to 1

    def test_toggle_visibility(self):
        """Test visibility toggle cycles through levels."""
        handler = ThinkingHandler()
        assert handler.is_visible is True  # Default MEDIUM

        # toggle_visibility now cycles levels: MEDIUM -> HIGH -> OFF -> LOW -> MEDIUM
        new_state = handler.toggle_visibility()
        assert new_state is True  # HIGH is still enabled

        new_state = handler.toggle_visibility()
        assert new_state is False  # OFF
        assert handler.is_visible is False

    def test_block_count_property(self):
        """Test block_count property."""
        handler = ThinkingHandler()
        assert handler.block_count == 0

        handler.add_thinking("One")
        assert handler.block_count == 1

        handler.add_thinking("Two")
        assert handler.block_count == 2

        handler.clear()
        assert handler.block_count == 0


class TestThinkToolRemoved:
    """Tests verifying think tool has been removed from schemas.

    Thinking is now a separate pre-processing phase, not a tool.
    """

    def test_think_schema_not_in_builtin(self):
        """Test that think tool schema is NOT defined (removed)."""
        from swecli.core.agents.components.schemas import _BUILTIN_TOOL_SCHEMAS

        names = [s["function"]["name"] for s in _BUILTIN_TOOL_SCHEMAS]
        assert "think" not in names, "Think tool should be removed from schemas"

    def test_think_not_in_planning_tools(self):
        """Test that think is NOT in plan mode tools (removed)."""
        from swecli.core.agents.components import PLANNING_TOOLS

        assert "think" not in PLANNING_TOOLS, "Think should be removed from PLANNING_TOOLS"


class TestThinkingHandlerStillExists:
    """Tests verifying ThinkingHandler still exists for visibility tracking.

    Even though think is no longer a tool, we still need the handler
    for tracking thinking visibility state.
    """

    def test_registry_has_thinking_handler(self):
        """Test that ToolRegistry still has thinking_handler for visibility."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        assert hasattr(registry, "thinking_handler")
        assert isinstance(registry.thinking_handler, ThinkingHandler)


class TestThinkingUICallback:
    """Tests for thinking UI callback integration."""

    def test_callback_has_thinking_visible_attribute(self):
        """Test that TextualUICallback initializes thinking visibility."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        callback = TextualUICallback(mock_conversation)

        assert hasattr(callback, "_thinking_visible")
        assert callback._thinking_visible is False  # Default OFF

    def test_on_thinking_calls_add_thinking_block(self):
        """Test that on_thinking calls conversation.add_thinking_block."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        mock_conversation.add_thinking_block = MagicMock()
        callback = TextualUICallback(mock_conversation)
        callback._thinking_visible = True  # Enable visibility

        # Mock _run_on_ui to call function directly
        callback._run_on_ui = lambda f, *args: f(*args)

        callback.on_thinking("Test thinking content")

        mock_conversation.add_thinking_block.assert_called_once_with("Test thinking content")

    def test_on_thinking_skipped_when_not_visible(self):
        """Test that on_thinking is skipped when visibility is off."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        mock_conversation.add_thinking_block = MagicMock()
        callback = TextualUICallback(mock_conversation)
        callback._thinking_visible = False

        callback.on_thinking("Test content")

        mock_conversation.add_thinking_block.assert_not_called()

    def test_on_thinking_reads_from_chat_app_state(self):
        """Test that on_thinking reads visibility from chat_app._thinking_visible."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        mock_conversation.add_thinking_block = MagicMock()
        mock_app = MagicMock()
        mock_app._thinking_visible = False  # App says hidden

        callback = TextualUICallback(mock_conversation, chat_app=mock_app)
        callback._thinking_visible = True  # Local state says visible
        callback._run_on_ui = lambda f, *args: f(*args)

        callback.on_thinking("Test content")

        # Should NOT display because app state says hidden
        mock_conversation.add_thinking_block.assert_not_called()

    def test_on_thinking_uses_app_state_when_visible(self):
        """Test that on_thinking displays when chat_app._thinking_visible is True."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        mock_conversation.add_thinking_block = MagicMock()
        mock_app = MagicMock()
        mock_app._thinking_visible = True  # App says visible

        callback = TextualUICallback(mock_conversation, chat_app=mock_app)
        callback._run_on_ui = lambda f, *args: f(*args)

        callback.on_thinking("Test content")

        # Should display because app state says visible
        mock_conversation.add_thinking_block.assert_called_once_with("Test content")

    def test_on_thinking_skipped_for_empty_content(self):
        """Test that on_thinking is skipped for empty content."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        mock_conversation.add_thinking_block = MagicMock()
        callback = TextualUICallback(mock_conversation)

        callback.on_thinking("")
        callback.on_thinking("   ")

        mock_conversation.add_thinking_block.assert_not_called()

    def test_toggle_thinking_visibility(self):
        """Test toggle_thinking_visibility method (fallback when no app)."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        callback = TextualUICallback(mock_conversation)

        assert callback._thinking_visible is False  # Default OFF

        new_state = callback.toggle_thinking_visibility()
        assert new_state is True
        assert callback._thinking_visible is True

        new_state = callback.toggle_thinking_visibility()
        assert new_state is False
        assert callback._thinking_visible is False

    def test_toggle_thinking_visibility_syncs_with_app(self):
        """Test toggle_thinking_visibility syncs with chat_app._thinking_visible."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        mock_app = MagicMock()
        mock_app._thinking_visible = True

        callback = TextualUICallback(mock_conversation, chat_app=mock_app)

        # Toggle should change both app and local state
        new_state = callback.toggle_thinking_visibility()
        assert new_state is False
        assert mock_app._thinking_visible is False
        assert callback._thinking_visible is False

        # Toggle again
        new_state = callback.toggle_thinking_visibility()
        assert new_state is True
        assert mock_app._thinking_visible is True
        assert callback._thinking_visible is True

    def test_on_thinking_called_directly(self):
        """Test that on_thinking works when called directly (from thinking phase)."""
        from swecli.ui_textual.ui_callback import TextualUICallback

        mock_conversation = MagicMock()
        mock_conversation.add_thinking_block = MagicMock()
        mock_app = MagicMock()
        mock_app._thinking_visible = True
        callback = TextualUICallback(mock_conversation, chat_app=mock_app)

        # Mock _run_on_ui to call function directly
        callback._run_on_ui = lambda f, *args: f(*args)

        # Called directly from the thinking phase (not via tool result)
        callback.on_thinking("My reasoning from thinking phase")

        mock_conversation.add_thinking_block.assert_called_once_with("My reasoning from thinking phase")


class TestCallbackProtocol:
    """Tests for callback protocol compliance."""

    def test_base_ui_callback_has_on_thinking(self):
        """Test BaseUICallback has on_thinking method."""
        from swecli.ui_textual.callback_interface import BaseUICallback

        callback = BaseUICallback()
        assert hasattr(callback, "on_thinking")
        # Should not raise
        callback.on_thinking("test content")

    def test_forwarding_callback_forwards_on_thinking(self):
        """Test ForwardingUICallback forwards on_thinking."""
        from swecli.ui_textual.callback_interface import ForwardingUICallback

        mock_parent = MagicMock()
        mock_parent.on_thinking = MagicMock()

        callback = ForwardingUICallback(mock_parent)
        callback.on_thinking("test content")

        mock_parent.on_thinking.assert_called_once_with("test content")


class TestStyleTokens:
    """Tests for thinking-related style tokens."""

    def test_thinking_tokens_exist(self):
        """Test that THINKING and THINKING_ICON tokens are defined."""
        from swecli.ui_textual.style_tokens import THINKING, THINKING_ICON

        assert THINKING is not None
        assert isinstance(THINKING, str)
        assert THINKING.startswith("#")  # Should be a hex color

        assert THINKING_ICON is not None
        assert isinstance(THINKING_ICON, str)


class TestMessageRendererDedup:
    """Tests for assistant message dedup — now handled by DisplayLedger.

    The renderer no longer does its own dedup; DisplayLedger coordinates
    cross-path dedup at a higher level. These tests verify the renderer
    always renders (dedup responsibility moved to DisplayLedger).
    """

    def test_identical_response_always_renders_in_renderer(self):
        """Renderer always renders — dedup is DisplayLedger's job now."""
        from swecli.ui_textual.widgets.conversation.message_renderer import DefaultMessageRenderer

        mock_log = MagicMock()
        mock_log.lines = []
        renderer = DefaultMessageRenderer(mock_log)

        renderer.add_assistant_message("Hello! How can I help?")
        first_call_count = mock_log.write.call_count
        assert first_call_count > 0, "First message should render"

        # Same message renders again (renderer no longer deduplicates)
        renderer.add_assistant_message("Hello! How can I help?")
        assert mock_log.write.call_count > first_call_count, (
            "Renderer should always render — dedup moved to DisplayLedger"
        )

    def test_display_ledger_handles_cross_path_dedup(self):
        """DisplayLedger deduplicates identical assistant messages from different paths."""
        from swecli.ui_textual.managers.display_ledger import DisplayLedger

        mock_conversation = MagicMock()
        mock_conversation.add_assistant_message = MagicMock()
        ledger = DisplayLedger(mock_conversation)

        ledger.display_user_message("hello", "test")
        ledger.display_assistant_message("Hello! How can I help?", "ui_callback")
        ledger.display_assistant_message("Hello! How can I help?", "render_responses")

        # Second call deduped by ledger
        mock_conversation.add_assistant_message.assert_called_once_with("Hello! How can I help?")


class TestMessageRenderer:
    """Tests for thinking block rendering."""

    def test_add_thinking_block_method_exists(self):
        """Test DefaultMessageRenderer has add_thinking_block."""
        from swecli.ui_textual.widgets.conversation.message_renderer import DefaultMessageRenderer

        mock_log = MagicMock()
        renderer = DefaultMessageRenderer(mock_log)

        assert hasattr(renderer, "add_thinking_block")

    def test_add_thinking_block_writes_to_log(self):
        """Test add_thinking_block writes to log."""
        from swecli.ui_textual.widgets.conversation.message_renderer import DefaultMessageRenderer
        from rich.text import Text

        mock_log = MagicMock()
        mock_log.lines = []  # Empty log so SpacingManager sees no prior content
        renderer = DefaultMessageRenderer(mock_log)

        renderer.add_thinking_block("My thinking content")

        # Should write the thinking text and a structural blank line
        assert mock_log.write.call_count == 2

    def test_add_thinking_block_skips_empty(self):
        """Test add_thinking_block skips empty content."""
        from swecli.ui_textual.widgets.conversation.message_renderer import DefaultMessageRenderer

        mock_log = MagicMock()
        renderer = DefaultMessageRenderer(mock_log)

        renderer.add_thinking_block("")
        renderer.add_thinking_block("   ")

        mock_log.write.assert_not_called()


class TestConversationLog:
    """Tests for ConversationLog delegation."""

    def test_add_thinking_block_delegates(self):
        """Test ConversationLog.add_thinking_block delegates to renderer."""
        from swecli.ui_textual.widgets.conversation_log import ConversationLog

        # ConversationLog inherits from RichLog which requires more setup
        # Use a simpler approach - just verify the method exists
        assert hasattr(ConversationLog, "add_thinking_block")


class TestStatusBar:
    """Tests for StatusBar thinking mode display."""

    def test_status_bar_has_thinking_enabled_attribute(self):
        """Test StatusBar initializes with thinking_enabled based on default thinking_level."""
        from swecli.ui_textual.widgets.status_bar import StatusBar

        status_bar = StatusBar()
        assert hasattr(status_bar, "thinking_enabled")
        # Default thinking_level is "Medium", so thinking_enabled is True
        assert status_bar.thinking_enabled is True

    def test_set_thinking_enabled(self):
        """Test set_thinking_enabled method."""
        from swecli.ui_textual.widgets.status_bar import StatusBar

        status_bar = StatusBar()
        # Mock update_status to avoid Textual context requirement
        status_bar.update_status = MagicMock()

        status_bar.set_thinking_enabled(False)
        assert status_bar.thinking_enabled is False
        status_bar.update_status.assert_called()

        status_bar.set_thinking_enabled(True)
        assert status_bar.thinking_enabled is True


class TestThinkingModeReminder:
    """Tests for thinking mode placeholder replacement.

    Note: With the new architecture, thinking is a separate phase,
    so these placeholders may need to be updated or removed.
    """

    def test_no_placeholder_leaves_content_unchanged(self):
        """Test that prompts without placeholder are left unchanged."""
        from swecli.repl.query_enhancer import QueryEnhancer

        file_ops = MagicMock()
        session_manager = MagicMock()
        session_manager.current_session = None
        config = MagicMock()
        config.playbook = None
        console = MagicMock()

        enhancer = QueryEnhancer(file_ops, session_manager, config, console)
        mock_agent = MagicMock()
        mock_agent.system_prompt = "No placeholder here"

        messages = enhancer.prepare_messages(
            query="Help me",
            enhanced_query="Help me",
            agent=mock_agent,
            thinking_visible=True
        )

        system_content = messages[0]["content"]
        assert system_content == "No placeholder here"


class TestThinkingModeSchemaFiltering:
    """Tests for tool schema building - think tool is now removed.

    With the new architecture, thinking is a separate phase,
    so the think tool is no longer in schemas at all.
    """

    def test_think_tool_never_in_schemas(self):
        """Test that think tool is NEVER in schemas (removed from architecture)."""
        from swecli.core.agents.components import ToolSchemaBuilder

        mock_registry = MagicMock()
        mock_registry.subagent_manager = None
        mock_registry.get_all_mcp_tools.return_value = []

        builder = ToolSchemaBuilder(mock_registry)

        # Should not have think tool regardless of thinking_visible parameter
        schemas_visible = builder.build(thinking_visible=True)
        schemas_not_visible = builder.build(thinking_visible=False)

        names_visible = [s.get("function", {}).get("name") for s in schemas_visible]
        names_not_visible = [s.get("function", {}).get("name") for s in schemas_not_visible]

        assert "think" not in names_visible, "Think tool should be removed from schemas"
        assert "think" not in names_not_visible, "Think tool should be removed from schemas"

        # Both should have the same tools (thinking_visible is deprecated)
        assert len(schemas_visible) == len(schemas_not_visible)


class TestThinkingModelSelection:
    """Tests for using Thinking model when thinking mode is ON."""

    def test_thinking_model_used_when_configured_and_visible(self):
        """Test agent always uses Normal model in call_llm (thinking happens in separate call)."""
        from swecli.core.agents.main_agent import MainAgent

        config = MagicMock()
        config.model = "gpt-4"
        config.model_thinking = "o1-preview"
        config.model_thinking_provider = "openai"
        config.model_provider = "openai"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.get_api_key.return_value = "test-key"

        tool_registry = MagicMock()
        tool_registry.subagent_manager = None
        tool_registry.get_all_mcp_tools.return_value = []

        mode_manager = MagicMock()

        with patch.object(MainAgent, 'build_system_prompt', return_value="Test prompt"), \
             patch.object(MainAgent, 'build_tool_schemas', return_value=[]):
            agent = MainAgent(config, tool_registry, mode_manager)

            # Mock the HTTP client's post_json to return a successful response
            mock_result = MagicMock()
            mock_result.success = True
            mock_result.response = MagicMock()
            mock_result.response.status_code = 200
            mock_result.response.json.return_value = {
                "choices": [{"message": {"content": "test response", "reasoning_content": "my reasoning"}}]
            }

            with patch.object(agent, "_MainAgent__http_client") as mock_client:
                mock_client.post_json.return_value = mock_result
                # Use the normal http client by patching _thinking_http_client to None
                agent._MainAgent__thinking_http_client = None

                response = agent.call_llm([{"role": "user", "content": "test"}], thinking_visible=True)

                # Check payload used the normal model (thinking is a separate phase)
                call_args = mock_client.post_json.call_args
                payload = call_args[0][0]
                assert payload["model"] == "gpt-4"

    def test_normal_model_used_when_not_visible(self):
        """Test agent uses Normal model when thinking_visible=False."""
        from swecli.core.agents.main_agent import MainAgent

        config = MagicMock()
        config.model = "gpt-4"
        config.model_thinking = "o1-preview"
        config.model_thinking_provider = "openai"
        config.model_provider = "openai"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.get_api_key.return_value = "test-key"

        tool_registry = MagicMock()
        tool_registry.subagent_manager = None
        tool_registry.get_all_mcp_tools.return_value = []

        mode_manager = MagicMock()

        with patch.object(MainAgent, 'build_system_prompt', return_value="Test prompt"), \
             patch.object(MainAgent, 'build_tool_schemas', return_value=[]):
            agent = MainAgent(config, tool_registry, mode_manager)

            mock_result = MagicMock()
            mock_result.success = True
            mock_result.response = MagicMock()
            mock_result.response.status_code = 200
            mock_result.response.json.return_value = {
                "choices": [{"message": {"content": "test response"}}]
            }

            with patch.object(agent, "_MainAgent__http_client") as mock_client:
                mock_client.post_json.return_value = mock_result

                response = agent.call_llm([{"role": "user", "content": "test"}], thinking_visible=False)

                call_args = mock_client.post_json.call_args
                payload = call_args[0][0]
                assert payload["model"] == "gpt-4"

    def test_fallback_to_normal_when_no_thinking_model(self):
        """Test agent falls back to Normal model when no Thinking model configured."""
        from swecli.core.agents.main_agent import MainAgent

        config = MagicMock()
        config.model = "gpt-4"
        config.model_thinking = None  # No thinking model configured
        config.model_provider = "openai"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.get_api_key.return_value = "test-key"

        tool_registry = MagicMock()
        tool_registry.subagent_manager = None
        tool_registry.get_all_mcp_tools.return_value = []

        mode_manager = MagicMock()

        with patch.object(MainAgent, 'build_system_prompt', return_value="Test prompt"), \
             patch.object(MainAgent, 'build_tool_schemas', return_value=[]):
            agent = MainAgent(config, tool_registry, mode_manager)

            mock_result = MagicMock()
            mock_result.success = True
            mock_result.response = MagicMock()
            mock_result.response.status_code = 200
            mock_result.response.json.return_value = {
                "choices": [{"message": {"content": "test response"}}]
            }

            with patch.object(agent, "_MainAgent__http_client") as mock_client:
                mock_client.post_json.return_value = mock_result

                response = agent.call_llm([{"role": "user", "content": "test"}], thinking_visible=True)

                call_args = mock_client.post_json.call_args
                payload = call_args[0][0]
                assert payload["model"] == "gpt-4"  # Falls back to normal model


class TestReasoningContentExtraction:
    """Tests for extracting reasoning_content from model responses."""

    def test_reasoning_content_extracted(self):
        """Test that reasoning_content is extracted from response."""
        from swecli.core.agents.main_agent import MainAgent

        config = MagicMock()
        config.model = "o1-preview"
        config.model_thinking = None
        config.model_provider = "openai"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.get_api_key.return_value = "test-key"

        tool_registry = MagicMock()
        tool_registry.subagent_manager = None
        tool_registry.get_all_mcp_tools.return_value = []

        mode_manager = MagicMock()

        with patch.object(MainAgent, 'build_system_prompt', return_value="Test prompt"), \
             patch.object(MainAgent, 'build_tool_schemas', return_value=[]):
            agent = MainAgent(config, tool_registry, mode_manager)

            mock_result = MagicMock()
            mock_result.success = True
            mock_result.response = MagicMock()
            mock_result.response.status_code = 200
            mock_result.response.json.return_value = {
                "choices": [{
                    "message": {
                        "content": "Final answer",
                        "reasoning_content": "Step 1: analyze...\nStep 2: evaluate..."
                    }
                }]
            }

            with patch.object(agent, "_MainAgent__http_client") as mock_client:
                mock_client.post_json.return_value = mock_result

                response = agent.call_llm([{"role": "user", "content": "test"}])

                assert response["success"] is True
                assert response["reasoning_content"] == "Step 1: analyze...\nStep 2: evaluate..."

    def test_reasoning_content_none_when_not_present(self):
        """Test reasoning_content is None when model doesn't provide it."""
        from swecli.core.agents.main_agent import MainAgent

        config = MagicMock()
        config.model = "gpt-4"
        config.model_thinking = None
        config.model_provider = "openai"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.get_api_key.return_value = "test-key"

        tool_registry = MagicMock()
        tool_registry.subagent_manager = None
        tool_registry.get_all_mcp_tools.return_value = []

        mode_manager = MagicMock()

        with patch.object(MainAgent, 'build_system_prompt', return_value="Test prompt"), \
             patch.object(MainAgent, 'build_tool_schemas', return_value=[]):
            agent = MainAgent(config, tool_registry, mode_manager)

            mock_result = MagicMock()
            mock_result.success = True
            mock_result.response = MagicMock()
            mock_result.response.status_code = 200
            mock_result.response.json.return_value = {
                "choices": [{"message": {"content": "Just content, no reasoning"}}]
            }

            with patch.object(agent, "_MainAgent__http_client") as mock_client:
                mock_client.post_json.return_value = mock_result

                response = agent.call_llm([{"role": "user", "content": "test"}])

                assert response["success"] is True
                assert response["reasoning_content"] is None


class TestReactExecutorReasoningDisplay:
    """Tests for react_executor displaying reasoning content."""

    def test_parse_llm_response_extracts_reasoning(self):
        """Test _parse_llm_response extracts reasoning_content."""
        from swecli.repl.react_executor import ReactExecutor

        mock_console = MagicMock()
        mock_session_manager = MagicMock()
        mock_config = MagicMock()
        mock_llm_caller = MagicMock()
        mock_tool_executor = MagicMock()

        executor = ReactExecutor(
            mock_console,
            mock_session_manager,
            mock_config,
            mock_llm_caller,
            mock_tool_executor
        )

        response = {
            "content": "My response",
            "tool_calls": [{"function": {"name": "test"}}],
            "reasoning_content": "My reasoning"
        }

        content, tool_calls, reasoning_content = executor._parse_llm_response(response)

        assert content == "My response"
        assert tool_calls is not None
        assert reasoning_content == "My reasoning"

    def test_parse_llm_response_handles_no_reasoning(self):
        """Test _parse_llm_response handles missing reasoning_content."""
        from swecli.repl.react_executor import ReactExecutor

        mock_console = MagicMock()
        mock_session_manager = MagicMock()
        mock_config = MagicMock()
        mock_llm_caller = MagicMock()
        mock_tool_executor = MagicMock()

        executor = ReactExecutor(
            mock_console,
            mock_session_manager,
            mock_config,
            mock_llm_caller,
            mock_tool_executor
        )

        response = {
            "content": "My response",
            "tool_calls": None
        }

        content, tool_calls, reasoning_content = executor._parse_llm_response(response)

        assert content == "My response"
        assert tool_calls is None
        assert reasoning_content is None


class TestThinkingPromptBuilder:
    """Tests for ThinkingPromptBuilder."""

    def test_thinking_prompt_builder_loads_prompt(self):
        """Test ThinkingPromptBuilder loads thinking prompt template."""
        from swecli.core.agents.components import ThinkingPromptBuilder

        mock_return = "Thinking Mode: You are in thinking mode.\nWorking directory context will be added."
        with (
            patch("swecli.core.agents.components.prompts.builders.load_prompt") as mock_load,
            patch("swecli.core.agents.prompts.loader.load_prompt") as mock_loader_load,
        ):
            mock_load.return_value = mock_return
            mock_loader_load.return_value = mock_return

            builder = ThinkingPromptBuilder(tool_registry=None, working_dir="/test/dir")
            prompt = builder.build()

            assert "Thinking Mode" in prompt
            assert "/test/dir" in prompt

    def test_thinking_prompt_builder_includes_mcp_tools(self):
        """Test ThinkingPromptBuilder includes MCP tools if available."""
        from swecli.core.agents.components import ThinkingPromptBuilder

        mock_registry = MagicMock()
        mock_registry.mcp_manager = MagicMock()
        mock_registry.mcp_manager.list_servers.return_value = ["test_server"]
        mock_registry.mcp_manager.is_connected.return_value = True
        mock_registry.mcp_manager.get_server_tools.return_value = [
            {"name": "test_tool", "description": "A test tool"}
        ]
        # Prevent MagicMock from _skill_loader attribute access
        mock_registry._skill_loader = None

        with (
            patch("swecli.core.agents.components.prompts.builders.load_prompt") as mock_load,
            patch("swecli.core.agents.prompts.loader.load_prompt") as mock_loader_load,
        ):
            mock_load.return_value = "Thinking Mode prompt"
            mock_loader_load.return_value = "Thinking Mode prompt"

            builder = ThinkingPromptBuilder(tool_registry=mock_registry, working_dir="/test")
            prompt = builder.build()

            assert "test_server" in prompt


class TestHTTPClientFactory:
    """Tests for HTTP client factory for different providers."""

    def test_openai_client_creation(self):
        """Test creating HTTP client for OpenAI provider."""
        from swecli.core.agents.components import create_http_client_for_provider

        config = MagicMock()

        with patch.dict("os.environ", {"OPENAI_API_KEY": "test-key"}):
            client = create_http_client_for_provider("openai", config)
            assert client is not None

    def test_fireworks_client_creation(self):
        """Test creating HTTP client for Fireworks provider."""
        from swecli.core.agents.components import create_http_client_for_provider

        config = MagicMock()

        with patch.dict("os.environ", {"FIREWORKS_API_KEY": "test-key"}):
            client = create_http_client_for_provider("fireworks", config)
            assert client is not None

    def test_missing_api_key_raises(self):
        """Test that missing API key raises ValueError."""
        from swecli.core.agents.components import create_http_client_for_provider

        config = MagicMock()

        with patch.dict("os.environ", {}, clear=True):
            with pytest.raises(ValueError, match="OPENAI_API_KEY"):
                create_http_client_for_provider("openai", config)

    def test_unknown_provider_raises(self):
        """Test that unknown provider raises ValueError."""
        from swecli.core.agents.components import create_http_client_for_provider

        config = MagicMock()

        with pytest.raises(ValueError, match="Unknown provider"):
            create_http_client_for_provider("unknown_provider", config)


class TestToolChoiceBehavior:
    """Tests for tool_choice behavior.

    With the new architecture:
    - force_think parameter is removed
    - tool_choice is always 'auto' for action phase
    - Thinking happens in a separate LLM call (no tools)
    """

    def test_tool_choice_always_auto(self):
        """tool_choice should always be 'auto' (no more force_think)."""
        from swecli.core.agents.main_agent import MainAgent

        config = MagicMock()
        config.model = "gpt-4"
        config.model_thinking = "o1-preview"
        config.model_thinking_provider = "openai"
        config.model_provider = "openai"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.get_api_key.return_value = "test-key"

        tool_registry = MagicMock()
        tool_registry.subagent_manager = None
        tool_registry.get_all_mcp_tools.return_value = []

        mode_manager = MagicMock()

        with patch.object(MainAgent, 'build_system_prompt', return_value="Test prompt"), \
             patch.object(MainAgent, 'build_tool_schemas', return_value=[]):
            agent = MainAgent(config, tool_registry, mode_manager)

            mock_result = MagicMock()
            mock_result.success = True
            mock_result.response = MagicMock()
            mock_result.response.status_code = 200
            mock_result.response.json.return_value = {
                "choices": [{"message": {"content": "Hello!"}}]
            }

            with patch.object(agent, "_MainAgent__http_client") as mock_client:
                mock_client.post_json.return_value = mock_result
                agent._MainAgent__thinking_http_client = None

                # Call without force_think (parameter removed)
                agent.call_llm(
                    [{"role": "user", "content": "hello"}],
                    thinking_visible=True
                )

                call_args = mock_client.post_json.call_args
                payload = call_args[0][0]
                assert payload["tool_choice"] == "auto", "tool_choice should always be auto"

    def test_call_llm_no_force_think_parameter(self):
        """Verify call_llm no longer accepts force_think parameter."""
        from swecli.core.agents.main_agent import MainAgent
        import inspect

        # Get the signature of call_llm
        sig = inspect.signature(MainAgent.call_llm)
        param_names = list(sig.parameters.keys())

        assert "force_think" not in param_names, "force_think parameter should be removed"
