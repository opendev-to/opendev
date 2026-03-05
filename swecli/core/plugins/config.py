"""Configuration utilities for loading and saving plugin registry files."""

import json
from pathlib import Path
from typing import Optional

from swecli.core.paths import get_paths
from swecli.core.plugins.models import (
    KnownMarketplaces,
    InstalledPlugins,
    MarketplaceInfo,
    InstalledPlugin,
    DirectPlugin,
    DirectPlugins,
)


def load_known_marketplaces(working_dir: Optional[Path] = None) -> KnownMarketplaces:
    """Load the known marketplaces registry.

    Args:
        working_dir: Working directory for path resolution

    Returns:
        KnownMarketplaces with loaded data or empty registry
    """
    paths = get_paths(working_dir)
    file_path = paths.known_marketplaces_file

    if not file_path.exists():
        return KnownMarketplaces()

    try:
        data = json.loads(file_path.read_text(encoding="utf-8"))
        # Convert datetime strings back to datetime objects
        for name, info in data.get("marketplaces", {}).items():
            if "added_at" in info and isinstance(info["added_at"], str):
                from datetime import datetime

                info["added_at"] = datetime.fromisoformat(info["added_at"])
            if "last_updated" in info and isinstance(info["last_updated"], str):
                from datetime import datetime

                info["last_updated"] = datetime.fromisoformat(info["last_updated"])
        return KnownMarketplaces.model_validate(data)
    except Exception:
        return KnownMarketplaces()


def save_known_marketplaces(
    marketplaces: KnownMarketplaces, working_dir: Optional[Path] = None
) -> None:
    """Save the known marketplaces registry.

    Args:
        marketplaces: KnownMarketplaces to save
        working_dir: Working directory for path resolution
    """
    paths = get_paths(working_dir)
    file_path = paths.known_marketplaces_file

    # Ensure directory exists
    file_path.parent.mkdir(parents=True, exist_ok=True)

    # Convert to JSON-serializable format
    data = marketplaces.model_dump(mode="json")
    file_path.write_text(json.dumps(data, indent=2, default=str), encoding="utf-8")


def load_installed_plugins(
    working_dir: Optional[Path] = None, scope: str = "user"
) -> InstalledPlugins:
    """Load the installed plugins registry.

    Args:
        working_dir: Working directory for path resolution
        scope: 'user' for global plugins, 'project' for project-specific

    Returns:
        InstalledPlugins with loaded data or empty registry
    """
    paths = get_paths(working_dir)

    if scope == "project":
        file_path = paths.project_installed_plugins_file
    else:
        file_path = paths.global_installed_plugins_file

    if not file_path.exists():
        return InstalledPlugins()

    try:
        data = json.loads(file_path.read_text(encoding="utf-8"))
        # Convert datetime strings
        for key, plugin in data.get("plugins", {}).items():
            if "installed_at" in plugin and isinstance(plugin["installed_at"], str):
                from datetime import datetime

                plugin["installed_at"] = datetime.fromisoformat(plugin["installed_at"])
        return InstalledPlugins.model_validate(data)
    except Exception:
        return InstalledPlugins()


def save_installed_plugins(
    plugins: InstalledPlugins, working_dir: Optional[Path] = None, scope: str = "user"
) -> None:
    """Save the installed plugins registry.

    Args:
        plugins: InstalledPlugins to save
        working_dir: Working directory for path resolution
        scope: 'user' for global plugins, 'project' for project-specific
    """
    paths = get_paths(working_dir)

    if scope == "project":
        file_path = paths.project_installed_plugins_file
    else:
        file_path = paths.global_installed_plugins_file

    # Ensure directory exists
    file_path.parent.mkdir(parents=True, exist_ok=True)

    # Convert to JSON-serializable format
    data = plugins.model_dump(mode="json")
    file_path.write_text(json.dumps(data, indent=2, default=str), encoding="utf-8")


def get_all_installed_plugins(working_dir: Optional[Path] = None) -> list[InstalledPlugin]:
    """Get all installed plugins from both user and project scopes.

    Args:
        working_dir: Working directory for path resolution

    Returns:
        List of all installed plugins (project plugins first, then user)
    """
    plugins = []

    # Load project plugins first (higher priority)
    project_plugins = load_installed_plugins(working_dir, scope="project")
    plugins.extend(project_plugins.plugins.values())

    # Load user plugins
    user_plugins = load_installed_plugins(working_dir, scope="user")

    # Only add user plugins that aren't overridden by project plugins
    project_keys = set(project_plugins.plugins.keys())
    for key, plugin in user_plugins.plugins.items():
        if key not in project_keys:
            plugins.append(plugin)

    return plugins


def load_direct_plugins(working_dir: Optional[Path] = None, scope: str = "user") -> DirectPlugins:
    """Load the direct plugins (bundles) registry.

    Args:
        working_dir: Working directory for path resolution
        scope: 'user' for global bundles, 'project' for project-specific

    Returns:
        DirectPlugins with loaded data or empty registry
    """
    paths = get_paths(working_dir)

    if scope == "project":
        file_path = paths.project_bundles_file
    else:
        file_path = paths.global_bundles_file

    if not file_path.exists():
        return DirectPlugins()

    try:
        data = json.loads(file_path.read_text(encoding="utf-8"))
        # Convert datetime strings
        for name, bundle in data.get("bundles", {}).items():
            if "installed_at" in bundle and isinstance(bundle["installed_at"], str):
                from datetime import datetime

                bundle["installed_at"] = datetime.fromisoformat(bundle["installed_at"])
        return DirectPlugins.model_validate(data)
    except Exception:
        return DirectPlugins()


def save_direct_plugins(
    plugins: DirectPlugins, working_dir: Optional[Path] = None, scope: str = "user"
) -> None:
    """Save the direct plugins (bundles) registry.

    Args:
        plugins: DirectPlugins to save
        working_dir: Working directory for path resolution
        scope: 'user' for global bundles, 'project' for project-specific
    """
    paths = get_paths(working_dir)

    if scope == "project":
        file_path = paths.project_bundles_file
    else:
        file_path = paths.global_bundles_file

    # Ensure directory exists
    file_path.parent.mkdir(parents=True, exist_ok=True)

    # Convert to JSON-serializable format
    data = plugins.model_dump(mode="json")
    file_path.write_text(json.dumps(data, indent=2, default=str), encoding="utf-8")


def get_all_direct_plugins(working_dir: Optional[Path] = None) -> list[DirectPlugin]:
    """Get all direct plugin bundles from both user and project scopes.

    Args:
        working_dir: Working directory for path resolution

    Returns:
        List of all bundles (project bundles first, then user)
    """
    bundles = []

    # Load project bundles first (higher priority)
    project_bundles = load_direct_plugins(working_dir, scope="project")
    bundles.extend(project_bundles.bundles.values())

    # Load user bundles
    user_bundles = load_direct_plugins(working_dir, scope="user")

    # Only add user bundles that aren't overridden by project bundles
    project_names = set(project_bundles.bundles.keys())
    for name, bundle in user_bundles.bundles.items():
        if name not in project_names:
            bundles.append(bundle)

    return bundles
