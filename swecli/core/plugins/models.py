"""Pydantic models for the plugin/marketplace system."""

from datetime import datetime
from pathlib import Path
from typing import Literal, Optional

from pydantic import BaseModel, Field


class MarketplaceInfo(BaseModel):
    """Information about a registered marketplace."""

    name: str = Field(..., description="Unique name for this marketplace")
    url: str = Field(..., description="Git URL of the marketplace repository")
    branch: str = Field(default="main", description="Git branch to track")
    added_at: datetime = Field(
        default_factory=datetime.now, description="When this marketplace was added"
    )
    last_updated: Optional[datetime] = Field(
        default=None, description="Last time marketplace was synced"
    )


class KnownMarketplaces(BaseModel):
    """Registry of known marketplaces."""

    marketplaces: dict[str, MarketplaceInfo] = Field(
        default_factory=dict, description="Map of marketplace name to info"
    )


class PluginMetadata(BaseModel):
    """Metadata for a plugin from its plugin.json file."""

    name: str = Field(..., description="Plugin name")
    version: str = Field(..., description="Plugin version (semver)")
    description: str = Field(default="", description="Plugin description")
    author: Optional[str] = Field(default=None, description="Plugin author")
    skills: list[str] = Field(default_factory=list, description="List of skill names in plugin")
    repository: Optional[str] = Field(default=None, description="Source repository URL")
    license: Optional[str] = Field(default=None, description="License identifier")


class InstalledPlugin(BaseModel):
    """Information about an installed plugin."""

    name: str = Field(..., description="Plugin name")
    marketplace: str = Field(..., description="Marketplace the plugin came from")
    version: str = Field(..., description="Installed version")
    scope: Literal["user", "project"] = Field(default="user", description="Installation scope")
    path: str = Field(..., description="Path to installed plugin")
    enabled: bool = Field(default=True, description="Whether plugin is enabled")
    installed_at: datetime = Field(
        default_factory=datetime.now, description="When plugin was installed"
    )


class InstalledPlugins(BaseModel):
    """Registry of installed plugins."""

    plugins: dict[str, InstalledPlugin] = Field(
        default_factory=dict, description="Map of 'marketplace:plugin' to InstalledPlugin"
    )

    def get_key(self, marketplace: str, plugin: str) -> str:
        """Generate registry key for a plugin."""
        return f"{marketplace}:{plugin}"

    def add(self, plugin: InstalledPlugin) -> None:
        """Add a plugin to the registry."""
        key = self.get_key(plugin.marketplace, plugin.name)
        self.plugins[key] = plugin

    def remove(self, marketplace: str, plugin: str) -> Optional[InstalledPlugin]:
        """Remove a plugin from the registry."""
        key = self.get_key(marketplace, plugin)
        return self.plugins.pop(key, None)

    def get(self, marketplace: str, plugin: str) -> Optional[InstalledPlugin]:
        """Get a plugin from the registry."""
        key = self.get_key(marketplace, plugin)
        return self.plugins.get(key)


class SkillMetadata(BaseModel):
    """Metadata for a skill, including source information."""

    name: str = Field(..., description="Skill name")
    description: str = Field(default="", description="Skill description")
    source: Literal["project", "personal", "plugin", "bundle"] = Field(
        ..., description="Where the skill comes from"
    )
    plugin_name: Optional[str] = Field(
        default=None, description="Plugin name if source is 'plugin'"
    )
    bundle_name: Optional[str] = Field(
        default=None, description="Bundle name if source is 'bundle'"
    )
    path: Path = Field(..., description="Path to SKILL.md")
    token_count: Optional[int] = Field(
        default=None, description="Approximate token count of SKILL.md"
    )

    @property
    def display_name(self) -> str:
        """Get display name with namespace for plugin/bundle skills."""
        if self.source == "plugin" and self.plugin_name:
            return f"{self.plugin_name}:{self.name}"
        if self.source == "bundle" and self.bundle_name:
            return f"{self.bundle_name}:{self.name}"
        return self.name

    @property
    def source_display(self) -> str:
        """Get display string for source."""
        if self.source == "plugin" and self.plugin_name:
            return f"plugin:{self.plugin_name}"
        if self.source == "bundle" and self.bundle_name:
            return f"bundle:{self.bundle_name}"
        return self.source


class DirectPlugin(BaseModel):
    """A directly-installed plugin bundle from URL."""

    name: str = Field(..., description="Unique name for this bundle")
    url: str = Field(..., description="Git URL of the bundle repository")
    branch: str = Field(default="main", description="Git branch to track")
    scope: Literal["user", "project"] = Field(default="user", description="Installation scope")
    path: str = Field(..., description="Path to installed bundle directory")
    enabled: bool = Field(default=True, description="Whether bundle is enabled")
    installed_at: datetime = Field(
        default_factory=datetime.now, description="When this bundle was installed"
    )


class DirectPlugins(BaseModel):
    """Registry of direct plugin bundles."""

    bundles: dict[str, DirectPlugin] = Field(
        default_factory=dict, description="Map of bundle name to DirectPlugin"
    )

    def add(self, bundle: DirectPlugin) -> None:
        """Add a bundle to the registry."""
        self.bundles[bundle.name] = bundle

    def get(self, name: str) -> Optional[DirectPlugin]:
        """Get a bundle from the registry."""
        return self.bundles.get(name)

    def remove(self, name: str) -> Optional[DirectPlugin]:
        """Remove a bundle from the registry."""
        return self.bundles.pop(name, None)
