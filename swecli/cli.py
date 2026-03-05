"""Command-line interface entry point for OpenDev."""

import argparse
import sys
from pathlib import Path

from rich.console import Console

from swecli.ui_textual.style_tokens import CYAN, ERROR, SUCCESS, WARNING

from swecli.core.runtime.approval import ApprovalManager
from swecli.core.runtime import ConfigManager, ModeManager
from swecli.core.context_engineering.history import SessionManager, UndoManager
from swecli.core.runtime.services import RuntimeService
from swecli.models.agent_deps import AgentDependencies
from swecli.models.message import ChatMessage, Role
from swecli.ui_textual.runner import launch_textual_cli
from swecli.setup import run_setup_wizard
from swecli.setup.wizard import config_exists
from swecli.core.context_engineering.tools.implementations import (
    BashTool,
    EditTool,
    FileOperations,
    VLMTool,
    WebFetchTool,
    WebScreenshotTool,
    WriteTool,
)
from swecli.core.context_engineering.tools.implementations.web_search_tool import WebSearchTool
from swecli.core.context_engineering.tools.implementations.notebook_edit_tool import (
    NotebookEditTool,
)
from swecli.core.context_engineering.tools.implementations.ask_user_tool import AskUserTool


def main() -> None:
    """Main entry point for OpenDev CLI."""
    import sys

    parser = argparse.ArgumentParser(
        prog="swecli",
        description="OpenDev - AI-powered command-line tool for accelerated development",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  swecli                          # Start interactive CLI session
  swecli "do something"           # Start session with initial message
  swecli run ui                   # Start web UI (backend + frontend) and open browser
  swecli -p "create hello.py"     # Non-interactive mode
  swecli mcp list                 # List MCP servers
  swecli mcp add myserver uvx mcp-server-example
        """,
    )

    parser.add_argument(
        "--version",
        "-V",
        action="version",
        version="OpenDev 0.1.7",
    )

    parser.add_argument(
        "--working-dir",
        "-d",
        metavar="PATH",
        help="Set working directory (defaults to current directory)",
    )

    parser.add_argument(
        "--prompt",
        "-p",
        metavar="TEXT",
        help="Execute a single prompt and exit (non-interactive mode)",
    )

    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Enable verbose output with detailed logging",
    )

    parser.add_argument(
        "--continue",
        "-c",
        dest="continue_session",
        action="store_true",
        help="Resume the most recent session for the current working directory",
    )

    parser.add_argument(
        "--resume",
        "-r",
        metavar="SESSION_ID",
        help="Resume a specific session by its ID",
    )

    parser.add_argument(
        "--dangerously-skip-permissions",
        action="store_true",
        default=False,
        help="Skip all permission prompts and auto-approve every operation (use with caution)",
    )

    # Add subparsers for commands
    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # Config subcommand
    config_parser = subparsers.add_parser(
        "config",
        help="Manage OpenDev configuration",
        description="Configure AI providers, models, and other settings",
    )
    config_subparsers = config_parser.add_subparsers(
        dest="config_command", help="Config operations"
    )

    # config setup
    config_subparsers.add_parser("setup", help="Run the interactive setup wizard")

    # config show
    config_subparsers.add_parser("show", help="Display current configuration")

    # MCP subcommand
    mcp_parser = subparsers.add_parser(
        "mcp",
        help="Configure and manage MCP (Model Context Protocol) servers",
        description="Manage MCP servers for extending OpenDev with external tools and capabilities",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  swecli mcp list                                    # List all servers
  swecli mcp add myserver uvx mcp-server-sqlite      # Add SQLite MCP server
  swecli mcp add custom node server.js arg1 arg2     # Add custom server with args
  swecli mcp add api python api.py --env API_KEY=xyz # Add with environment variable
  swecli mcp get myserver                            # Show server details
  swecli mcp enable myserver                         # Enable a server
  swecli mcp remove myserver                         # Remove a server
        """,
    )
    mcp_subparsers = mcp_parser.add_subparsers(dest="mcp_command", help="MCP operations")

    # mcp list
    mcp_subparsers.add_parser("list", help="List all configured MCP servers with their status")

    # mcp get
    mcp_get = mcp_subparsers.add_parser(
        "get", help="Show detailed information about a specific MCP server"
    )
    mcp_get.add_argument("name", help="Name of the MCP server")

    # mcp add
    mcp_add = mcp_subparsers.add_parser(
        "add",
        help="Add a new MCP server to the configuration",
        description="Register a new MCP server that will be available to OpenDev",
    )
    mcp_add.add_argument("name", help="Unique name for the server")
    mcp_add.add_argument(
        "command", help="Command to start the MCP server (e.g., 'uvx', 'node', 'python')"
    )
    mcp_add.add_argument("args", nargs="*", help="Arguments to pass to the command")
    mcp_add.add_argument(
        "--env",
        nargs="*",
        metavar="KEY=VALUE",
        help="Environment variables for the server (e.g., API_KEY=xyz TOKEN=abc)",
    )
    mcp_add.add_argument(
        "--no-auto-start",
        action="store_true",
        help="Don't automatically start this server when OpenDev launches",
    )

    # mcp remove
    mcp_remove = mcp_subparsers.add_parser(
        "remove", help="Remove an MCP server from the configuration"
    )
    mcp_remove.add_argument("name", help="Name of the server to remove")

    # mcp enable
    mcp_enable = mcp_subparsers.add_parser(
        "enable", help="Enable an MCP server (will auto-start if configured)"
    )
    mcp_enable.add_argument("name", help="Name of the server to enable")

    # mcp disable
    mcp_disable = mcp_subparsers.add_parser(
        "disable", help="Disable an MCP server (won't auto-start)"
    )
    mcp_disable.add_argument("name", help="Name of the server to disable")

    # Run subcommand
    run_parser = subparsers.add_parser(
        "run", help="Run development tools", description="Run development servers and tools"
    )
    run_subparsers = run_parser.add_subparsers(dest="run_command", help="Run operations")

    # run ui
    run_ui_parser = run_subparsers.add_parser(
        "ui", help="Start the web UI (backend + frontend) and open in browser"
    )
    run_ui_parser.add_argument(
        "--ui-port",
        type=int,
        default=8080,
        metavar="PORT",
        help="Port for backend API server (default: 8080)",
    )
    run_ui_parser.add_argument(
        "--ui-host",
        default="127.0.0.1",
        metavar="HOST",
        help="Host for backend API server (default: 127.0.0.1)",
    )

    # Support bare positional prompt: swecli "hello" opens interactive TUI with initial message
    known_subcommands = {"config", "mcp", "run"}
    argv = sys.argv[1:]
    bare_prompt = None

    # Check if -p/--prompt is already specified
    has_prompt_flag = any(
        a in ("-p", "--prompt") or a.startswith("--prompt=") for a in argv
    )

    # Find first positional (non-flag) argument
    first_positional = None
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg in ("-p", "--prompt", "-d", "--working-dir", "-r", "--resume"):
            i += 2
        elif arg.startswith("-"):
            i += 1
        else:
            first_positional = arg
            break

    if (
        first_positional is not None
        and first_positional not in known_subcommands
        and not has_prompt_flag
    ):
        # Separate flags from positional args, store positionals as bare_prompt
        flags: list[str] = []
        prompt_parts: list[str] = []
        i = 0
        while i < len(argv):
            arg = argv[i]
            if arg in ("-p", "--prompt", "-d", "--working-dir", "-r", "--resume"):
                flags.extend([arg, argv[i + 1]])
                i += 2
            elif arg.startswith("-"):
                flags.append(arg)
                i += 1
            else:
                prompt_parts.append(arg)
                i += 1
        bare_prompt = " ".join(prompt_parts)
        argv = flags

    args = parser.parse_args(argv)

    # Handle config commands
    if args.command == "config":
        _handle_config_command(args)
        return

    # Handle MCP commands
    if args.command == "mcp":
        _handle_mcp_command(args)
        return

    # Handle run commands
    if args.command == "run":
        _handle_run_command(args)
        return

    console = Console()

    # Run setup wizard if config doesn't exist
    if not config_exists():
        if not run_setup_wizard():
            console.print(f"[{WARNING}]Setup cancelled. Exiting.[/{WARNING}]")
            sys.exit(0)

    # Clear terminal before starting interactive session
    # This prevents shell prompt from bleeding into the TUI
    sys.stdout.write("\033[3J")  # Clear scrollback buffer
    sys.stdout.write("\033[2J")  # Clear screen
    sys.stdout.write("\033[H")  # Move cursor to home
    sys.stdout.flush()

    # Set working directory
    working_dir = Path(args.working_dir) if args.working_dir else Path.cwd()
    if not working_dir.exists():
        console.print(f"[{ERROR}]Error: Working directory does not exist: {working_dir}[/{ERROR}]")
        sys.exit(1)

    try:
        # Initialize managers
        config_manager = ConfigManager(working_dir)
        config = config_manager.load_config()

        # Override verbose if specified
        if args.verbose:
            config.verbose = True

        # Ensure directories exist
        config_manager.ensure_directories()

        # Initialize session manager (project-scoped)
        import os
        env_session_dir = os.environ.get("OPENDEV_SESSION_DIR")
        if env_session_dir:
            session_manager = SessionManager(session_dir=Path(env_session_dir))
        else:
            session_manager = SessionManager(working_dir=working_dir)

        # Non-interactive mode
        if args.prompt:
            session_manager.create_session(working_directory=str(working_dir))

            # Initialize debug logger for non-interactive mode
            if config.verbose:
                from swecli.core.debug import SessionDebugLogger, set_debug_logger

                session = session_manager.get_current_session()
                if session:
                    dbg_logger = SessionDebugLogger(session_manager.session_dir, session.id)
                    set_debug_logger(dbg_logger)
                    dbg_logger.log(
                        "session_start",
                        "runner",
                        session_id=session.id,
                        working_dir=str(working_dir),
                        model=config.model,
                        mode="non_interactive",
                    )

            _run_non_interactive(
                config_manager,
                session_manager,
                args.prompt,
                dangerously_skip_permissions=args.dangerously_skip_permissions,
            )

            # Clean up debug logger
            if config.verbose:
                from swecli.core.debug import get_debug_logger, set_debug_logger as _set_dl

                get_debug_logger().log("session_end", "runner")
                _set_dl(None)

            return

        launch_textual_cli(
            message=bare_prompt,
            working_dir=working_dir,
            continue_session=args.continue_session,
            resume_session_id=args.resume,
            dangerously_skip_permissions=args.dangerously_skip_permissions,
        )
        return

    except KeyboardInterrupt:
        console.print(f"\n[{WARNING}]Interrupted.[/{WARNING}]")
        sys.exit(130)
    except Exception as e:
        console.print(f"[{ERROR}]Error: {str(e)}[/{ERROR}]")
        if args.verbose:
            import traceback

            console.print(traceback.format_exc())
        sys.exit(1)


def _handle_config_command(args) -> None:
    """Handle config subcommands.

    Args:
        args: Parsed command-line arguments
    """
    import json

    console = Console()

    if not args.config_command:
        console.print(
            f"[{WARNING}]No config subcommand specified. Use --help for available commands.[/{WARNING}]"
        )
        sys.exit(1)

    if args.config_command == "setup":
        # Run setup wizard (can be used to reconfigure)
        if not run_setup_wizard():
            console.print(f"[{WARNING}]Setup cancelled.[/{WARNING}]")
            sys.exit(0)

    elif args.config_command == "show":
        # Display current configuration
        from swecli.core.paths import get_paths

        config_file = get_paths().global_settings

        if not config_file.exists():
            console.print(
                f"[{WARNING}]No configuration found. Run 'swecli config setup' first.[/{WARNING}]"
            )
            sys.exit(1)

        try:
            with open(config_file, "r") as f:
                config = json.load(f)

            from rich.table import Table

            table = Table(title="Current Configuration", show_header=True, header_style="bold cyan")
            table.add_column("Setting", style="cyan")
            table.add_column("Value", style="white")

            # Display non-sensitive config values
            for key, value in config.items():
                if key == "api_key":
                    # Mask API key
                    if value:
                        masked = (
                            value[:8] + "*" * (len(value) - 12) + value[-4:]
                            if len(value) > 12
                            else "*" * len(value)
                        )
                        table.add_row(key, masked)
                    else:
                        table.add_row(key, "[dim]Not set[/dim]")
                else:
                    table.add_row(key, str(value))

            console.print()
            console.print(table)
            console.print()
            console.print(f"[dim]Config file: {config_file}[/dim]")

        except json.JSONDecodeError:
            console.print(
                f"[{ERROR}]Error: Invalid JSON in configuration file: {config_file}[/{ERROR}]"
            )
            sys.exit(1)
        except Exception as e:
            console.print(f"[{ERROR}]Error reading configuration: {e}[/{ERROR}]")
            sys.exit(1)


def _handle_mcp_command(args) -> None:
    """Handle MCP subcommands.

    Args:
        args: Parsed command-line arguments
    """
    from swecli.core.context_engineering.mcp.manager import MCPManager
    from rich.table import Table

    console = Console()
    mcp_manager = MCPManager()

    if not args.mcp_command:
        console.print(
            f"[{WARNING}]No MCP subcommand specified. Use --help for available commands.[/{WARNING}]"
        )
        sys.exit(1)

    try:
        if args.mcp_command == "list":
            servers = mcp_manager.list_servers()

            if not servers:
                console.print(f"[{WARNING}]No MCP servers configured[/{WARNING}]")
                return

            table = Table(title="MCP Servers", show_header=True, header_style="bold cyan")
            table.add_column("Name", style="cyan")
            table.add_column("Command")
            table.add_column("Enabled", justify="center")
            table.add_column("Auto-start", justify="center")

            for name, config in servers.items():
                enabled = f"[{SUCCESS}]✓[/{SUCCESS}]" if config.enabled else f"[{ERROR}]✗[/{ERROR}]"
                auto_start = f"[{SUCCESS}]✓[/{SUCCESS}]" if config.auto_start else "[dim]-[/dim]"
                command = (
                    f"{config.command} {' '.join(config.args[:2])}"
                    if config.args
                    else config.command
                )
                if len(command) > 60:
                    command = command[:57] + "..."

                table.add_row(name, command, enabled, auto_start)

            console.print(table)

        elif args.mcp_command == "get":
            servers = mcp_manager.list_servers()
            if args.name not in servers:
                console.print(f"[{ERROR}]Error: Server '{args.name}' not found[/{ERROR}]")
                sys.exit(1)

            config = servers[args.name]
            console.print(f"\n[bold {CYAN}]{args.name}[/bold {CYAN}]\n")
            console.print(f"Command: {config.command}")
            if config.args:
                console.print(f"Args: {' '.join(config.args)}")
            if config.env:
                console.print("Environment variables:")
                for key, value in config.env.items():
                    console.print(f"  {key}={value}")
            console.print(f"Enabled: {'Yes' if config.enabled else 'No'}")
            console.print(f"Auto-start: {'Yes' if config.auto_start else 'No'}")
            console.print(f"Transport: {config.transport}")

        elif args.mcp_command == "add":
            # Parse environment variables
            env = {}
            if args.env:
                for env_var in args.env:
                    if "=" not in env_var:
                        console.print(
                            f"[{ERROR}]Error: Invalid environment variable format: {env_var}[/{ERROR}]"
                        )
                        console.print("Use KEY=VALUE format")
                        sys.exit(1)
                    key, value = env_var.split("=", 1)
                    env[key] = value

            mcp_manager.add_server(
                name=args.name, command=args.command, args=args.args or [], env=env
            )

            # Update auto_start if specified
            if args.no_auto_start:
                config = mcp_manager.get_config()
                config.mcp_servers[args.name].auto_start = False
                from swecli.core.context_engineering.mcp.config import save_config

                save_config(config)

            console.print(f"[{SUCCESS}]✓[/{SUCCESS}] Added MCP server '{args.name}'")

        elif args.mcp_command == "remove":
            success = mcp_manager.remove_server(args.name)
            if success:
                console.print(f"[{SUCCESS}]✓[/{SUCCESS}] Removed MCP server '{args.name}'")
            else:
                console.print(f"[{ERROR}]Error: Server '{args.name}' not found[/{ERROR}]")
                sys.exit(1)

        elif args.mcp_command == "enable":
            success = mcp_manager.enable_server(args.name)
            if success:
                console.print(f"[{SUCCESS}]✓[/{SUCCESS}] Enabled MCP server '{args.name}'")
            else:
                console.print(f"[{ERROR}]Error: Server '{args.name}' not found[/{ERROR}]")
                sys.exit(1)

        elif args.mcp_command == "disable":
            success = mcp_manager.disable_server(args.name)
            if success:
                console.print(f"[{SUCCESS}]✓[/{SUCCESS}] Disabled MCP server '{args.name}'")
            else:
                console.print(f"[{ERROR}]Error: Server '{args.name}' not found[/{ERROR}]")
                sys.exit(1)

    except Exception as e:
        console.print(f"[{ERROR}]Error: {str(e)}[/{ERROR}]")
        sys.exit(1)


def _handle_run_command(args) -> None:
    """Handle run subcommands.

    Args:
        args: Parsed command-line arguments
    """
    import webbrowser
    import time
    from pathlib import Path

    console = Console()

    if not args.run_command:
        console.print(
            f"[{WARNING}]No run subcommand specified. Use --help for available commands.[/{WARNING}]"
        )
        sys.exit(1)

    if args.run_command == "ui":
        try:
            # Show spinner while starting up
            with console.status(f"[{CYAN}]Starting Web UI…[/{CYAN}]", spinner="dots"):
                # Initialize managers for backend
                from swecli.core.runtime import ConfigManager, ModeManager
                from swecli.core.context_engineering.history import SessionManager, UndoManager
                from swecli.core.runtime.approval import ApprovalManager
                from swecli.core.context_engineering.mcp.manager import MCPManager
                import webbrowser

                working_dir = Path.cwd()
                config_manager = ConfigManager(working_dir)
                config = config_manager.load_config()
                session_manager = SessionManager(working_dir=working_dir)
                mode_manager = ModeManager()
                approval_manager = ApprovalManager(console)
                undo_manager = UndoManager(config.max_undo_history)
                mcp_manager = MCPManager(working_dir)

                # Get port and host from args
                preferred_port = getattr(args, "ui_port", 8080)
                backend_host = getattr(args, "ui_host", "127.0.0.1")

                # Find an available port
                from swecli.web.port_utils import find_available_port

                backend_port = find_available_port(backend_host, preferred_port, max_attempts=10)

                if backend_port is None:
                    console.print(
                        f"[{ERROR}]Error: Could not find available port starting from {preferred_port}[/{ERROR}]"
                    )
                    sys.exit(1)

                # Check for static files
                from swecli.web import find_static_directory

                static_dir = find_static_directory()

                if not static_dir or not static_dir.exists():
                    console.print(f"[{ERROR}]Error: Built web UI static files not found[/{ERROR}]")
                    sys.exit(1)

                try:
                    from swecli.web import start_server

                    web_server_thread = start_server(
                        config_manager=config_manager,
                        session_manager=session_manager,
                        mode_manager=mode_manager,
                        approval_manager=approval_manager,
                        undo_manager=undo_manager,
                        mcp_manager=mcp_manager,
                        host=backend_host,
                        port=backend_port,
                        open_browser=False,
                    )

                    # Wait for backend to be ready
                    time.sleep(1.5)

                    # Verify server is running
                    if not web_server_thread.is_alive():
                        console.print(
                            f"[{ERROR}]Error: Backend server thread terminated unexpectedly[/{ERROR}]"
                        )
                        sys.exit(1)

                except ImportError:
                    console.print(f"[{ERROR}]Error: Web dependencies not installed[/{ERROR}]")
                    console.print(
                        f"[{WARNING}]Install with: pip install 'swe-cli[web]'[/{WARNING}]"
                    )
                    sys.exit(1)
                except Exception as e:
                    console.print(f"[{ERROR}]Error starting backend server: {str(e)}[/{ERROR}]")
                    sys.exit(1)

                url = f"http://{backend_host}:{backend_port}"

                # Open browser in background
                import threading

                def open_browser():
                    webbrowser.open(url)

                threading.Thread(target=open_browser, daemon=True).start()

            # Simple success message
            console.print(f"[{SUCCESS}]✓ Web UI available at [{CYAN}]{url}[/{CYAN}][/{SUCCESS}]\n")

            # Keep the main thread alive and serve until interrupted
            try:
                while True:
                    time.sleep(1)
            except KeyboardInterrupt:
                console.print(f"\n[{WARNING}]Stopping Web UI…[/{WARNING}]")

        except KeyboardInterrupt:
            console.print(f"\n[{WARNING}]Startup cancelled.[/{WARNING}]")
        except Exception as e:
            console.print(f"[{ERROR}]Error: {str(e)}[/{ERROR}]")
            sys.exit(1)


def _run_non_interactive(
    config_manager: ConfigManager,
    session_manager: SessionManager,
    prompt: str,
    dangerously_skip_permissions: bool = False,
) -> None:
    """Run a single prompt in non-interactive mode.

    Args:
        config_manager: Configuration manager
        session_manager: Session manager
        prompt: User prompt to execute
        dangerously_skip_permissions: If True, auto-approve all operations
    """
    console = Console()

    if dangerously_skip_permissions:
        console.print(
            f"[{WARNING}]Warning: --dangerously-skip-permissions is enabled. "
            f"All operations will be auto-approved without confirmation.[/{WARNING}]"
        )

    config = config_manager.get_config()
    mode_manager = ModeManager()
    approval_manager = ApprovalManager(console)
    if dangerously_skip_permissions:
        approval_manager.auto_approve_remaining = True
    undo_manager = UndoManager(config.max_undo_history)

    # Initialize plan file path for plan mode
    session = session_manager.get_current_session()
    if session:
        from swecli.core.paths import get_paths

        plans_dir = get_paths().global_dir / "plans"
        plans_dir.mkdir(parents=True, exist_ok=True)
        plan_file_path = plans_dir / f"{session.id}.md"
        mode_manager.set_plan_file_path(str(plan_file_path))

    file_ops = FileOperations(config, config_manager.working_dir)
    write_tool = WriteTool(config, config_manager.working_dir)
    edit_tool = EditTool(config, config_manager.working_dir)
    bash_tool = BashTool(config, config_manager.working_dir)
    web_fetch_tool = WebFetchTool(config, config_manager.working_dir)
    web_search_tool = WebSearchTool(config, config_manager.working_dir)
    notebook_edit_tool = NotebookEditTool(config_manager.working_dir)
    ask_user_tool = AskUserTool()  # Uses console fallback in non-interactive mode
    vlm_tool = VLMTool(config, config_manager.working_dir)
    web_screenshot_tool = WebScreenshotTool(config, config_manager.working_dir)

    runtime_service = RuntimeService(config_manager, mode_manager)
    runtime_suite = runtime_service.build_suite(
        file_ops=file_ops,
        write_tool=write_tool,
        edit_tool=edit_tool,
        bash_tool=bash_tool,
        web_fetch_tool=web_fetch_tool,
        web_search_tool=web_search_tool,
        notebook_edit_tool=notebook_edit_tool,
        ask_user_tool=ask_user_tool,
        vlm_tool=vlm_tool,
        web_screenshot_tool=web_screenshot_tool,
        mcp_manager=None,
    )

    agent = runtime_suite.agents.normal

    session = session_manager.get_current_session()
    if not session:
        session = session_manager.create_session(working_directory=str(config_manager.working_dir))

    message_history = session.to_api_messages()

    deps = AgentDependencies(
        mode_manager=mode_manager,
        approval_manager=approval_manager,
        undo_manager=undo_manager,
        session_manager=session_manager,
        working_dir=config_manager.working_dir,
        console=console,
        config=config,
    )

    try:
        result = agent.run_sync(prompt, deps, message_history=message_history)
    except Exception as exc:  # noqa: BLE001
        console.print(f"[{ERROR}]Error: {exc}[/{ERROR}]")
        sys.exit(1)

    if not result.get("success", False):
        error = result.get("error", "Unknown error")
        console.print(f"[{ERROR}]Error: {error}[/{ERROR}]")
        sys.exit(1)

    user_msg = ChatMessage(role=Role.USER, content=prompt)
    session_manager.add_message(user_msg, config.auto_save_interval)

    assistant_content = result.get("content", "") or ""
    raw_assistant_content = assistant_content
    history = result.get("messages") or []
    for msg in reversed(history):
        if msg.get("role") == "assistant":
            raw_assistant_content = msg.get("content", raw_assistant_content)
            break

    # Extract metadata from agent response for session persistence
    thinking_trace = result.get("thinking_trace")
    reasoning_content = result.get("reasoning_content")
    token_usage = result.get("usage")

    metadata = {}
    if raw_assistant_content is not None:
        metadata["raw_content"] = raw_assistant_content

    assistant_msg = ChatMessage(
        role=Role.ASSISTANT,
        content=assistant_content,
        metadata=metadata,
        thinking_trace=thinking_trace,
        reasoning_content=reasoning_content,
        token_usage=token_usage,
    )
    session_manager.add_message(assistant_msg, config.auto_save_interval)
    session_manager.save_session()

    console.print(assistant_content)


if __name__ == "__main__":
    main()
