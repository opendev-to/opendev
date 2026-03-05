"""Command handler for /plugins command to manage plugins and marketplaces."""

import sys
from typing import Any

from rich.console import Console
from rich.table import Table
from rich.prompt import Prompt, Confirm

from swecli.repl.commands.base import CommandHandler, CommandResult
from swecli.core.plugins import (
    PluginManager,
    PluginManagerError,
    MarketplaceNotFoundError,
    PluginNotFoundError,
    BundleNotFoundError,
)
from swecli.ui_textual.components.plugin_panels import create_scope_selection_panel


class PluginsCommands(CommandHandler):
    """Handler for /plugins command to manage plugins and marketplaces."""

    def __init__(
        self,
        console: Console,
        config_manager: Any,
        *,
        is_tui: bool = False,
    ):
        """Initialize plugins command handler.

        Args:
            console: Rich console for output
            config_manager: Configuration manager
            is_tui: Whether running inside the TUI (disables interactive prompts)
        """
        super().__init__(console)
        self.config_manager = config_manager
        self.plugin_manager = PluginManager(config_manager.working_dir)
        self._is_tui = is_tui

    def _can_prompt_interactively(self) -> bool:
        """Check if interactive prompts are available (not in TUI)."""
        return not self._is_tui and sys.stdin.isatty()

    def handle(self, args: str) -> CommandResult:
        """Handle /plugins command and subcommands.

        Args:
            args: Command arguments

        Returns:
            CommandResult with execution status
        """
        if not args:
            return self._show_menu()

        parts = args.split(maxsplit=1)
        subcmd = parts[0].lower()
        subcmd_args = parts[1] if len(parts) > 1 else ""

        if subcmd == "marketplace":
            return self._handle_marketplace(subcmd_args)
        elif subcmd == "install":
            return self._install_plugin(subcmd_args)
        elif subcmd == "uninstall" or subcmd == "remove":
            return self._uninstall_plugin(subcmd_args)
        elif subcmd == "update":
            return self._update_plugin(subcmd_args)
        elif subcmd == "list":
            return self._list_plugins()
        elif subcmd == "enable":
            return self._enable_plugin(subcmd_args)
        elif subcmd == "disable":
            return self._disable_plugin(subcmd_args)
        elif subcmd == "sync":
            return self._sync_plugin(subcmd_args)
        else:
            return self._show_menu()

    def _show_menu(self) -> CommandResult:
        """Show available plugins commands."""
        self.print_line("[cyan]/plugins list[/cyan]                    List installed plugins")
        self.print_continuation("[cyan]/plugins install <url>[/cyan]           Install plugin from URL")
        self.print_continuation("[cyan]/plugins uninstall <name>[/cyan]        Uninstall a plugin")
        self.print_continuation("[cyan]/plugins sync <name>[/cyan]             Update a plugin")
        self.print_continuation("[cyan]/plugins enable <name>[/cyan]           Enable a plugin")
        self.print_continuation("[cyan]/plugins disable <name>[/cyan]          Disable a plugin")
        self.console.print()

        return CommandResult(success=True)

    def _handle_marketplace(self, args: str) -> CommandResult:
        """Handle marketplace subcommands.

        Args:
            args: Marketplace subcommand arguments

        Returns:
            CommandResult with execution status
        """
        if not args:
            return self._list_marketplaces()

        parts = args.split(maxsplit=1)
        subcmd = parts[0].lower()
        subcmd_args = parts[1] if len(parts) > 1 else ""

        if subcmd == "add":
            return self._add_marketplace(subcmd_args)
        elif subcmd == "list":
            return self._list_marketplaces()
        elif subcmd == "sync":
            return self._sync_marketplace(subcmd_args)
        elif subcmd == "remove":
            return self._remove_marketplace(subcmd_args)
        elif subcmd == "plugins":
            return self._list_marketplace_plugins(subcmd_args)
        else:
            self.print_error(f"Unknown marketplace command: {subcmd}")
            return CommandResult(success=False, message=f"Unknown command: {subcmd}")

    def _add_marketplace(self, url: str) -> CommandResult:
        """Add a marketplace by URL.

        Args:
            url: Git URL of the marketplace

        Returns:
            CommandResult with execution status
        """
        if not url:
            self.print_error("URL required: /plugins marketplace add <url>")
            return CommandResult(success=False, message="URL required")

        # Optionally get name
        name = None
        if " " in url:
            parts = url.split(maxsplit=1)
            url = parts[0]
            if parts[1].startswith("--name="):
                name = parts[1].replace("--name=", "")

        self.print_command_header("Adding marketplace", url)

        try:
            info = self.plugin_manager.add_marketplace(url, name=name)
            self.print_success(f"Added marketplace: [cyan]{info.name}[/cyan]")
            self.print_continuation(f"URL: {info.url}")
            self.print_continuation(f"Branch: {info.branch}")
            self.console.print()

            # Show available plugins
            try:
                plugins = self.plugin_manager.list_marketplace_plugins(info.name)
                if plugins:
                    self.print_info(f"Found {len(plugins)} plugin(s):")
                    for plugin in plugins[:5]:
                        self.print_continuation(f"  - {plugin.name}: {plugin.description[:50]}...")
                    if len(plugins) > 5:
                        self.print_continuation(f"  ... and {len(plugins) - 5} more")
                    self.console.print()
            except Exception:
                pass

            return CommandResult(success=True, message=f"Added marketplace: {info.name}", data=info)

        except PluginManagerError as e:
            self.print_error(str(e))
            return CommandResult(success=False, message=str(e))

    def _list_marketplaces(self) -> CommandResult:
        """List known marketplaces.

        Returns:
            CommandResult with execution status
        """
        marketplaces = self.plugin_manager.list_marketplaces()

        if not marketplaces:
            self.print_info("No marketplaces registered.")
            self.print_continuation("Add one with: /plugins marketplace add <url>")
            self.console.print()
            return CommandResult(success=True)

        table = Table(show_header=True, header_style="bold")
        table.add_column("Name", style="cyan")
        table.add_column("URL")
        table.add_column("Branch", style="dim")
        table.add_column("Last Synced", style="dim")

        for mp in marketplaces:
            last_updated = (
                mp.last_updated.strftime("%Y-%m-%d %H:%M") if mp.last_updated else "Never"
            )
            table.add_row(mp.name, mp.url, mp.branch, last_updated)

        self.console.print(table)
        self.console.print()

        return CommandResult(success=True, data=marketplaces)

    def _sync_marketplace(self, name: str) -> CommandResult:
        """Sync marketplace(s).

        Args:
            name: Marketplace name or empty for all

        Returns:
            CommandResult with execution status
        """
        if not name:
            # Sync all
            self.print_command_header("Syncing all marketplaces")
            results = self.plugin_manager.sync_all_marketplaces()

            success_count = sum(1 for v in results.values() if v is None)
            fail_count = len(results) - success_count

            for mp_name, error in results.items():
                if error:
                    self.print_error(f"{mp_name}: {error}")
                else:
                    self.print_success(f"{mp_name}: synced")

            self.console.print()
            self.print_info(f"Synced {success_count}/{len(results)} marketplace(s)")
            self.console.print()

            return CommandResult(
                success=fail_count == 0,
                message=f"Synced {success_count}/{len(results)} marketplaces",
            )
        else:
            # Sync specific marketplace
            self.print_command_header("Syncing marketplace", name)

            try:
                self.plugin_manager.sync_marketplace(name)
                self.print_success(f"Synced: {name}")
                self.console.print()
                return CommandResult(success=True, message=f"Synced: {name}")
            except (MarketplaceNotFoundError, PluginManagerError) as e:
                self.print_error(str(e))
                return CommandResult(success=False, message=str(e))

    def _remove_marketplace(self, args: str) -> CommandResult:
        """Remove a marketplace.

        Args:
            args: Marketplace name and optional flags (--force/-f)

        Returns:
            CommandResult with execution status
        """
        # Parse --force flag
        parts = args.split()
        name = ""
        force = False
        for part in parts:
            if part in ("--force", "-f"):
                force = True
            elif not name:
                name = part

        if not name:
            self.print_error(
                "Marketplace name required: /plugins marketplace remove <name> [--force]"
            )
            return CommandResult(success=False, message="Name required")

        # Skip confirmation if --force or non-interactive (TUI)
        if not force and self._can_prompt_interactively():
            if not Confirm.ask(f"Remove marketplace '{name}'?"):
                self.print_info("Cancelled")
                return CommandResult(success=False, message="Cancelled")

        try:
            self.plugin_manager.remove_marketplace(name)
            self.print_success(f"Removed marketplace: {name}")
            self.console.print()
            return CommandResult(success=True, message=f"Removed: {name}")
        except MarketplaceNotFoundError as e:
            self.print_error(str(e))
            return CommandResult(success=False, message=str(e))

    def _list_marketplace_plugins(self, name: str) -> CommandResult:
        """List plugins available in a marketplace.

        Args:
            name: Marketplace name

        Returns:
            CommandResult with execution status
        """
        if not name:
            self.print_error("Marketplace name required: /plugins marketplace plugins <name>")
            return CommandResult(success=False, message="Name required")

        try:
            plugins = self.plugin_manager.list_marketplace_plugins(name)

            if not plugins:
                self.print_info(f"No plugins found in marketplace '{name}'")
                self.console.print()
                return CommandResult(success=True)

            table = Table(show_header=True, header_style="bold")
            table.add_column("Name", style="cyan")
            table.add_column("Version")
            table.add_column("Description")
            table.add_column("Skills", style="dim")

            for plugin in plugins:
                desc = (
                    plugin.description[:40] + "..."
                    if len(plugin.description) > 40
                    else plugin.description
                )
                skills = ", ".join(plugin.skills[:3])
                if len(plugin.skills) > 3:
                    skills += f" +{len(plugin.skills) - 3}"
                table.add_row(plugin.name, plugin.version, desc, skills)

            self.console.print(table)
            self.console.print()
            self.print_info(f"Found {len(plugins)} plugin(s) in '{name}'")
            self.console.print()

            return CommandResult(success=True, data=plugins)

        except (MarketplaceNotFoundError, PluginManagerError) as e:
            self.print_error(str(e))
            return CommandResult(success=False, message=str(e))

    def _install_plugin(self, spec: str) -> CommandResult:
        """Install a plugin from URL or marketplace.

        Args:
            spec: Either a URL or plugin spec in format <plugin>@<marketplace>

        Returns:
            CommandResult with execution status
        """
        if not spec:
            self.print_error(
                "Usage: /plugins install <url> or /plugins install <plugin>@<marketplace>"
            )
            return CommandResult(success=False, message="Spec required")

        # Check if it's a URL (direct bundle install)
        if spec.startswith(("http://", "https://", "git@")):
            return self._install_from_url(spec)

        # Traditional marketplace install
        if "@" not in spec:
            self.print_error(
                "Usage: /plugins install <url> or /plugins install <plugin>@<marketplace>"
            )
            return CommandResult(success=False, message="Invalid spec")

        plugin_name, marketplace = spec.rsplit("@", 1)

        # Ask for scope with styled panel
        panel = create_scope_selection_panel(0, str(self.config_manager.working_dir))
        self.console.print(panel)

        choice = Prompt.ask("Select", choices=["1", "2"], default="1")
        scope = "user" if choice == "1" else "project"

        self.print_command_header("Installing plugin", f"{plugin_name} from {marketplace}")

        try:
            installed = self.plugin_manager.install_plugin(plugin_name, marketplace, scope=scope)
            self.print_success(f"Installed: [cyan]{installed.name}[/cyan] v{installed.version}")
            self.print_continuation(f"Scope: {scope}")
            self.print_continuation(f"Path: {installed.path}")
            self.console.print()

            # Show installed skills
            skills = self.plugin_manager.get_plugin_skills()
            plugin_skills = [s for s in skills if s.plugin_name == plugin_name]
            if plugin_skills:
                self.print_info(f"Available skills ({len(plugin_skills)}):")
                for skill in plugin_skills:
                    self.print_continuation(f"  - {skill.display_name}")
                self.console.print()

            return CommandResult(success=True, message=f"Installed: {plugin_name}", data=installed)

        except (MarketplaceNotFoundError, PluginNotFoundError, PluginManagerError) as e:
            self.print_error(str(e))
            return CommandResult(success=False, message=str(e))

    def _install_from_url(self, args: str) -> CommandResult:
        """Install plugin bundle directly from URL.

        Supports:
        - /plugins install <url>
        - /plugins install <url> --project
        - /plugins install <url> --name=custom-name

        Args:
            args: URL and optional flags

        Returns:
            CommandResult with execution status
        """
        parts = args.split()
        url = parts[0]
        scope = "user"  # Default to user scope
        name = None

        for part in parts[1:]:
            if part == "--project":
                scope = "project"
            elif part.startswith("--name="):
                name = part.split("=", 1)[1]

        self.print_command_header("Installing from URL", url)

        try:
            bundle = self.plugin_manager.install_from_url(url, scope=scope, name=name)
            self.print_success(f"Installed: [cyan]{bundle.name}[/cyan]")
            self.print_continuation(f"Scope: {scope}")
            self.print_continuation(f"Path: {bundle.path}")
            self.console.print()

            # Show available skills from the bundle
            skills = self.plugin_manager.get_plugin_skills()
            bundle_skills = [s for s in skills if s.bundle_name == bundle.name]
            if bundle_skills:
                self.print_info(f"Available skills ({len(bundle_skills)}):")
                for skill in bundle_skills[:10]:
                    desc = (
                        skill.description[:50] + "..."
                        if len(skill.description) > 50
                        else skill.description
                    )
                    self.print_continuation(f"  - [cyan]{skill.display_name}[/cyan]: {desc}")
                if len(bundle_skills) > 10:
                    self.print_continuation(f"  ... and {len(bundle_skills) - 10} more")
                self.console.print()

            return CommandResult(success=True, message=f"Installed: {bundle.name}", data=bundle)

        except PluginManagerError as e:
            self.print_error(str(e))
            return CommandResult(success=False, message=str(e))

    def _uninstall_plugin(self, args: str) -> CommandResult:
        """Uninstall a plugin or bundle.

        Args:
            args: Plugin spec (<plugin>@<marketplace>) or bundle name, with optional --force/-f

        Returns:
            CommandResult with execution status
        """
        # Parse --force flag
        parts = args.split()
        spec = ""
        force = False
        for part in parts:
            if part in ("--force", "-f"):
                force = True
            elif not spec:
                spec = part

        if not spec:
            self.print_error(
                "Usage: /plugins uninstall <name> [--force] or /plugins uninstall <plugin>@<marketplace> [--force]"
            )
            return CommandResult(success=False, message="Name required")

        # Check if it's a bundle (no @ in spec)
        if "@" not in spec:
            return self._uninstall_bundle(spec, force=force)

        # Traditional marketplace plugin uninstall
        plugin_name, marketplace = spec.rsplit("@", 1)

        # Skip confirmation if --force or non-interactive (TUI)
        if not force and self._can_prompt_interactively():
            if not Confirm.ask(f"Uninstall plugin '{plugin_name}' from '{marketplace}'?"):
                self.print_info("Cancelled")
                return CommandResult(success=False, message="Cancelled")

        # Try both scopes
        for scope in ["project", "user"]:
            try:
                self.plugin_manager.uninstall_plugin(plugin_name, marketplace, scope=scope)
                self.print_success(f"Uninstalled: {plugin_name} ({scope} scope)")
                self.console.print()
                return CommandResult(success=True, message=f"Uninstalled: {plugin_name}")
            except PluginNotFoundError:
                continue

        self.print_error(f"Plugin '{plugin_name}' not found in any scope")
        return CommandResult(success=False, message="Plugin not found")

    def _uninstall_bundle(self, name: str, force: bool = False) -> CommandResult:
        """Uninstall a bundle.

        Args:
            name: Bundle name
            force: Skip confirmation prompt

        Returns:
            CommandResult with execution status
        """
        # Skip confirmation if --force or non-interactive (TUI)
        if not force and self._can_prompt_interactively():
            if not Confirm.ask(f"Uninstall bundle '{name}'?"):
                self.print_info("Cancelled")
                return CommandResult(success=False, message="Cancelled")

        try:
            self.plugin_manager.uninstall_bundle(name)
            self.print_success(f"Uninstalled: {name}")
            self.console.print()
            return CommandResult(success=True, message=f"Uninstalled: {name}")
        except BundleNotFoundError:
            self.print_error(f"Bundle '{name}' not found")
            return CommandResult(success=False, message="Bundle not found")
        except PluginManagerError as e:
            self.print_error(str(e))
            return CommandResult(success=False, message=str(e))

    def _sync_plugin(self, name: str) -> CommandResult:
        """Sync/update a plugin or bundle.

        Args:
            name: Plugin name, bundle name, or empty for all

        Returns:
            CommandResult with execution status
        """
        if not name:
            # Sync all bundles and marketplaces
            self.print_command_header("Syncing all plugins and bundles")

            # Sync bundles
            bundle_results = self.plugin_manager.sync_all_bundles()
            for bundle_name, error in bundle_results.items():
                if error:
                    self.print_error(f"Bundle {bundle_name}: {error}")
                else:
                    self.print_success(f"Bundle {bundle_name}: synced")

            # Sync marketplaces
            mp_results = self.plugin_manager.sync_all_marketplaces()
            for mp_name, error in mp_results.items():
                if error:
                    self.print_error(f"Marketplace {mp_name}: {error}")
                else:
                    self.print_success(f"Marketplace {mp_name}: synced")

            total = len(bundle_results) + len(mp_results)
            success_count = sum(1 for v in bundle_results.values() if v is None)
            success_count += sum(1 for v in mp_results.values() if v is None)

            self.console.print()
            self.print_info(f"Synced {success_count}/{total}")
            self.console.print()

            return CommandResult(
                success=True,
                message=f"Synced {success_count}/{total}",
            )

        # Try to sync as bundle first
        try:
            self.plugin_manager.sync_bundle(name)
            self.print_success(f"Synced bundle: {name}")
            self.console.print()
            return CommandResult(success=True, message=f"Synced: {name}")
        except BundleNotFoundError:
            pass

        # Try to sync as marketplace
        try:
            self.plugin_manager.sync_marketplace(name)
            self.print_success(f"Synced marketplace: {name}")
            self.console.print()
            return CommandResult(success=True, message=f"Synced: {name}")
        except MarketplaceNotFoundError:
            pass

        self.print_error(f"'{name}' not found as bundle or marketplace")
        return CommandResult(success=False, message="Not found")

    def _update_plugin(self, spec: str) -> CommandResult:
        """Update a plugin.

        Args:
            spec: Plugin spec in format <plugin>@<marketplace>

        Returns:
            CommandResult with execution status
        """
        if not spec or "@" not in spec:
            self.print_error("Plugin spec required: /plugins update <plugin>@<marketplace>")
            return CommandResult(success=False, message="Invalid spec")

        plugin_name, marketplace = spec.rsplit("@", 1)

        self.print_command_header("Updating plugin", plugin_name)

        # Try both scopes
        for scope in ["project", "user"]:
            try:
                installed = self.plugin_manager.update_plugin(plugin_name, marketplace, scope=scope)
                self.print_success(f"Updated: [cyan]{installed.name}[/cyan] v{installed.version}")
                self.console.print()
                return CommandResult(
                    success=True, message=f"Updated: {plugin_name}", data=installed
                )
            except PluginNotFoundError:
                continue
            except PluginManagerError as e:
                self.print_error(str(e))
                return CommandResult(success=False, message=str(e))

        self.print_error(f"Plugin '{plugin_name}' not found in any scope")
        return CommandResult(success=False, message="Plugin not found")

    def _list_plugins(self) -> CommandResult:
        """List installed plugins and bundles.

        Returns:
            CommandResult with execution status
        """
        plugins = self.plugin_manager.list_installed()
        bundles = self.plugin_manager.list_bundles()

        if not plugins and not bundles:
            self.print_info("No plugins or bundles installed.")
            self.print_continuation("Install with: /plugins install <url>")
            self.console.print()
            return CommandResult(success=True)

        # Show bundles first (URL installs)
        if bundles:
            table = Table(show_header=True, header_style="bold")
            table.add_column("Name", style="cyan")
            table.add_column("URL")
            table.add_column("Scope", style="dim")
            table.add_column("Status")

            for bundle in bundles:
                status = "[green]enabled[/green]" if bundle.enabled else "[dim]disabled[/dim]"
                # Truncate URL for display
                url = bundle.url
                if len(url) > 40:
                    url = url[:37] + "..."
                table.add_row(
                    bundle.name,
                    url,
                    bundle.scope,
                    status,
                )

            self.console.print(table)
            self.console.print()

        # Show marketplace plugins
        if plugins:
            self.print_line("[bold]Marketplace Plugins:[/bold]")
            table = Table(show_header=True, header_style="bold")
            table.add_column("Plugin", style="cyan")
            table.add_column("Version")
            table.add_column("Marketplace")
            table.add_column("Scope", style="dim")
            table.add_column("Status")

            for plugin in plugins:
                status = "[green]enabled[/green]" if plugin.enabled else "[dim]disabled[/dim]"
                table.add_row(
                    plugin.name,
                    plugin.version,
                    plugin.marketplace,
                    plugin.scope,
                    status,
                )

            self.console.print(table)
            self.console.print()

        return CommandResult(success=True, data={"plugins": plugins, "bundles": bundles})

    def _enable_plugin(self, spec: str) -> CommandResult:
        """Enable a plugin or bundle.

        Args:
            spec: Plugin spec (<plugin>@<marketplace>) or bundle name

        Returns:
            CommandResult with execution status
        """
        if not spec:
            self.print_error(
                "Usage: /plugins enable <name> or /plugins enable <plugin>@<marketplace>"
            )
            return CommandResult(success=False, message="Name required")

        # Check if it's a bundle (no @ in spec)
        if "@" not in spec:
            try:
                self.plugin_manager.enable_bundle(spec)
                self.print_success(f"Enabled bundle: {spec}")
                self.console.print()
                return CommandResult(success=True, message=f"Enabled: {spec}")
            except BundleNotFoundError:
                self.print_error(f"Bundle '{spec}' not found")
                return CommandResult(success=False, message="Bundle not found")

        # Traditional marketplace plugin
        plugin_name, marketplace = spec.rsplit("@", 1)

        for scope in ["project", "user"]:
            try:
                self.plugin_manager.enable_plugin(plugin_name, marketplace, scope=scope)
                self.print_success(f"Enabled: {plugin_name}")
                self.console.print()
                return CommandResult(success=True, message=f"Enabled: {plugin_name}")
            except PluginNotFoundError:
                continue

        self.print_error(f"Plugin '{plugin_name}' not found in any scope")
        return CommandResult(success=False, message="Plugin not found")

    def _disable_plugin(self, spec: str) -> CommandResult:
        """Disable a plugin or bundle.

        Args:
            spec: Plugin spec (<plugin>@<marketplace>) or bundle name

        Returns:
            CommandResult with execution status
        """
        if not spec:
            self.print_error(
                "Usage: /plugins disable <name> or /plugins disable <plugin>@<marketplace>"
            )
            return CommandResult(success=False, message="Name required")

        # Check if it's a bundle (no @ in spec)
        if "@" not in spec:
            try:
                self.plugin_manager.disable_bundle(spec)
                self.print_success(f"Disabled bundle: {spec}")
                self.console.print()
                return CommandResult(success=True, message=f"Disabled: {spec}")
            except BundleNotFoundError:
                self.print_error(f"Bundle '{spec}' not found")
                return CommandResult(success=False, message="Bundle not found")

        # Traditional marketplace plugin
        plugin_name, marketplace = spec.rsplit("@", 1)

        for scope in ["project", "user"]:
            try:
                self.plugin_manager.disable_plugin(plugin_name, marketplace, scope=scope)
                self.print_success(f"Disabled: {plugin_name}")
                self.console.print()
                return CommandResult(success=True, message=f"Disabled: {plugin_name}")
            except PluginNotFoundError:
                continue

        self.print_error(f"Plugin '{plugin_name}' not found in any scope")
        return CommandResult(success=False, message="Plugin not found")
