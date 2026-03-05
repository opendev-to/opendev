"""Unit tests for subagent infrastructure."""

import pytest
from unittest.mock import MagicMock, patch, AsyncMock
import asyncio

from swecli.core.agents.subagents import (
    SubAgentSpec,
    CompiledSubAgent,
    SubAgentManager,
    create_task_tool_schema,
    TASK_TOOL_NAME,
    ALL_SUBAGENTS,
)
from swecli.core.agents.subagents.manager import SubAgentDeps
from swecli.models.config import AppConfig


class TestSubAgentSpec:
    """Tests for SubAgentSpec TypedDict structure."""

    def test_subagent_spec_required_fields(self):
        """Test that SubAgentSpec requires name, description, and system_prompt."""
        spec = SubAgentSpec(
            name="test-agent",
            description="A test agent",
            system_prompt="You are a test agent.",
        )
        assert spec["name"] == "test-agent"
        assert spec["description"] == "A test agent"
        assert spec["system_prompt"] == "You are a test agent."

    def test_subagent_spec_optional_tools(self):
        """Test that tools field is optional."""
        spec = SubAgentSpec(
            name="test-agent",
            description="A test agent",
            system_prompt="You are a test agent.",
            tools=["read_file", "search"],
        )
        assert spec["tools"] == ["read_file", "search"]

    def test_subagent_spec_optional_model(self):
        """Test that model field is optional."""
        spec = SubAgentSpec(
            name="test-agent",
            description="A test agent",
            system_prompt="You are a test agent.",
            model="gpt-4o",
        )
        assert spec["model"] == "gpt-4o"


class TestDefaultSubAgents:
    """Tests for default subagent specifications."""

    def test_default_subagents_exist(self):
        """Test that default subagents are defined."""
        assert len(ALL_SUBAGENTS) > 0

    def test_required_subagent_types(self):
        """Test that required subagent types are present."""
        names = [spec["name"] for spec in ALL_SUBAGENTS]
        assert "ask-user" in names
        assert "Code-Explorer" in names
        assert "Web-clone" in names
        assert "Web-Generator" in names
        assert "Planner" in names

    def test_explorer_has_readonly_tools(self):
        """Test that explorer agent only has read-only tools."""
        explorer = next(s for s in ALL_SUBAGENTS if s["name"] == "Code-Explorer")
        tools = explorer.get("tools", [])
        # Should not have write/edit/run tools
        assert "write_file" not in tools
        assert "edit_file" not in tools
        assert "run_command" not in tools
        # Should have read tools
        assert "read_file" in tools
        assert "search" in tools

    def test_all_subagents_have_system_prompt(self):
        """Test that all subagents have system prompts defined."""
        for spec in ALL_SUBAGENTS:
            assert "system_prompt" in spec
            assert len(spec["system_prompt"]) > 0


class TestSubAgentManager:
    """Tests for SubAgentManager."""

    @pytest.fixture
    def mock_config(self):
        """Create a mock AppConfig."""
        config = MagicMock(spec=AppConfig)
        config.model = "gpt-4o"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.api_key = "test-key"
        config.api_base_url = None
        return config

    @pytest.fixture
    def mock_tool_registry(self):
        """Create a mock tool registry."""
        return MagicMock()

    @pytest.fixture
    def mock_mode_manager(self):
        """Create a mock mode manager."""
        return MagicMock()

    @pytest.fixture
    def manager(self, mock_config, mock_tool_registry, mock_mode_manager):
        """Create a SubAgentManager with mocked dependencies."""
        return SubAgentManager(
            config=mock_config,
            tool_registry=mock_tool_registry,
            mode_manager=mock_mode_manager,
            working_dir="/tmp/test",
        )

    def test_manager_initialization(self, manager):
        """Test that manager initializes with empty agents dict."""
        assert manager._agents == {}

    def test_get_all_tool_names(self, manager):
        """Test that _get_all_tool_names returns core tools."""
        tools = manager._get_all_tool_names()
        assert "read_file" in tools
        assert "write_file" in tools
        assert "search" in tools
        assert "run_command" in tools

    def test_get_available_types_empty_initially(self, manager):
        """Test that no agents are available before registration."""
        assert manager.get_available_types() == []

    def test_get_descriptions_empty_initially(self, manager):
        """Test that no descriptions before registration."""
        assert manager.get_descriptions() == {}

    @patch("swecli.core.agents.MainAgent")
    def test_register_subagent(self, mock_agent_class, manager):
        """Test registering a custom subagent."""
        mock_agent = MagicMock()
        mock_agent_class.return_value = mock_agent

        spec = SubAgentSpec(
            name="custom-agent",
            description="A custom test agent",
            system_prompt="You are a custom agent.",
            tools=["read_file"],
        )
        manager.register_subagent(spec)

        assert "custom-agent" in manager.get_available_types()
        assert manager.get_descriptions()["custom-agent"] == "A custom test agent"

    @patch("swecli.core.agents.MainAgent")
    def test_register_defaults(self, mock_agent_class, manager):
        """Test registering default subagents."""
        mock_agent = MagicMock()
        mock_agent_class.return_value = mock_agent

        manager.register_defaults()

        available = manager.get_available_types()
        assert "ask-user" in available
        assert "Code-Explorer" in available
        assert "Web-clone" in available
        assert "Web-Generator" in available
        assert "Planner" in available

    @patch("swecli.core.agents.MainAgent")
    def test_get_subagent(self, mock_agent_class, manager):
        """Test getting a registered subagent."""
        mock_agent = MagicMock()
        mock_agent_class.return_value = mock_agent

        spec = SubAgentSpec(
            name="test-agent",
            description="Test",
            system_prompt="Test prompt",
        )
        manager.register_subagent(spec)

        subagent = manager.get_subagent("test-agent")
        assert subagent is not None
        assert subagent["name"] == "test-agent"

    def test_get_subagent_not_found(self, manager):
        """Test getting a non-existent subagent returns None."""
        assert manager.get_subagent("nonexistent") is None

    def test_execute_subagent_unknown_type(self, manager):
        """Test executing with unknown subagent type returns error."""
        deps = SubAgentDeps(
            mode_manager=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
        )

        result = manager.execute_subagent(
            name="unknown-agent",
            task="Do something",
            deps=deps,
        )

        assert result["success"] is False
        assert "Unknown subagent type" in result["error"]

    @patch("swecli.core.agents.MainAgent")
    def test_execute_subagent_success(self, mock_agent_class, manager):
        """Test successful subagent execution."""
        mock_agent = MagicMock()
        mock_agent.run_sync.return_value = {
            "success": True,
            "content": "Task completed successfully",
        }
        mock_agent_class.return_value = mock_agent

        spec = SubAgentSpec(
            name="test-agent",
            description="Test",
            system_prompt="Test prompt",
        )
        manager.register_subagent(spec)

        deps = SubAgentDeps(
            mode_manager=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
        )

        result = manager.execute_subagent(
            name="test-agent",
            task="Do something",
            deps=deps,
        )

        assert result["success"] is True
        mock_agent.run_sync.assert_called_once()

    @patch("swecli.core.agents.MainAgent")
    def test_subagent_config_with_model_override(self, mock_agent_class, mock_config, mock_tool_registry, mock_mode_manager):
        """Test that model override creates new config."""
        mock_agent = MagicMock()
        mock_agent_class.return_value = mock_agent

        manager = SubAgentManager(
            config=mock_config,
            tool_registry=mock_tool_registry,
            mode_manager=mock_mode_manager,
            working_dir="/tmp/test",
        )

        spec = SubAgentSpec(
            name="override-agent",
            description="Agent with model override",
            system_prompt="Test prompt",
            model="gpt-3.5-turbo",
        )
        manager.register_subagent(spec)

        # Verify MainAgent was called with a config
        mock_agent_class.assert_called()
        call_args = mock_agent_class.call_args
        passed_config = call_args[1]["config"] if call_args[1] else call_args[0][0]
        assert passed_config.model == "gpt-3.5-turbo"


class TestSubAgentManagerAsync:
    """Tests for async subagent execution."""

    @pytest.fixture
    def mock_config(self):
        """Create a mock AppConfig."""
        config = MagicMock(spec=AppConfig)
        config.model = "gpt-4o"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.api_key = "test-key"
        config.api_base_url = None
        return config

    @pytest.fixture
    def manager(self, mock_config):
        """Create a SubAgentManager with mocked dependencies."""
        return SubAgentManager(
            config=mock_config,
            tool_registry=MagicMock(),
            mode_manager=MagicMock(),
            working_dir="/tmp/test",
        )

    @patch("swecli.core.agents.MainAgent")
    @pytest.mark.asyncio
    async def test_execute_subagent_async(self, mock_agent_class, manager):
        """Test async subagent execution."""
        mock_agent = MagicMock()
        mock_agent.run_sync.return_value = {
            "success": True,
            "content": "Async task completed",
        }
        mock_agent_class.return_value = mock_agent

        spec = SubAgentSpec(
            name="async-agent",
            description="Test",
            system_prompt="Test prompt",
        )
        manager.register_subagent(spec)

        deps = SubAgentDeps(
            mode_manager=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
        )

        result = await manager.execute_subagent_async(
            name="async-agent",
            task="Do async task",
            deps=deps,
        )

        assert result["success"] is True

    @patch("swecli.core.agents.MainAgent")
    @pytest.mark.asyncio
    async def test_execute_parallel(self, mock_agent_class, manager):
        """Test parallel subagent execution."""
        call_count = 0

        def mock_run_sync(*args, **kwargs):
            nonlocal call_count
            call_count += 1
            return {"success": True, "content": f"Task {call_count} done"}

        mock_agent = MagicMock()
        mock_agent.run_sync.side_effect = mock_run_sync
        mock_agent_class.return_value = mock_agent

        # Register multiple agents
        for name in ["agent-1", "agent-2", "agent-3"]:
            spec = SubAgentSpec(
                name=name,
                description=f"Agent {name}",
                system_prompt="Test prompt",
            )
            manager.register_subagent(spec)

        deps = SubAgentDeps(
            mode_manager=MagicMock(),
            approval_manager=MagicMock(),
            undo_manager=MagicMock(),
        )

        tasks = [
            ("agent-1", "Task 1"),
            ("agent-2", "Task 2"),
            ("agent-3", "Task 3"),
        ]

        results = await manager.execute_parallel(tasks, deps)

        assert len(results) == 3
        assert all(r["success"] for r in results)


class TestSpawnSubagentToolSchema:
    """Tests for spawn_subagent tool schema generation."""

    @pytest.fixture
    def mock_manager(self):
        """Create a mock SubAgentManager."""
        manager = MagicMock()
        config1 = MagicMock()
        config1.name = "Code-Explorer"
        config1.description = "Codebase exploration agent"
        config2 = MagicMock()
        config2.name = "Web-clone"
        config2.description = "Website cloning agent"
        manager.get_agent_configs.return_value = [config1, config2]
        return manager

    def test_spawn_subagent_tool_name(self):
        """Test that spawn_subagent tool name is correct."""
        assert TASK_TOOL_NAME == "spawn_subagent"

    def test_create_tool_schema_structure(self, mock_manager):
        """Test that spawn_subagent tool schema has correct structure."""
        schema = create_task_tool_schema(mock_manager)

        assert schema["type"] == "function"
        assert "function" in schema
        assert schema["function"]["name"] == "spawn_subagent"
        assert "description" in schema["function"]
        assert "parameters" in schema["function"]

    def test_tool_schema_parameters(self, mock_manager):
        """Test that spawn_subagent tool has required parameters."""
        schema = create_task_tool_schema(mock_manager)
        params = schema["function"]["parameters"]

        assert params["type"] == "object"
        assert "description" in params["properties"]
        assert "subagent_type" in params["properties"]
        assert "description" in params["required"]
        assert "subagent_type" in params["required"]

    def test_tool_schema_subagent_enum(self, mock_manager):
        """Test that subagent_type has correct enum values."""
        schema = create_task_tool_schema(mock_manager)
        subagent_type = schema["function"]["parameters"]["properties"]["subagent_type"]

        assert "enum" in subagent_type
        assert "Code-Explorer" in subagent_type["enum"]
        assert "Web-clone" in subagent_type["enum"]

    def test_tool_schema_description_includes_subagents(self, mock_manager):
        """Test that tool description lists available subagents."""
        schema = create_task_tool_schema(mock_manager)
        description = schema["function"]["description"]

        assert "Code-Explorer" in description
        assert "Web-clone" in description


class TestToolRegistryIntegration:
    """Tests for spawn_subagent tool integration with ToolRegistry.

    These tests require full ToolRegistry import which depends on LSP modules.
    Skip if LSP modules are not available.
    """

    @pytest.fixture(autouse=True)
    def skip_if_lsp_unavailable(self):
        """Skip tests if LSP modules are not available."""
        try:
            from swecli.core.context_engineering.tools.registry import ToolRegistry  # noqa: F401
        except (ImportError, ModuleNotFoundError) as e:
            pytest.skip(f"ToolRegistry import failed: {e}")

    def test_tool_registry_set_subagent_manager(self):
        """Test setting subagent manager on tool registry."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        mock_manager = MagicMock()

        registry.set_subagent_manager(mock_manager)

        assert registry._subagent_manager == mock_manager

    def test_tool_registry_get_subagent_manager(self):
        """Test getting subagent manager from tool registry."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        mock_manager = MagicMock()
        registry.set_subagent_manager(mock_manager)

        assert registry.get_subagent_manager() == mock_manager

    def test_tool_registry_spawn_subagent_handler_exists(self):
        """Test that spawn_subagent handler is registered."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        assert "spawn_subagent" in registry._handlers

    def test_execute_spawn_subagent_without_manager(self):
        """Test executing spawn_subagent tool without manager returns error."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        # Don't set subagent manager

        result = registry.execute_tool(
            "spawn_subagent",
            {"description": "Test task", "subagent_type": "Code-Explorer"},
        )

        assert result["success"] is False
        assert "not configured" in result["error"]

    def test_execute_spawn_subagent_without_description(self):
        """Test executing spawn_subagent tool without description returns error."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        mock_manager = MagicMock()
        registry.set_subagent_manager(mock_manager)

        result = registry.execute_tool(
            "spawn_subagent",
            {"subagent_type": "Code-Explorer"},
        )

        assert result["success"] is False
        assert "prompt" in result["error"].lower()


def _can_import_agent_factory():
    """Check if AgentFactory can be imported."""
    try:
        from swecli.core.base.factories.agent_factory import AgentFactory  # noqa: F401
        return True
    except (ImportError, ModuleNotFoundError):
        return False


@pytest.mark.skipif(not _can_import_agent_factory(), reason="AgentFactory import requires LSP modules")
class TestAgentFactoryIntegration:
    """Tests for subagent integration with AgentFactory.

    These tests require full AgentFactory import which depends on ToolRegistry.
    Skip if required modules are not available.
    """

    @pytest.fixture
    def mock_config(self):
        """Create a mock AppConfig."""
        config = MagicMock(spec=AppConfig)
        config.model = "gpt-4o"
        config.temperature = 0.7
        config.max_tokens = 4096
        config.api_key = "test-key"
        config.api_base_url = None
        return config

    @pytest.fixture
    def mock_tool_registry(self):
        """Create a mock tool registry."""
        registry = MagicMock()
        registry.set_subagent_manager = MagicMock()
        return registry

    @pytest.fixture
    def mock_mode_manager(self):
        """Create a mock mode manager."""
        return MagicMock()

    @patch("swecli.core.base.factories.agent_factory.MainAgent")
    @patch("swecli.core.base.factories.agent_factory.SubAgentManager")
    def test_agent_factory_creates_subagent_manager(
        self, mock_subagent_manager_class, mock_swecli,
        mock_config, mock_tool_registry, mock_mode_manager
    ):
        """Test that AgentFactory creates SubAgentManager when enabled."""
        from swecli.core.base.factories.agent_factory import AgentFactory

        mock_manager_instance = MagicMock()
        mock_subagent_manager_class.return_value = mock_manager_instance

        factory = AgentFactory(
            config=mock_config,
            tool_registry=mock_tool_registry,
            mode_manager=mock_mode_manager,
            enable_subagents=True,
        )
        suite = factory.create_agents()

        mock_subagent_manager_class.assert_called_once()
        mock_manager_instance.register_defaults.assert_called_once()
        mock_tool_registry.set_subagent_manager.assert_called_once_with(mock_manager_instance)
        assert suite.subagent_manager == mock_manager_instance

    @patch("swecli.core.base.factories.agent_factory.MainAgent")
    def test_agent_factory_skips_subagents_when_disabled(
        self, mock_swecli,
        mock_config, mock_tool_registry, mock_mode_manager
    ):
        """Test that AgentFactory skips SubAgentManager when disabled."""
        from swecli.core.base.factories.agent_factory import AgentFactory

        factory = AgentFactory(
            config=mock_config,
            tool_registry=mock_tool_registry,
            mode_manager=mock_mode_manager,
            enable_subagents=False,
        )
        suite = factory.create_agents()

        mock_tool_registry.set_subagent_manager.assert_not_called()
        assert suite.subagent_manager is None


class TestNestedUICallback:
    """Tests for NestedUICallback wrapper class."""

    def test_nested_callback_initialization(self):
        """Test that NestedUICallback initializes correctly."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        parent_callback = MagicMock()
        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        assert callback._parent == parent_callback
        assert callback._context == "researcher"
        assert callback._depth == 1

    def test_nested_callback_forwards_tool_call(self):
        """Test that on_tool_call forwards to parent with nesting info."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        parent_callback = MagicMock()
        parent_callback.on_nested_tool_call = MagicMock()

        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        callback.on_tool_call("read_file", {"path": "/test/file.py"})

        # Verify on_nested_tool_call was called with expected args
        # tool_id is auto-generated so we use ANY
        from unittest.mock import ANY
        parent_callback.on_nested_tool_call.assert_called_once_with(
            "read_file",
            {"path": "/test/file.py"},
            depth=1,
            parent="researcher",
            tool_id=ANY,
        )

    def test_nested_callback_forwards_tool_result(self):
        """Test that on_tool_result forwards to parent with nesting info."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        parent_callback = MagicMock()
        parent_callback.on_nested_tool_result = MagicMock()

        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        result = {"success": True, "output": "file content"}
        callback.on_tool_result("read_file", {"path": "/test/file.py"}, result)

        # Verify on_nested_tool_result was called with expected args
        # tool_id is auto-generated so we use ANY
        from unittest.mock import ANY
        parent_callback.on_nested_tool_result.assert_called_once_with(
            "read_file",
            {"path": "/test/file.py"},
            result,
            depth=1,
            parent="researcher",
            tool_id=ANY,
        )

    def test_nested_callback_fallback_to_regular_methods(self):
        """Test fallback to regular methods when nested methods unavailable."""
        from swecli.ui_textual.nested_callback import NestedUICallback
        from unittest.mock import ANY

        parent_callback = MagicMock(spec=["on_tool_call", "on_tool_result"])

        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        callback.on_tool_call("read_file", {"path": "/test/file.py"})
        # Fallback passes tool_call_id as third argument
        parent_callback.on_tool_call.assert_called_once_with("read_file", {"path": "/test/file.py"}, ANY)

    def test_nested_callback_handles_none_parent(self):
        """Test that NestedUICallback handles None parent gracefully."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        callback = NestedUICallback(
            parent_callback=None,
            parent_context="researcher",
            depth=1,
        )

        # Should not raise
        callback.on_tool_call("read_file", {"path": "/test/file.py"})
        callback.on_tool_result("read_file", {}, {"success": True})

    def test_nested_callback_create_nested(self):
        """Test creating further nested callbacks."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        parent_callback = MagicMock()

        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        child_callback = callback.create_nested("sub-researcher")

        assert child_callback._parent == parent_callback
        assert child_callback._context == "sub-researcher"
        assert child_callback._depth == 2

    def test_nested_callback_suppresses_thinking_events(self):
        """Test that thinking events are suppressed for subagents."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        parent_callback = MagicMock()
        parent_callback.on_thinking_start = MagicMock()
        parent_callback.on_thinking_complete = MagicMock()

        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        # These should not forward to parent
        callback.on_thinking_start()
        callback.on_thinking_complete()

        parent_callback.on_thinking_start.assert_not_called()
        parent_callback.on_thinking_complete.assert_not_called()

    def test_nested_callback_suppresses_assistant_messages(self):
        """Test that assistant messages are suppressed for subagents."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        parent_callback = MagicMock()
        parent_callback.on_assistant_message = MagicMock()

        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        # Should not forward to parent (final result shown separately)
        callback.on_assistant_message("Intermediate thinking...")

        parent_callback.on_assistant_message.assert_not_called()

    def test_nested_callback_forwards_debug(self):
        """Test that debug messages are forwarded with context prefix."""
        from swecli.ui_textual.nested_callback import NestedUICallback

        parent_callback = MagicMock()
        parent_callback.on_debug = MagicMock()

        callback = NestedUICallback(
            parent_callback=parent_callback,
            parent_context="researcher",
            depth=1,
        )

        callback.on_debug("Found 5 files", "SEARCH")

        parent_callback.on_debug.assert_called_once_with(
            "[researcher] Found 5 files",
            "SEARCH"
        )
