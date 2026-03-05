"""Tests for the ModelConfigManager component."""

from types import SimpleNamespace
from unittest.mock import MagicMock, AsyncMock, patch

import pytest

from swecli.ui_textual.runner_components.model_config_manager import ModelConfigManager


class MockConfig:
    """Mock configuration for testing."""

    def __init__(self):
        self.model_provider = "openai"
        self.model = "gpt-4"
        self.model_thinking_provider = "anthropic"
        self.model_thinking = "claude-3"
        self.model_vlm_provider = "openai"
        self.model_vlm = "gpt-4-vision"
        self.model_critique_provider = "anthropic"
        self.model_critique = "claude-3-critique"
        self.model_compact_provider = None
        self.model_compact = None


class MockConfigManager:
    """Mock configuration manager for testing."""

    def __init__(self, config=None):
        self._config = config or MockConfig()

    def get_config(self):
        return self._config


class MockRepl:
    """Mock REPL for testing."""

    def __init__(self):
        self.config_commands = MagicMock()
        self.rebuild_agents = MagicMock()


class MockApp:
    """Mock Textual app for testing."""

    def __init__(self):
        self.update_primary_model_called = False
        self.update_model_slots_called = False
        self._primary_model = None
        self._model_slots = None

    def update_primary_model(self, model):
        self.update_primary_model_called = True
        self._primary_model = model

    def update_model_slots(self, slots):
        self.update_model_slots_called = True
        self._model_slots = slots


@pytest.fixture
def mock_config():
    """Create a mock config."""
    return MockConfig()


@pytest.fixture
def mock_config_manager(mock_config):
    """Create a mock config manager."""
    return MockConfigManager(mock_config)


@pytest.fixture
def mock_repl():
    """Create a mock REPL."""
    return MockRepl()


@pytest.fixture
def mock_app():
    """Create a mock app."""
    return MockApp()


@pytest.fixture
def manager(mock_config_manager, mock_repl):
    """Create a ModelConfigManager for testing."""
    return ModelConfigManager(mock_config_manager, mock_repl)


class TestModelConfigManager:
    """Test suite for ModelConfigManager."""

    def test_init(self, mock_config_manager, mock_repl):
        """Test manager initialization."""
        manager = ModelConfigManager(mock_config_manager, mock_repl)
        assert manager._config_manager is mock_config_manager
        assert manager._repl is mock_repl

    def test_get_model_config_snapshot_basic(self, manager):
        """Test getting config snapshot."""
        snapshot = manager.get_model_config_snapshot()

        assert "normal" in snapshot
        assert snapshot["normal"]["provider"] == "openai"
        assert snapshot["normal"]["model"] == "gpt-4"
        assert "provider_display" in snapshot["normal"]
        assert "model_display" in snapshot["normal"]

    def test_get_model_config_snapshot_with_thinking(self, manager):
        """Test snapshot includes thinking slot."""
        snapshot = manager.get_model_config_snapshot()

        assert "thinking" in snapshot
        assert snapshot["thinking"]["provider"] == "anthropic"
        assert snapshot["thinking"]["model"] == "claude-3"

    def test_get_model_config_snapshot_with_vision(self, manager):
        """Test snapshot includes vision slot."""
        snapshot = manager.get_model_config_snapshot()

        assert "vision" in snapshot
        assert snapshot["vision"]["provider"] == "openai"
        assert snapshot["vision"]["model"] == "gpt-4-vision"


class TestBuildModelSlots:
    """Test suite for _build_model_slots method."""

    def test_build_model_slots_all(self, manager):
        """Test building slots with all models configured."""
        slots = manager._build_model_slots()

        assert "normal" in slots
        assert slots["normal"] == ("OpenAI", "GPT-4")

        assert "thinking" in slots
        assert slots["thinking"] == ("Anthropic", "claude-3")

        assert "vision" in slots
        assert slots["vision"] == ("OpenAI", "gpt-4-vision")

        assert "critique" in slots
        assert slots["critique"] == ("Anthropic", "claude-3-critique")

    def test_build_model_slots_normal_only(self, mock_repl):
        """Test building slots with only normal model."""
        config = MockConfig()
        config.model_thinking = None
        config.model_vlm = None
        config.model_critique = None
        config_manager = MockConfigManager(config)

        manager = ModelConfigManager(config_manager, mock_repl)
        slots = manager._build_model_slots()

        assert "normal" in slots
        assert "thinking" not in slots
        assert "vision" not in slots

    def test_build_model_slots_same_as_normal(self, mock_repl):
        """Test that specialized slots are shown even if same model as normal."""
        config = MockConfig()
        config.model_thinking = "gpt-4"  # Same as normal
        config.model_vlm = "gpt-4"  # Same as normal
        config.model_critique = "gpt-4"  # Same as normal
        config_manager = MockConfigManager(config)

        manager = ModelConfigManager(config_manager, mock_repl)
        slots = manager._build_model_slots()

        # All slots should be present even if they use the same model
        assert "normal" in slots
        assert "thinking" in slots
        assert "vision" in slots
        assert "critique" in slots


class TestApplySelection:
    """Test suite for apply_model_selection method."""

    def test_apply_selection_success(self, mock_config_manager, mock_repl):
        """Test applying model selection successfully."""
        import asyncio
        
        manager = ModelConfigManager(mock_config_manager, mock_repl)
        
        mock_result = MagicMock()
        mock_result.success = True
        mock_repl.config_commands._switch_to_model = MagicMock(return_value=mock_result)

        result = asyncio.run(manager.apply_model_selection("normal", "openai", "gpt-4-turbo"))

        mock_repl.config_commands._switch_to_model.assert_called_once_with(
            "openai", "gpt-4-turbo", "normal"
        )
        mock_repl.rebuild_agents.assert_called_once()
        assert result.success is True

    def test_apply_selection_failure(self, mock_config_manager, mock_repl):
        """Test applying model selection that fails."""
        import asyncio
        
        manager = ModelConfigManager(mock_config_manager, mock_repl)
        
        mock_result = MagicMock()
        mock_result.success = False
        mock_repl.config_commands._switch_to_model = MagicMock(return_value=mock_result)

        result = asyncio.run(manager.apply_model_selection("normal", "invalid", "model"))

        mock_repl.config_commands._switch_to_model.assert_called_once()
        mock_repl.rebuild_agents.assert_not_called()
        assert result.success is False


class TestRefreshUIConfig:
    """Test suite for refresh_ui_config method."""

    def test_refresh_ui_config(self, manager, mock_app):
        """Test refreshing UI config."""
        manager.set_app(mock_app)
        manager.refresh_ui_config()

        assert mock_app.update_primary_model_called is True
        assert mock_app._primary_model == "OpenAI/GPT-4"
        assert mock_app.update_model_slots_called is True
        assert "normal" in mock_app._model_slots

    def test_refresh_ui_config_missing_methods(self, manager):
        """Test refresh with app missing update methods."""
        app = object()  # No update methods
        manager.set_app(app)

        # Should not raise
        manager.refresh_ui_config()
