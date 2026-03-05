"""Plugin Manager for handling marketplace and plugin operations."""

import json
import re
import shutil
import subprocess
from datetime import datetime
from pathlib import Path
from typing import Literal, Optional
from urllib.parse import urlparse

from swecli.core.paths import get_paths
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


class PluginManagerError(Exception):
    """Base exception for plugin manager errors."""

    pass


class MarketplaceNotFoundError(PluginManagerError):
    """Marketplace not found error."""

    pass


class PluginNotFoundError(PluginManagerError):
    """Plugin not found error."""

    pass


class BundleNotFoundError(PluginManagerError):
    """Bundle not found error."""

    pass


class PluginManager:
    """Manager for marketplace and plugin operations."""

    def __init__(self, working_dir: Optional[Path] = None):
        """Initialize plugin manager.

        Args:
            working_dir: Working directory for path resolution
        """
        self.working_dir = working_dir
        self.paths = get_paths(working_dir)

    def add_marketplace(
        self, url: str, name: Optional[str] = None, branch: str = "main"
    ) -> MarketplaceInfo:
        """Add a marketplace by cloning its repository.

        Args:
            url: Git URL of the marketplace repository
            name: Optional name for the marketplace (derived from URL if not provided)
            branch: Git branch to track (default: main)

        Returns:
            MarketplaceInfo for the added marketplace

        Raises:
            PluginManagerError: If cloning fails or marketplace is invalid
        """
        # Derive name from URL if not provided
        if not name:
            name = self._extract_name_from_url(url)

        # Check if marketplace already exists
        marketplaces = load_known_marketplaces(self.working_dir)
        if name in marketplaces.marketplaces:
            raise PluginManagerError(
                f"Marketplace '{name}' already exists. Use 'sync' to update it."
            )

        # Prepare target directory
        target_dir = self.paths.global_marketplaces_dir / name
        if target_dir.exists():
            shutil.rmtree(target_dir)

        # Clone repository
        try:
            result = subprocess.run(
                ["git", "clone", "--depth", "1", "--branch", branch, url, str(target_dir)],
                capture_output=True,
                text=True,
                timeout=120,
            )
            if result.returncode != 0:
                raise PluginManagerError(f"Git clone failed: {result.stderr}")
        except subprocess.TimeoutExpired:
            raise PluginManagerError("Git clone timed out")
        except FileNotFoundError:
            raise PluginManagerError("Git is not installed or not in PATH")

        # Validate marketplace structure
        if not self._validate_marketplace(target_dir):
            shutil.rmtree(target_dir)
            raise PluginManagerError(
                "Invalid marketplace: no marketplace.json found. "
                "Expected one of: .opendev/marketplace.json, marketplace.json"
            )

        # Register marketplace
        info = MarketplaceInfo(
            name=name,
            url=url,
            branch=branch,
            added_at=datetime.now(),
            last_updated=datetime.now(),
        )
        marketplaces.marketplaces[name] = info
        save_known_marketplaces(marketplaces, self.working_dir)

        return info

    def remove_marketplace(self, name: str) -> None:
        """Remove a marketplace.

        Args:
            name: Marketplace name

        Raises:
            MarketplaceNotFoundError: If marketplace doesn't exist
        """
        marketplaces = load_known_marketplaces(self.working_dir)
        if name not in marketplaces.marketplaces:
            raise MarketplaceNotFoundError(f"Marketplace '{name}' not found")

        # Remove directory
        marketplace_dir = self.paths.global_marketplaces_dir / name
        if marketplace_dir.exists():
            shutil.rmtree(marketplace_dir)

        # Remove from registry
        del marketplaces.marketplaces[name]
        save_known_marketplaces(marketplaces, self.working_dir)

    def list_marketplaces(self) -> list[MarketplaceInfo]:
        """List all registered marketplaces.

        Returns:
            List of MarketplaceInfo objects
        """
        marketplaces = load_known_marketplaces(self.working_dir)
        return list(marketplaces.marketplaces.values())

    def sync_marketplace(self, name: str) -> None:
        """Sync (git pull) a marketplace.

        Args:
            name: Marketplace name

        Raises:
            MarketplaceNotFoundError: If marketplace doesn't exist
            PluginManagerError: If sync fails
        """
        marketplaces = load_known_marketplaces(self.working_dir)
        if name not in marketplaces.marketplaces:
            raise MarketplaceNotFoundError(f"Marketplace '{name}' not found")

        marketplace_dir = self.paths.global_marketplaces_dir / name
        if not marketplace_dir.exists():
            raise PluginManagerError(f"Marketplace directory missing: {marketplace_dir}")

        # Git pull
        try:
            result = subprocess.run(
                ["git", "pull"],
                cwd=str(marketplace_dir),
                capture_output=True,
                text=True,
                timeout=60,
            )
            if result.returncode != 0:
                raise PluginManagerError(f"Git pull failed: {result.stderr}")
        except subprocess.TimeoutExpired:
            raise PluginManagerError("Git pull timed out")

        # Update last_updated timestamp
        marketplaces.marketplaces[name].last_updated = datetime.now()
        save_known_marketplaces(marketplaces, self.working_dir)

    def sync_all_marketplaces(self) -> dict[str, Optional[str]]:
        """Sync all registered marketplaces.

        Returns:
            Dict of marketplace name to error message (None if successful)
        """
        results = {}
        for marketplace in self.list_marketplaces():
            try:
                self.sync_marketplace(marketplace.name)
                results[marketplace.name] = None
            except Exception as e:
                results[marketplace.name] = str(e)
        return results

    def get_marketplace_catalog(self, name: str) -> dict:
        """Get the plugin catalog from a marketplace.

        If no marketplace.json exists, auto-discovers plugins from plugins/ directory.

        Args:
            name: Marketplace name

        Returns:
            Catalog dict from marketplace.json or auto-generated

        Raises:
            MarketplaceNotFoundError: If marketplace doesn't exist
        """
        marketplaces = load_known_marketplaces(self.working_dir)
        if name not in marketplaces.marketplaces:
            raise MarketplaceNotFoundError(f"Marketplace '{name}' not found")

        marketplace_dir = self.paths.global_marketplaces_dir / name
        catalog_path = self._get_marketplace_json_path(marketplace_dir)

        if catalog_path is not None:
            return json.loads(catalog_path.read_text(encoding="utf-8"))

        # Auto-discover plugins from plugins/ or skills/ directories
        return self._auto_discover_catalog(marketplace_dir)

    def _auto_discover_catalog(self, marketplace_dir: Path) -> dict:
        """Auto-discover plugins when no marketplace.json exists.

        Args:
            marketplace_dir: Marketplace directory

        Returns:
            Auto-generated catalog dict
        """
        plugins = []

        # Check plugins/ directory
        plugins_dir = marketplace_dir / "plugins"
        if plugins_dir.exists() and plugins_dir.is_dir():
            for item in plugins_dir.iterdir():
                if item.is_dir():
                    plugins.append(item.name)

        # Check skills/ directory (treat each skill as a single-skill plugin)
        skills_dir = marketplace_dir / "skills"
        if skills_dir.exists() and skills_dir.is_dir():
            for item in skills_dir.iterdir():
                if item.is_dir() and (item / "SKILL.md").exists():
                    plugins.append(item.name)

        return {"plugins": plugins, "auto_discovered": True}

    def list_marketplace_plugins(self, name: str) -> list[PluginMetadata]:
        """List all plugins available in a marketplace.

        Args:
            name: Marketplace name

        Returns:
            List of PluginMetadata objects
        """
        catalog = self.get_marketplace_catalog(name)
        plugins = []
        marketplace_dir = self.paths.global_marketplaces_dir / name

        # Check plugins/ directory
        plugins_dir = marketplace_dir / "plugins"
        if plugins_dir.exists():
            for plugin_name in catalog.get("plugins", []):
                plugin_dir = plugins_dir / plugin_name
                if plugin_dir.exists():
                    metadata = self._load_plugin_metadata(plugin_dir)
                    if metadata:
                        plugins.append(metadata)
                    else:
                        # Create metadata from directory if no plugin.json
                        plugins.append(
                            PluginMetadata(
                                name=plugin_name,
                                version="0.0.0",
                                description=f"Plugin: {plugin_name}",
                                skills=self._discover_skills_in_dir(plugin_dir),
                            )
                        )

        # Check skills/ directory (each skill is treated as a plugin)
        skills_dir = marketplace_dir / "skills"
        if skills_dir.exists() and catalog.get("auto_discovered"):
            for skill_name in catalog.get("plugins", []):
                skill_dir = skills_dir / skill_name
                if skill_dir.exists() and (skill_dir / "SKILL.md").exists():
                    # Already listed from plugins/ check above? Skip duplicate
                    if any(p.name == skill_name for p in plugins):
                        continue
                    name_from_skill, desc = self._parse_skill_metadata(skill_dir / "SKILL.md")
                    plugins.append(
                        PluginMetadata(
                            name=skill_name,
                            version="0.0.0",
                            description=desc or f"Skill: {skill_name}",
                            skills=[skill_name],
                        )
                    )

        return plugins

    def _discover_skills_in_dir(self, plugin_dir: Path) -> list[str]:
        """Discover skill names in a plugin directory.

        Args:
            plugin_dir: Plugin directory path

        Returns:
            List of skill names
        """
        skills = []
        skills_dir = plugin_dir / "skills"
        if skills_dir.exists():
            for item in skills_dir.iterdir():
                if item.is_dir() and (item / "SKILL.md").exists():
                    skills.append(item.name)
        return skills

    def install_plugin(
        self,
        plugin_name: str,
        marketplace: str,
        scope: Literal["user", "project"] = "user",
        version: Optional[str] = None,
    ) -> InstalledPlugin:
        """Install a plugin from a marketplace.

        Args:
            plugin_name: Plugin name
            marketplace: Marketplace name
            scope: Installation scope ('user' or 'project')
            version: Specific version (default: latest)

        Returns:
            InstalledPlugin for the installed plugin

        Raises:
            PluginNotFoundError: If plugin doesn't exist in marketplace
            PluginManagerError: If installation fails
        """
        # Verify marketplace exists
        marketplaces = load_known_marketplaces(self.working_dir)
        if marketplace not in marketplaces.marketplaces:
            raise MarketplaceNotFoundError(f"Marketplace '{marketplace}' not found")

        marketplace_dir = self.paths.global_marketplaces_dir / marketplace

        # Find plugin in marketplace - check plugins/ first, then skills/
        source_dir = marketplace_dir / "plugins" / plugin_name
        is_skill_as_plugin = False

        if not source_dir.exists():
            # Check if it's a skill in the skills/ directory
            skill_dir = marketplace_dir / "skills" / plugin_name
            if skill_dir.exists() and (skill_dir / "SKILL.md").exists():
                source_dir = skill_dir
                is_skill_as_plugin = True
            else:
                raise PluginNotFoundError(f"Plugin '{plugin_name}' not found in '{marketplace}'")

        # Load plugin metadata
        metadata = self._load_plugin_metadata(source_dir)
        if not metadata and not is_skill_as_plugin:
            # For regular plugins, metadata is required
            raise PluginManagerError(f"Invalid plugin: missing plugin.json")

        if is_skill_as_plugin:
            # Create metadata for skill-as-plugin
            skill_name, skill_desc = self._parse_skill_metadata(source_dir / "SKILL.md")
            plugin_version = version or "0.0.0"
        else:
            plugin_version = version or metadata.version

        # Determine target directory based on scope
        if scope == "project":
            cache_dir = self.paths.project_plugins_dir / "cache"
        else:
            cache_dir = self.paths.global_plugin_cache_dir

        target_dir = cache_dir / marketplace / plugin_name / plugin_version

        # Copy plugin to cache
        if target_dir.exists():
            shutil.rmtree(target_dir)
        target_dir.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source_dir, target_dir, dirs_exist_ok=True)

        # Register installation
        installed = InstalledPlugin(
            name=plugin_name,
            marketplace=marketplace,
            version=plugin_version,
            scope=scope,
            path=str(target_dir),
            enabled=True,
            installed_at=datetime.now(),
        )

        plugins = load_installed_plugins(self.working_dir, scope=scope)
        plugins.add(installed)
        save_installed_plugins(plugins, self.working_dir, scope=scope)

        return installed

    def uninstall_plugin(
        self, plugin_name: str, marketplace: str, scope: Literal["user", "project"] = "user"
    ) -> None:
        """Uninstall a plugin.

        Args:
            plugin_name: Plugin name
            marketplace: Marketplace name
            scope: Installation scope

        Raises:
            PluginNotFoundError: If plugin isn't installed
        """
        plugins = load_installed_plugins(self.working_dir, scope=scope)
        plugin = plugins.get(marketplace, plugin_name)

        if not plugin:
            raise PluginNotFoundError(
                f"Plugin '{marketplace}:{plugin_name}' not installed in {scope} scope"
            )

        # Remove from cache
        plugin_path = Path(plugin.path)
        if plugin_path.exists():
            shutil.rmtree(plugin_path)

        # Remove from registry
        plugins.remove(marketplace, plugin_name)
        save_installed_plugins(plugins, self.working_dir, scope=scope)

    def update_plugin(
        self, plugin_name: str, marketplace: str, scope: Literal["user", "project"] = "user"
    ) -> InstalledPlugin:
        """Update a plugin to the latest version.

        Args:
            plugin_name: Plugin name
            marketplace: Marketplace name
            scope: Installation scope

        Returns:
            Updated InstalledPlugin

        Raises:
            PluginNotFoundError: If plugin isn't installed
        """
        plugins = load_installed_plugins(self.working_dir, scope=scope)
        plugin = plugins.get(marketplace, plugin_name)

        if not plugin:
            raise PluginNotFoundError(
                f"Plugin '{marketplace}:{plugin_name}' not installed in {scope} scope"
            )

        # Sync marketplace first
        self.sync_marketplace(marketplace)

        # Reinstall (will get latest version)
        return self.install_plugin(plugin_name, marketplace, scope=scope)

    def list_installed(
        self, scope: Optional[Literal["user", "project"]] = None
    ) -> list[InstalledPlugin]:
        """List installed plugins.

        Args:
            scope: Optional scope filter ('user', 'project', or None for all)

        Returns:
            List of InstalledPlugin objects
        """
        if scope:
            plugins = load_installed_plugins(self.working_dir, scope=scope)
            return list(plugins.plugins.values())
        else:
            return get_all_installed_plugins(self.working_dir)

    def enable_plugin(
        self, plugin_name: str, marketplace: str, scope: Literal["user", "project"] = "user"
    ) -> None:
        """Enable a disabled plugin.

        Args:
            plugin_name: Plugin name
            marketplace: Marketplace name
            scope: Installation scope
        """
        plugins = load_installed_plugins(self.working_dir, scope=scope)
        plugin = plugins.get(marketplace, plugin_name)

        if not plugin:
            raise PluginNotFoundError(
                f"Plugin '{marketplace}:{plugin_name}' not installed in {scope} scope"
            )

        plugin.enabled = True
        save_installed_plugins(plugins, self.working_dir, scope=scope)

    def disable_plugin(
        self, plugin_name: str, marketplace: str, scope: Literal["user", "project"] = "user"
    ) -> None:
        """Disable a plugin.

        Args:
            plugin_name: Plugin name
            marketplace: Marketplace name
            scope: Installation scope
        """
        plugins = load_installed_plugins(self.working_dir, scope=scope)
        plugin = plugins.get(marketplace, plugin_name)

        if not plugin:
            raise PluginNotFoundError(
                f"Plugin '{marketplace}:{plugin_name}' not installed in {scope} scope"
            )

        plugin.enabled = False
        save_installed_plugins(plugins, self.working_dir, scope=scope)

    def get_plugin_skills(self) -> list[SkillMetadata]:
        """Get all skills from installed plugins and bundles.

        Returns:
            List of SkillMetadata objects for plugin and bundle skills
        """
        skills = []

        # Skills from marketplace plugins
        for plugin in self.list_installed():
            if not plugin.enabled:
                continue

            plugin_path = Path(plugin.path)
            skills_dir = plugin_path / "skills"

            if not skills_dir.exists():
                continue

            for skill_dir in skills_dir.iterdir():
                if not skill_dir.is_dir():
                    continue

                skill_file = skill_dir / "SKILL.md"
                if not skill_file.exists():
                    continue

                name, description = self._parse_skill_metadata(skill_file)
                if not name:
                    name = skill_dir.name

                # Calculate token count
                token_count = self._estimate_tokens(skill_file)

                skills.append(
                    SkillMetadata(
                        name=name,
                        description=description,
                        source="plugin",
                        plugin_name=plugin.name,
                        path=skill_file,
                        token_count=token_count,
                    )
                )

        # Skills from direct bundles (URL installs)
        for bundle in self.list_bundles():
            if not bundle.enabled:
                continue

            bundle_path = Path(bundle.path)
            skills_dir = bundle_path / "skills"

            if not skills_dir.exists():
                continue

            for skill_dir in skills_dir.iterdir():
                if not skill_dir.is_dir():
                    continue

                skill_file = skill_dir / "SKILL.md"
                if not skill_file.exists():
                    continue

                name, description = self._parse_skill_metadata(skill_file)
                if not name:
                    name = skill_dir.name

                # Calculate token count
                token_count = self._estimate_tokens(skill_file)

                skills.append(
                    SkillMetadata(
                        name=name,
                        description=description,
                        source="bundle",
                        bundle_name=bundle.name,
                        path=skill_file,
                        token_count=token_count,
                    )
                )

        return skills

    # ========================================================================
    # Direct Bundle Methods (URL installs)
    # ========================================================================

    def install_from_url(
        self,
        url: str,
        scope: Literal["user", "project"] = "user",
        name: Optional[str] = None,
        branch: str = "main",
    ) -> DirectPlugin:
        """Install a plugin bundle directly from URL.

        This method auto-detects the repository type:
        - If skills/ exists at root with SKILL.md files, treat as direct bundle
        - Otherwise, provide guidance to use marketplace workflow

        Args:
            url: Git URL of the repository
            scope: Installation scope ('user' or 'project')
            name: Optional name for the bundle (derived from URL if not provided)
            branch: Git branch to track (default: main)

        Returns:
            DirectPlugin for the installed bundle

        Raises:
            PluginManagerError: If installation fails or repo is a marketplace
        """
        # Derive name from URL if not provided
        if not name:
            name = self._extract_name_from_url(url)

        # Check if bundle already exists
        existing = self._get_bundle(name, scope)
        if existing:
            raise PluginManagerError(f"Bundle '{name}' already installed. Use 'sync' to update it.")

        # Create temp directory for cloning
        import tempfile

        temp_dir = Path(tempfile.mkdtemp())

        try:
            # Clone repository
            result = subprocess.run(
                ["git", "clone", "--depth", "1", "--branch", branch, url, str(temp_dir)],
                capture_output=True,
                text=True,
                timeout=120,
            )
            if result.returncode != 0:
                raise PluginManagerError(f"Git clone failed: {result.stderr}")

            # Detect repo type
            repo_type = self._detect_repo_type(temp_dir)

            if repo_type == "marketplace":
                # Clean up temp dir
                shutil.rmtree(temp_dir)
                raise PluginManagerError(
                    f"This repository appears to be a marketplace (has plugins/ directory).\n"
                    f"Use the marketplace workflow instead:\n"
                    f"  /plugins marketplace add {url}\n"
                    f"  /plugins install <plugin>@{name}"
                )

            # Move to bundles directory
            if scope == "project":
                bundles_dir = self.paths.project_bundles_dir
            else:
                bundles_dir = self.paths.global_bundles_dir

            target_dir = bundles_dir / name
            if target_dir.exists():
                shutil.rmtree(target_dir)

            bundles_dir.mkdir(parents=True, exist_ok=True)
            shutil.move(str(temp_dir), str(target_dir))

            # Register bundle
            bundle = DirectPlugin(
                name=name,
                url=url,
                branch=branch,
                scope=scope,
                path=str(target_dir),
                enabled=True,
                installed_at=datetime.now(),
            )

            bundles = load_direct_plugins(self.working_dir, scope=scope)
            bundles.add(bundle)
            save_direct_plugins(bundles, self.working_dir, scope=scope)

            return bundle

        except subprocess.TimeoutExpired:
            shutil.rmtree(temp_dir, ignore_errors=True)
            raise PluginManagerError("Git clone timed out")
        except FileNotFoundError:
            shutil.rmtree(temp_dir, ignore_errors=True)
            raise PluginManagerError("Git is not installed or not in PATH")
        except PluginManagerError:
            raise
        except Exception as e:
            shutil.rmtree(temp_dir, ignore_errors=True)
            raise PluginManagerError(f"Installation failed: {e}")

    def _detect_repo_type(self, directory: Path) -> Literal["direct", "marketplace"]:
        """Detect if a repository is a direct bundle or marketplace.

        A direct bundle has skills/ at root with SKILL.md files.
        A marketplace has plugins/ directory or marketplace.json.

        Args:
            directory: Repository directory to check

        Returns:
            'direct' if skills bundle, 'marketplace' if marketplace repo
        """
        # Check for marketplace indicators
        plugins_dir = directory / "plugins"
        if plugins_dir.exists() and plugins_dir.is_dir():
            return "marketplace"

        marketplace_paths = [
            directory / ".opendev" / "marketplace.json",
            directory / "marketplace.json",
            directory / ".swecli" / "marketplace.json",  # legacy fallback
            directory / ".swecli-marketplace" / "marketplace.json",  # legacy fallback
        ]
        if any(p.exists() for p in marketplace_paths):
            return "marketplace"

        # Check for direct bundle (skills/ at root with SKILL.md files)
        skills_dir = directory / "skills"
        if skills_dir.exists() and skills_dir.is_dir():
            # Verify at least one SKILL.md exists
            for item in skills_dir.iterdir():
                if item.is_dir() and (item / "SKILL.md").exists():
                    return "direct"

        # Default to direct (treat unknown repos as potential skill bundles)
        return "direct"

    def list_bundles(
        self, scope: Optional[Literal["user", "project"]] = None
    ) -> list[DirectPlugin]:
        """List installed bundles.

        Args:
            scope: Optional scope filter ('user', 'project', or None for all)

        Returns:
            List of DirectPlugin objects
        """
        if scope:
            bundles = load_direct_plugins(self.working_dir, scope=scope)
            return list(bundles.bundles.values())
        else:
            return get_all_direct_plugins(self.working_dir)

    def _get_bundle(
        self, name: str, scope: Optional[Literal["user", "project"]] = None
    ) -> Optional[DirectPlugin]:
        """Get a specific bundle by name.

        Args:
            name: Bundle name
            scope: Optional scope to search (None = search both)

        Returns:
            DirectPlugin or None if not found
        """
        if scope:
            bundles = load_direct_plugins(self.working_dir, scope=scope)
            return bundles.get(name)
        else:
            # Search project first, then user
            project_bundles = load_direct_plugins(self.working_dir, scope="project")
            if name in project_bundles.bundles:
                return project_bundles.bundles[name]

            user_bundles = load_direct_plugins(self.working_dir, scope="user")
            return user_bundles.get(name)

    def uninstall_bundle(self, name: str) -> None:
        """Uninstall a bundle.

        Args:
            name: Bundle name

        Raises:
            BundleNotFoundError: If bundle isn't installed
        """
        # Find bundle in either scope
        for scope in ["project", "user"]:
            bundles = load_direct_plugins(self.working_dir, scope=scope)
            bundle = bundles.get(name)

            if bundle:
                # Remove directory
                bundle_path = Path(bundle.path)
                if bundle_path.exists():
                    shutil.rmtree(bundle_path)

                # Remove from registry
                bundles.remove(name)
                save_direct_plugins(bundles, self.working_dir, scope=scope)
                return

        raise BundleNotFoundError(f"Bundle '{name}' not found")

    def sync_bundle(self, name: str) -> None:
        """Sync (git pull) a bundle.

        Args:
            name: Bundle name

        Raises:
            BundleNotFoundError: If bundle doesn't exist
            PluginManagerError: If sync fails
        """
        bundle = self._get_bundle(name)
        if not bundle:
            raise BundleNotFoundError(f"Bundle '{name}' not found")

        bundle_dir = Path(bundle.path)
        if not bundle_dir.exists():
            raise PluginManagerError(f"Bundle directory missing: {bundle_dir}")

        # Git pull
        try:
            result = subprocess.run(
                ["git", "pull"],
                cwd=str(bundle_dir),
                capture_output=True,
                text=True,
                timeout=60,
            )
            if result.returncode != 0:
                raise PluginManagerError(f"Git pull failed: {result.stderr}")
        except subprocess.TimeoutExpired:
            raise PluginManagerError("Git pull timed out")

    def sync_all_bundles(self) -> dict[str, Optional[str]]:
        """Sync all installed bundles.

        Returns:
            Dict of bundle name to error message (None if successful)
        """
        results = {}
        for bundle in self.list_bundles():
            try:
                self.sync_bundle(bundle.name)
                results[bundle.name] = None
            except Exception as e:
                results[bundle.name] = str(e)
        return results

    def enable_bundle(self, name: str) -> None:
        """Enable a disabled bundle.

        Args:
            name: Bundle name

        Raises:
            BundleNotFoundError: If bundle doesn't exist
        """
        for scope in ["project", "user"]:
            bundles = load_direct_plugins(self.working_dir, scope=scope)
            bundle = bundles.get(name)
            if bundle:
                bundle.enabled = True
                save_direct_plugins(bundles, self.working_dir, scope=scope)
                return

        raise BundleNotFoundError(f"Bundle '{name}' not found")

    def disable_bundle(self, name: str) -> None:
        """Disable a bundle.

        Args:
            name: Bundle name

        Raises:
            BundleNotFoundError: If bundle doesn't exist
        """
        for scope in ["project", "user"]:
            bundles = load_direct_plugins(self.working_dir, scope=scope)
            bundle = bundles.get(name)
            if bundle:
                bundle.enabled = False
                save_direct_plugins(bundles, self.working_dir, scope=scope)
                return

        raise BundleNotFoundError(f"Bundle '{name}' not found")

    def _extract_name_from_url(self, url: str) -> str:
        """Extract marketplace name from URL.

        Args:
            url: Git URL

        Returns:
            Derived name
        """
        # Handle various URL formats
        # https://github.com/user/swecli-marketplace
        # git@github.com:user/swecli-marketplace.git

        # Remove .git suffix
        url = re.sub(r"\.git$", "", url)

        # Extract repo name
        parsed = urlparse(url)
        if parsed.path:
            parts = parsed.path.strip("/").split("/")
            if parts:
                name = parts[-1]
                # Remove common prefixes/suffixes
                name = re.sub(r"^swecli-", "", name)
                name = re.sub(r"-marketplace$", "", name)
                return name or "default"

        # Handle SSH-style URLs
        if "@" in url and ":" in url:
            parts = url.split(":")[-1].strip("/").split("/")
            if parts:
                name = parts[-1]
                name = re.sub(r"^swecli-", "", name)
                name = re.sub(r"-marketplace$", "", name)
                return name or "default"

        return "default"

    def _validate_marketplace(self, directory: Path) -> bool:
        """Validate marketplace directory structure.

        Checks multiple possible locations for marketplace.json:
        1. .opendev/marketplace.json (consistent with app)
        2. marketplace.json (at root)
        3. .swecli/marketplace.json (legacy fallback)
        4. .swecli-marketplace/marketplace.json (legacy fallback)

        Also accepts repos with plugins/ or skills/ directories (auto-discovery mode).

        Args:
            directory: Marketplace directory

        Returns:
            True if valid, False otherwise
        """
        # Check for marketplace.json
        possible_paths = [
            directory / ".opendev" / "marketplace.json",
            directory / "marketplace.json",
            directory / ".swecli" / "marketplace.json",  # legacy fallback
            directory / ".swecli-marketplace" / "marketplace.json",  # legacy fallback
        ]
        if any(p.exists() for p in possible_paths):
            return True

        # Auto-discovery: accept if plugins/ or skills/ directory exists
        plugins_dir = directory / "plugins"
        skills_dir = directory / "skills"
        if plugins_dir.exists() and plugins_dir.is_dir():
            return True
        if skills_dir.exists() and skills_dir.is_dir():
            return True

        return False

    def _get_marketplace_json_path(self, directory: Path) -> Optional[Path]:
        """Find the marketplace.json file in a marketplace directory.

        Args:
            directory: Marketplace directory

        Returns:
            Path to marketplace.json or None if not found
        """
        possible_paths = [
            directory / ".opendev" / "marketplace.json",
            directory / "marketplace.json",
            directory / ".swecli" / "marketplace.json",  # legacy fallback
            directory / ".swecli-marketplace" / "marketplace.json",  # legacy fallback
        ]
        for p in possible_paths:
            if p.exists():
                return p
        return None

    def _load_plugin_metadata(self, plugin_dir: Path) -> Optional[PluginMetadata]:
        """Load plugin metadata from plugin.json.

        Checks multiple possible locations:
        1. .opendev/plugin.json (consistent with app)
        2. plugin.json (at root)
        3. .swecli/plugin.json (legacy fallback)
        4. .swecli-plugin/plugin.json (legacy fallback)

        Args:
            plugin_dir: Plugin directory

        Returns:
            PluginMetadata or None if invalid
        """
        possible_paths = [
            plugin_dir / ".opendev" / "plugin.json",
            plugin_dir / "plugin.json",
            plugin_dir / ".swecli" / "plugin.json",  # legacy fallback
            plugin_dir / ".swecli-plugin" / "plugin.json",  # legacy fallback
        ]

        metadata_file = None
        for p in possible_paths:
            if p.exists():
                metadata_file = p
                break

        if metadata_file is None:
            return None

        try:
            data = json.loads(metadata_file.read_text(encoding="utf-8"))
            return PluginMetadata.model_validate(data)
        except Exception:
            return None

    def _parse_skill_metadata(self, skill_file: Path) -> tuple[str, str]:
        """Parse SKILL.md for name and description.

        Args:
            skill_file: Path to SKILL.md

        Returns:
            Tuple of (name, description)
        """
        try:
            content = skill_file.read_text(encoding="utf-8")
            name = ""
            description = ""

            if content.startswith("---"):
                parts = content.split("---", 2)
                if len(parts) >= 3:
                    frontmatter = parts[1]
                    for line in frontmatter.strip().split("\n"):
                        if line.startswith("name:"):
                            name = line.split(":", 1)[1].strip().strip("\"'")
                        elif line.startswith("description:"):
                            description = line.split(":", 1)[1].strip().strip("\"'")

            return name, description
        except Exception:
            return "", ""

    def _estimate_tokens(self, file_path: Path) -> int:
        """Estimate token count for a file.

        Uses a simple heuristic: ~4 characters per token.

        Args:
            file_path: Path to file

        Returns:
            Estimated token count
        """
        try:
            content = file_path.read_text(encoding="utf-8")
            # Rough estimate: 4 characters per token
            return len(content) // 4
        except Exception:
            return 0
