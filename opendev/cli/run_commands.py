"""Run subcommand handlers for OpenDev CLI."""

import sys
import time
import webbrowser
from pathlib import Path

from rich.console import Console

from opendev.ui_textual.style_tokens import CYAN, ERROR, SUCCESS, WARNING


def _handle_run_command(args) -> None:
    """Handle run subcommands.

    Args:
        args: Parsed command-line arguments
    """
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
                from opendev.core.runtime import ConfigManager, ModeManager
                from opendev.core.context_engineering.history import SessionManager, UndoManager
                from opendev.core.runtime.approval import ApprovalManager
                from opendev.core.context_engineering.mcp.manager import MCPManager

                working_dir = Path.cwd()
                config_manager = ConfigManager(working_dir)
                config = config_manager.load_config()
                session_manager = SessionManager(working_dir=working_dir)
                mode_manager = ModeManager()
                approval_manager = ApprovalManager(console)
                undo_manager = UndoManager(config.max_undo_history)
                mcp_manager = MCPManager(working_dir)

                from opendev.core.paths import get_paths
                from opendev.core.auth.user_store import UserStore

                paths = get_paths(working_dir)
                user_store = UserStore(paths.global_dir)

                # Get port and host from args
                preferred_port = getattr(args, "ui_port", 8080)
                backend_host = getattr(args, "ui_host", "127.0.0.1")

                # Find an available port
                from opendev.web.port_utils import find_available_port

                backend_port = find_available_port(backend_host, preferred_port, max_attempts=10)

                if backend_port is None:
                    console.print(
                        f"[{ERROR}]Error: Could not find available port starting from {preferred_port}[/{ERROR}]"
                    )
                    sys.exit(1)

                # Check for static files
                from opendev.web import find_static_directory

                static_dir = find_static_directory()

                if not static_dir or not static_dir.exists():
                    console.print(f"[{ERROR}]Error: Built web UI static files not found[/{ERROR}]")
                    sys.exit(1)

                try:
                    from opendev.web import start_server

                    web_server_thread = start_server(
                        config_manager=config_manager,
                        session_manager=session_manager,
                        mode_manager=mode_manager,
                        approval_manager=approval_manager,
                        undo_manager=undo_manager,
                        user_store=user_store,
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
