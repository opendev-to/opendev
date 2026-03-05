import pytest
from typing import Any, Mapping
from unittest.mock import MagicMock, patch
import sys
import importlib

# Use local imports to avoid side effects

# --- BaseTool Tests ---

def test_base_tool_abstract():
    """Verify BaseTool cannot be instantiated directly."""
    from swecli.core.base.abstract.base_tool import BaseTool
    with pytest.raises(TypeError):
        BaseTool()

def test_base_tool_implementation():
    """Verify a concrete implementation of BaseTool works."""
    from swecli.core.base.abstract.base_tool import BaseTool

    class ConcreteTool(BaseTool):
        name = "test_tool"
        description = "A test tool"

        def run(self, **kwargs: Any) -> Mapping[str, Any]:
            return {"status": "success", "input": kwargs}

    tool = ConcreteTool()
    assert tool.name == "test_tool"
    assert tool.description == "A test tool"

    result = tool.run(param="value")
    assert result == {"status": "success", "input": {"param": "value"}}


# --- ToolFactory Tests ---

def test_tool_dependencies_dataclass():
    """Verify ToolDependencies stores values correctly."""
    # We can safely import ToolDependencies usually, but let's do it locally just in case
    # Note: ToolFactory module imports ToolRegistry at top level.
    # We need to make sure we don't trigger that import yet if we want to mock it later?
    # Actually, ToolDependencies is defined in the same file.

    # We'll just mock the import for this test too to be safe, or assume it's fine.
    # It's fine if we don't instantiate ToolFactory.

    # But wait, importing the module executes top level code.
    # So we should probably do the mocking dance here too if we want to be safe,
    # OR just rely on the fact that dataclasses don't depend on the other imports.
    # But the module *does* import ToolRegistry.

    # Let's mock it to be safe.
    with patch.dict(sys.modules, {'swecli.core.context_engineering.tools': MagicMock()}):
         from swecli.core.base.factories.tool_factory import ToolDependencies
         deps = ToolDependencies(
            file_ops="file_ops",
            write_tool="write_tool",
            edit_tool="edit_tool",
            bash_tool="bash_tool",
            web_fetch_tool="web_fetch_tool"
        )
         assert deps.file_ops == "file_ops"

def test_tool_factory_create_registry():
    """Verify ToolFactory creates a registry with correct dependencies."""

    # We need to patch the module 'swecli.core.context_engineering.tools' BEFORE importing ToolFactory
    mock_tools_module = MagicMock()
    MockToolRegistry = MagicMock()
    mock_tools_module.ToolRegistry = MockToolRegistry
    MockToolRegistry.return_value = "mock_registry_instance"

    # Ensure ToolFactory is not already in sys.modules, or reload it
    if 'swecli.core.base.factories.tool_factory' in sys.modules:
        del sys.modules['swecli.core.base.factories.tool_factory']

    # Use patch.dict to safely modify sys.modules temporarily
    with patch.dict(sys.modules, {'swecli.core.context_engineering.tools': mock_tools_module}):
        from swecli.core.base.factories.tool_factory import ToolFactory, ToolDependencies

        # Setup dependencies
        deps = ToolDependencies(
            file_ops=MagicMock(),
            write_tool=MagicMock(),
            edit_tool=MagicMock(),
            bash_tool=MagicMock(),
            web_fetch_tool=MagicMock(),
            web_search_tool=MagicMock(),
            notebook_edit_tool=MagicMock(),
            ask_user_tool=MagicMock(),
            open_browser_tool=MagicMock(),
            vlm_tool=MagicMock(),
            web_screenshot_tool=MagicMock()
        )

        factory = ToolFactory(deps)

        mcp_manager = MagicMock()

        # Execute
        registry = factory.create_registry(mcp_manager=mcp_manager)

        # Verify
        assert registry == "mock_registry_instance"

        MockToolRegistry.assert_called_once_with(
            file_ops=deps.file_ops,
            write_tool=deps.write_tool,
            edit_tool=deps.edit_tool,
            bash_tool=deps.bash_tool,
            web_fetch_tool=deps.web_fetch_tool,
            web_search_tool=deps.web_search_tool,
            notebook_edit_tool=deps.notebook_edit_tool,
            ask_user_tool=deps.ask_user_tool,
            open_browser_tool=deps.open_browser_tool,
            vlm_tool=deps.vlm_tool,
            web_screenshot_tool=deps.web_screenshot_tool,
            mcp_manager=mcp_manager
        )
