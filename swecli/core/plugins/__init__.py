"""Plugin and marketplace system for OpenDev."""

from swecli.core.plugins.models import (
    MarketplaceInfo,
    KnownMarketplaces,
    PluginMetadata,
    InstalledPlugin,
    InstalledPlugins,
    SkillMetadata,
    DirectPlugin,
    DirectPlugins,
)
from swecli.core.plugins.manager import (
    PluginManager,
    PluginManagerError,
    MarketplaceNotFoundError,
    PluginNotFoundError,
    BundleNotFoundError,
)
from swecli.core.plugins.config import (
    load_known_marketplaces,
    save_known_marketplaces,
    load_installed_plugins,
    save_installed_plugins,
    get_all_installed_plugins,
    load_direct_plugins,
    save_direct_plugins,
    get_all_direct_plugins,
)

__all__ = [
    # Models
    "MarketplaceInfo",
    "KnownMarketplaces",
    "PluginMetadata",
    "InstalledPlugin",
    "InstalledPlugins",
    "SkillMetadata",
    "DirectPlugin",
    "DirectPlugins",
    # Manager
    "PluginManager",
    "PluginManagerError",
    "MarketplaceNotFoundError",
    "PluginNotFoundError",
    "BundleNotFoundError",
    # Config
    "load_known_marketplaces",
    "save_known_marketplaces",
    "load_installed_plugins",
    "save_installed_plugins",
    "get_all_installed_plugins",
    "load_direct_plugins",
    "save_direct_plugins",
    "get_all_direct_plugins",
]
