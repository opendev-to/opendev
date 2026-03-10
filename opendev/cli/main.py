"""Main entry point and argument parsing for OpenDev CLI."""

import argparse
from pathlib import Path

from rich.console import Console

from opendev.ui_textual.style_tokens import ERROR, WARNING
from opendev.setup import run_setup_wizard
from opendev.setup.wizard import config_exists
from opendev.ui_textual.runner import launch_textual_cli


def _pick_session_interactively(working_dir: "Path") -> "str | None":
    """Show an interactive numbered session list and let the user pick one.

    Returns the selected session ID, or None if cancelled / no sessions.
    """
    import os
    from datetime import datetime

    from rich.console import Console
    from rich.table import Table

    from opendev.core.context_engineering.history import SessionManager

    console = Console()

    env_session_dir = os.environ.get("OPENDEV_SESSION_DIR")
    if env_session_dir:
        sm = SessionManager(session_dir=Path(env_session_dir))
    else:
        sm = SessionManager(working_dir=working_dir)

    sessions = sm.list_all_sessions()
    if not sessions:
        console.print("[yellow]No sessions found.[/yellow]")
        return None

    def _relative_time(dt: datetime) -> str:
        delta = datetime.now() - dt
        seconds = int(delta.total_seconds())
        if seconds < 60:
            return "just now"
        minutes = seconds // 60
        if minutes < 60:
            return f"{minutes}m ago"
        hours = minutes // 60
        if hours < 24:
            return f"{hours}h ago"
        days = hours // 24
        if days < 30:
            return f"{days}d ago"
        return dt.strftime("%Y-%m-%d")

    table = Table(title="Sessions", show_lines=False)
    table.add_column("#", style="bold cyan", width=4)
    table.add_column("Title", style="white", max_width=40)
    table.add_column("ID", style="dim")
    table.add_column("Updated", style="green")
    table.add_column("Msgs", style="yellow", justify="right")

    for idx, s in enumerate(sessions, 1):
        table.add_row(
            str(idx),
            s.title or "Untitled",
            s.id,
            _relative_time(s.updated_at),
            str(s.message_count),
        )

    console.print(table)

    while True:
        try:
            choice = input("\nSelect session number (or 'q' to cancel): ").strip()
        except (KeyboardInterrupt, EOFError):
            console.print()
            return None

        if choice.lower() == "q":
            return None

        try:
            num = int(choice)
        except ValueError:
            console.print("[red]Please enter a number or 'q'.[/red]")
            continue

        if 1 <= num <= len(sessions):
            return sessions[num - 1].id

        console.print(f"[red]Please enter a number between 1 and {len(sessions)}.[/red]")


def main() -> None:
    """Main entry point for OpenDev CLI."""
    import sys
    from dotenv import load_dotenv

    load_dotenv()

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
        nargs="?",
        const=True,
        default=None,
        metavar="SESSION_ID",
        help="Resume a session (optionally specify ID, or pick interactively)",
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
        if arg in ("-p", "--prompt", "-d", "--working-dir"):
            i += 2
        elif arg in ("-r", "--resume"):
            if i + 1 < len(argv) and not argv[i + 1].startswith("-"):
                i += 2
            else:
                i += 1
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
            if arg in ("-p", "--prompt", "-d", "--working-dir"):
                flags.extend([arg, argv[i + 1]])
                i += 2
            elif arg in ("-r", "--resume"):
                if i + 1 < len(argv) and not argv[i + 1].startswith("-"):
                    flags.extend([arg, argv[i + 1]])
                    i += 2
                else:
                    flags.append(arg)
                    i += 1
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
        from opendev.cli.config_commands import _handle_config_command

        _handle_config_command(args)
        return

    # Handle MCP commands
    if args.command == "mcp":
        from opendev.cli.mcp_commands import _handle_mcp_command

        _handle_mcp_command(args)
        return

    # Handle run commands
    if args.command == "run":
        from opendev.cli.run_commands import _handle_run_command

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
        from opendev.core.runtime import ConfigManager
        from opendev.core.context_engineering.history import SessionManager

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
                from opendev.core.debug import SessionDebugLogger, set_debug_logger

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

            from opendev.cli.non_interactive import _run_non_interactive

            _run_non_interactive(
                config_manager,
                session_manager,
                args.prompt,
                dangerously_skip_permissions=args.dangerously_skip_permissions,
            )

            # Clean up debug logger
            if config.verbose:
                from opendev.core.debug import get_debug_logger, set_debug_logger as _set_dl

                get_debug_logger().log("session_end", "runner")
                _set_dl(None)

            # Don't persist non-interactive sessions — they clutter the session picker
            session = session_manager.get_current_session()
            if session:
                session_manager.delete_session(session.id)

            return

        resume_session_id = None
        if args.resume is True:
            # --resume with no ID → interactive picker
            resume_session_id = _pick_session_interactively(working_dir)
            if resume_session_id is None:
                return
        elif args.resume:
            # --resume <ID> → validate the session exists
            sessions = session_manager.list_all_sessions()
            if not any(s.id == args.resume for s in sessions):
                console.print(f"[yellow]Session '{args.resume}' not found.[/yellow]")
                resume_session_id = _pick_session_interactively(working_dir)
                if resume_session_id is None:
                    return
            else:
                resume_session_id = args.resume

        launch_textual_cli(
            message=bare_prompt,
            working_dir=working_dir,
            continue_session=args.continue_session,
            resume_session_id=resume_session_id,
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


if __name__ == "__main__":
    main()
