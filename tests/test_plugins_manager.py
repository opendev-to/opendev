"""Tests for plugin manager and marketplace system."""

import json
from datetime import datetime
from pathlib import Path
from unittest.mock import patch, MagicMock

import pytest

from swecli.core.plugins.models import (
    MarketplaceInfo,
    KnownMarketplaces,
    PluginMetadata,
    InstalledPlugin,
    InstalledPlugins,
    SkillMetadata,
)
from swecli.core.plugins.config import (
    load_known_marketplaces,
    save_known_marketplaces,
    load_installed_plugins,
    save_installed_plugins,
    get_all_installed_plugins,
)
from swecli.core.plugins.manager import (
    PluginManager,
    PluginManagerError,
    MarketplaceNotFoundError,
    PluginNotFoundError,
)
from swecli.core.paths import reset_paths, ENV_OPENDEV_DIR


class TestModels:
    """Test Pydantic models."""

    def test_marketplace_info_default_branch(self):
        """Test MarketplaceInfo defaults branch to main."""
        info = MarketplaceInfo(name="test", url="https://github.com/test/marketplace")
        assert info.branch == "main"
        assert info.last_updated is None

    def test_marketplace_info_with_all_fields(self):
        """Test MarketplaceInfo with all fields."""
        now = datetime.now()
        info = MarketplaceInfo(
            name="test",
            url="https://github.com/test/marketplace",
            branch="develop",
            added_at=now,
            last_updated=now,
        )
        assert info.name == "test"
        assert info.url == "https://github.com/test/marketplace"
        assert info.branch == "develop"
        assert info.added_at == now
        assert info.last_updated == now

    def test_known_marketplaces_empty(self):
        """Test KnownMarketplaces with empty marketplaces."""
        km = KnownMarketplaces()
        assert km.marketplaces == {}

    def test_plugin_metadata_minimal(self):
        """Test PluginMetadata with minimal fields."""
        meta = PluginMetadata(name="test-plugin", version="1.0.0")
        assert meta.name == "test-plugin"
        assert meta.version == "1.0.0"
        assert meta.description == ""
        assert meta.author is None
        assert meta.skills == []

    def test_plugin_metadata_full(self):
        """Test PluginMetadata with all fields."""
        meta = PluginMetadata(
            name="test-plugin",
            version="1.0.0",
            description="A test plugin",
            author="Test Author",
            skills=["skill1", "skill2"],
            repository="https://github.com/test/plugin",
            license="MIT",
        )
        assert meta.name == "test-plugin"
        assert meta.skills == ["skill1", "skill2"]

    def test_installed_plugin_default_enabled(self):
        """Test InstalledPlugin defaults to enabled."""
        plugin = InstalledPlugin(
            name="test",
            marketplace="test-mp",
            version="1.0.0",
            path="/path/to/plugin",
        )
        assert plugin.enabled is True
        assert plugin.scope == "user"

    def test_installed_plugins_registry(self):
        """Test InstalledPlugins registry operations."""
        registry = InstalledPlugins()

        # Add plugin
        plugin = InstalledPlugin(
            name="test",
            marketplace="test-mp",
            version="1.0.0",
            path="/path/to/plugin",
        )
        registry.add(plugin)

        # Get key
        assert registry.get_key("test-mp", "test") == "test-mp:test"

        # Get plugin
        result = registry.get("test-mp", "test")
        assert result is not None
        assert result.name == "test"

        # Remove plugin
        removed = registry.remove("test-mp", "test")
        assert removed is not None
        assert registry.get("test-mp", "test") is None

    def test_skill_metadata_display_name(self):
        """Test SkillMetadata display_name for plugin skills."""
        # Local skill
        local = SkillMetadata(
            name="my-skill",
            source="personal",
            path=Path("/path/to/skill.md"),
        )
        assert local.display_name == "my-skill"
        assert local.source_display == "personal"

        # Plugin skill
        plugin_skill = SkillMetadata(
            name="test-skill",
            source="plugin",
            plugin_name="superpowers",
            path=Path("/path/to/skill.md"),
        )
        assert plugin_skill.display_name == "superpowers:test-skill"
        assert plugin_skill.source_display == "plugin:superpowers"


class TestConfig:
    """Test config utilities."""

    def test_load_known_marketplaces_empty(self, tmp_path, monkeypatch):
        """Test loading when no file exists."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        result = load_known_marketplaces()
        assert isinstance(result, KnownMarketplaces)
        assert result.marketplaces == {}

    def test_save_and_load_known_marketplaces(self, tmp_path, monkeypatch):
        """Test saving and loading marketplaces."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Create and save
        marketplaces = KnownMarketplaces()
        marketplaces.marketplaces["test"] = MarketplaceInfo(
            name="test",
            url="https://github.com/test/marketplace",
        )
        save_known_marketplaces(marketplaces)

        # Load and verify
        loaded = load_known_marketplaces()
        assert "test" in loaded.marketplaces
        assert loaded.marketplaces["test"].url == "https://github.com/test/marketplace"

    def test_load_installed_plugins_empty(self, tmp_path, monkeypatch):
        """Test loading when no file exists."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        result = load_installed_plugins()
        assert isinstance(result, InstalledPlugins)
        assert result.plugins == {}

    def test_save_and_load_installed_plugins(self, tmp_path, monkeypatch):
        """Test saving and loading installed plugins."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Create and save
        plugins = InstalledPlugins()
        plugins.add(
            InstalledPlugin(
                name="test-plugin",
                marketplace="test-mp",
                version="1.0.0",
                path="/path/to/plugin",
            )
        )
        save_installed_plugins(plugins)

        # Load and verify
        loaded = load_installed_plugins()
        assert "test-mp:test-plugin" in loaded.plugins

    def test_get_all_installed_plugins(self, tmp_path, monkeypatch):
        """Test getting all installed plugins from both scopes."""
        swecli_dir = tmp_path / ".opendev"
        project_dir = tmp_path / "project"
        project_dir.mkdir()

        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Save user plugin
        user_plugins = InstalledPlugins()
        user_plugins.add(
            InstalledPlugin(
                name="user-plugin",
                marketplace="mp1",
                version="1.0.0",
                scope="user",
                path="/path/user",
            )
        )
        save_installed_plugins(user_plugins, working_dir=project_dir, scope="user")

        # Save project plugin
        project_plugins = InstalledPlugins()
        project_plugins.add(
            InstalledPlugin(
                name="project-plugin",
                marketplace="mp2",
                version="2.0.0",
                scope="project",
                path="/path/project",
            )
        )
        save_installed_plugins(project_plugins, working_dir=project_dir, scope="project")

        # Get all
        all_plugins = get_all_installed_plugins(working_dir=project_dir)
        assert len(all_plugins) == 2


class TestPluginManager:
    """Test PluginManager class."""

    def test_extract_name_from_url_https(self):
        """Test extracting name from HTTPS URL."""
        manager = PluginManager()

        # Extracts repo name, removes swecli- prefix and -marketplace suffix
        name = manager._extract_name_from_url("https://github.com/user/swecli-marketplace")
        assert name == "marketplace"

        name = manager._extract_name_from_url("https://github.com/user/awesome-plugins.git")
        assert name == "awesome-plugins"

        name = manager._extract_name_from_url("https://github.com/user/swecli-superpowers-marketplace.git")
        assert name == "superpowers"

    def test_extract_name_from_url_ssh(self):
        """Test extracting name from SSH URL."""
        manager = PluginManager()

        # Extracts repo name, removes swecli- prefix and -marketplace suffix
        name = manager._extract_name_from_url("git@github.com:user/swecli-marketplace.git")
        assert name == "marketplace"

        name = manager._extract_name_from_url("git@github.com:user/my-plugins.git")
        assert name == "my-plugins"

    def test_validate_marketplace_valid_swecli_dir(self, tmp_path):
        """Test validating marketplace with .opendev/marketplace.json."""
        manager = PluginManager()

        marketplace_dir = tmp_path / "test-mp"
        (marketplace_dir / ".opendev").mkdir(parents=True)
        (marketplace_dir / ".opendev" / "marketplace.json").write_text(
            '{"name": "test", "plugins": []}'
        )

        assert manager._validate_marketplace(marketplace_dir) is True

    def test_validate_marketplace_valid_root(self, tmp_path):
        """Test validating marketplace with root marketplace.json."""
        manager = PluginManager()

        marketplace_dir = tmp_path / "test-mp"
        marketplace_dir.mkdir()
        (marketplace_dir / "marketplace.json").write_text(
            '{"name": "test", "plugins": []}'
        )

        assert manager._validate_marketplace(marketplace_dir) is True

    def test_validate_marketplace_valid_legacy(self, tmp_path):
        """Test validating marketplace with .swecli-marketplace/ (legacy)."""
        manager = PluginManager()

        marketplace_dir = tmp_path / "test-mp"
        (marketplace_dir / ".swecli-marketplace").mkdir(parents=True)
        (marketplace_dir / ".swecli-marketplace" / "marketplace.json").write_text(
            '{"name": "test", "plugins": []}'
        )

        assert manager._validate_marketplace(marketplace_dir) is True

    def test_validate_marketplace_valid_skills_dir(self, tmp_path):
        """Test validating marketplace with skills/ directory (auto-discovery)."""
        manager = PluginManager()

        marketplace_dir = tmp_path / "test-mp"
        (marketplace_dir / "skills" / "test-skill").mkdir(parents=True)
        (marketplace_dir / "skills" / "test-skill" / "SKILL.md").write_text("# Test")

        assert manager._validate_marketplace(marketplace_dir) is True

    def test_validate_marketplace_valid_plugins_dir(self, tmp_path):
        """Test validating marketplace with plugins/ directory (auto-discovery)."""
        manager = PluginManager()

        marketplace_dir = tmp_path / "test-mp"
        (marketplace_dir / "plugins" / "test-plugin").mkdir(parents=True)

        assert manager._validate_marketplace(marketplace_dir) is True

    def test_validate_marketplace_invalid(self, tmp_path):
        """Test validating an invalid marketplace structure."""
        manager = PluginManager()

        # Create invalid structure (missing marketplace.json and no plugins/skills)
        marketplace_dir = tmp_path / "invalid-mp"
        marketplace_dir.mkdir()

        assert manager._validate_marketplace(marketplace_dir) is False

    def test_load_plugin_metadata(self, tmp_path):
        """Test loading plugin metadata."""
        manager = PluginManager()

        # Create plugin with metadata
        plugin_dir = tmp_path / "test-plugin"
        (plugin_dir / ".swecli-plugin").mkdir(parents=True)
        (plugin_dir / ".swecli-plugin" / "plugin.json").write_text(
            json.dumps(
                {
                    "name": "test-plugin",
                    "version": "1.0.0",
                    "description": "A test plugin",
                    "skills": ["skill1"],
                }
            )
        )

        metadata = manager._load_plugin_metadata(plugin_dir)
        assert metadata is not None
        assert metadata.name == "test-plugin"
        assert metadata.version == "1.0.0"
        assert metadata.skills == ["skill1"]

    def test_load_plugin_metadata_missing(self, tmp_path):
        """Test loading metadata when file is missing."""
        manager = PluginManager()

        plugin_dir = tmp_path / "no-metadata"
        plugin_dir.mkdir()

        assert manager._load_plugin_metadata(plugin_dir) is None

    def test_parse_skill_metadata(self, tmp_path):
        """Test parsing SKILL.md frontmatter."""
        manager = PluginManager()

        skill_file = tmp_path / "SKILL.md"
        skill_file.write_text(
            """---
name: test-skill
description: "A test skill"
---

# Test Skill

Content here.
"""
        )

        name, description = manager._parse_skill_metadata(skill_file)
        assert name == "test-skill"
        assert description == "A test skill"

    def test_estimate_tokens(self, tmp_path):
        """Test token estimation."""
        manager = PluginManager()

        file_path = tmp_path / "test.md"
        file_path.write_text("A" * 400)  # 400 characters

        tokens = manager._estimate_tokens(file_path)
        assert tokens == 100  # 400 / 4

    def test_list_marketplaces_empty(self, tmp_path, monkeypatch):
        """Test listing marketplaces when none exist."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        manager = PluginManager()
        marketplaces = manager.list_marketplaces()
        assert marketplaces == []

    @patch("swecli.core.plugins.manager.subprocess.run")
    def test_add_marketplace_success(self, mock_run, tmp_path, monkeypatch):
        """Test adding a marketplace successfully."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Pre-create the marketplace structure that git clone would create
        marketplace_dir = swecli_dir / "plugins" / "marketplaces" / "test"

        def mock_clone(*args, **kwargs):
            """Mock git clone by creating the expected structure."""
            (marketplace_dir / ".swecli-marketplace").mkdir(parents=True)
            (marketplace_dir / ".swecli-marketplace" / "marketplace.json").write_text(
                '{"plugins": []}'
            )
            return MagicMock(returncode=0, stderr="")

        mock_run.side_effect = mock_clone

        manager = PluginManager()
        info = manager.add_marketplace("https://github.com/user/test", name="test")
        assert info.name == "test"
        assert info.url == "https://github.com/user/test"

    def test_add_marketplace_duplicate(self, tmp_path, monkeypatch):
        """Test adding a marketplace that already exists."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Pre-register a marketplace
        marketplaces = KnownMarketplaces()
        marketplaces.marketplaces["test"] = MarketplaceInfo(
            name="test",
            url="https://github.com/user/test",
        )
        save_known_marketplaces(marketplaces)

        manager = PluginManager()
        with pytest.raises(PluginManagerError, match="already exists"):
            manager.add_marketplace("https://github.com/user/test", name="test")

    def test_remove_marketplace_not_found(self, tmp_path, monkeypatch):
        """Test removing a marketplace that doesn't exist."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        manager = PluginManager()
        with pytest.raises(MarketplaceNotFoundError):
            manager.remove_marketplace("nonexistent")

    def test_sync_marketplace_not_found(self, tmp_path, monkeypatch):
        """Test syncing a marketplace that doesn't exist."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        manager = PluginManager()
        with pytest.raises(MarketplaceNotFoundError):
            manager.sync_marketplace("nonexistent")

    def test_list_installed_empty(self, tmp_path, monkeypatch):
        """Test listing installed plugins when none exist."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        manager = PluginManager()
        plugins = manager.list_installed()
        assert plugins == []

    def test_install_plugin_marketplace_not_found(self, tmp_path, monkeypatch):
        """Test installing from non-existent marketplace."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        manager = PluginManager()
        with pytest.raises(MarketplaceNotFoundError):
            manager.install_plugin("test-plugin", "nonexistent")

    def test_uninstall_plugin_not_found(self, tmp_path, monkeypatch):
        """Test uninstalling a plugin that isn't installed."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        manager = PluginManager()
        with pytest.raises(PluginNotFoundError):
            manager.uninstall_plugin("nonexistent", "nonexistent-mp")

    def test_get_plugin_skills_empty(self, tmp_path, monkeypatch):
        """Test getting skills when no plugins are installed."""
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(tmp_path / ".opendev"))
        reset_paths()

        manager = PluginManager()
        skills = manager.get_plugin_skills()
        assert skills == []


class TestPluginManagerIntegration:
    """Integration tests for plugin manager with mocked git."""

    def setup_marketplace(self, tmp_path, name="test-mp"):
        """Helper to set up a mock marketplace."""
        swecli_dir = tmp_path / ".opendev"
        marketplace_dir = swecli_dir / "plugins" / "marketplaces" / name

        # Create marketplace structure
        (marketplace_dir / ".swecli-marketplace").mkdir(parents=True)
        (marketplace_dir / ".swecli-marketplace" / "marketplace.json").write_text(
            json.dumps({"name": name, "plugins": ["plugin1"]})
        )

        # Create plugin
        plugin_dir = marketplace_dir / "plugins" / "plugin1"
        (plugin_dir / ".swecli-plugin").mkdir(parents=True)
        (plugin_dir / ".swecli-plugin" / "plugin.json").write_text(
            json.dumps(
                {
                    "name": "plugin1",
                    "version": "1.0.0",
                    "description": "Test plugin",
                    "skills": ["skill1"],
                }
            )
        )

        # Create skill
        skill_dir = plugin_dir / "skills" / "skill1"
        skill_dir.mkdir(parents=True)
        (skill_dir / "SKILL.md").write_text(
            """---
name: skill1
description: "Test skill"
---

# Test Skill
"""
        )

        return marketplace_dir

    def test_install_and_list_plugin(self, tmp_path, monkeypatch):
        """Test installing a plugin and listing installed plugins."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Setup marketplace
        self.setup_marketplace(tmp_path)

        # Register marketplace
        marketplaces = KnownMarketplaces()
        marketplaces.marketplaces["test-mp"] = MarketplaceInfo(
            name="test-mp",
            url="https://github.com/test/marketplace",
        )
        save_known_marketplaces(marketplaces)

        # Install plugin
        manager = PluginManager()
        installed = manager.install_plugin("plugin1", "test-mp")

        assert installed.name == "plugin1"
        assert installed.version == "1.0.0"
        assert installed.marketplace == "test-mp"

        # List installed
        plugins = manager.list_installed()
        assert len(plugins) == 1
        assert plugins[0].name == "plugin1"

    def test_get_plugin_skills_with_installed_plugin(self, tmp_path, monkeypatch):
        """Test getting skills from an installed plugin."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Setup marketplace
        self.setup_marketplace(tmp_path)

        # Register marketplace
        marketplaces = KnownMarketplaces()
        marketplaces.marketplaces["test-mp"] = MarketplaceInfo(
            name="test-mp",
            url="https://github.com/test/marketplace",
        )
        save_known_marketplaces(marketplaces)

        # Install plugin
        manager = PluginManager()
        manager.install_plugin("plugin1", "test-mp")

        # Get skills
        skills = manager.get_plugin_skills()
        assert len(skills) == 1
        assert skills[0].name == "skill1"
        assert skills[0].plugin_name == "plugin1"
        assert skills[0].display_name == "plugin1:skill1"

    def test_enable_disable_plugin(self, tmp_path, monkeypatch):
        """Test enabling and disabling a plugin."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Setup marketplace
        self.setup_marketplace(tmp_path)

        # Register marketplace
        marketplaces = KnownMarketplaces()
        marketplaces.marketplaces["test-mp"] = MarketplaceInfo(
            name="test-mp",
            url="https://github.com/test/marketplace",
        )
        save_known_marketplaces(marketplaces)

        # Install plugin
        manager = PluginManager()
        manager.install_plugin("plugin1", "test-mp")

        # Disable
        manager.disable_plugin("plugin1", "test-mp")
        plugins = manager.list_installed()
        assert plugins[0].enabled is False

        # Skills should be empty when disabled
        skills = manager.get_plugin_skills()
        assert len(skills) == 0

        # Enable
        manager.enable_plugin("plugin1", "test-mp")
        plugins = manager.list_installed()
        assert plugins[0].enabled is True

    def test_uninstall_plugin(self, tmp_path, monkeypatch):
        """Test uninstalling a plugin."""
        swecli_dir = tmp_path / ".opendev"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(swecli_dir))
        reset_paths()

        # Setup marketplace
        self.setup_marketplace(tmp_path)

        # Register marketplace
        marketplaces = KnownMarketplaces()
        marketplaces.marketplaces["test-mp"] = MarketplaceInfo(
            name="test-mp",
            url="https://github.com/test/marketplace",
        )
        save_known_marketplaces(marketplaces)

        # Install plugin
        manager = PluginManager()
        manager.install_plugin("plugin1", "test-mp")
        assert len(manager.list_installed()) == 1

        # Uninstall
        manager.uninstall_plugin("plugin1", "test-mp")
        assert len(manager.list_installed()) == 0
