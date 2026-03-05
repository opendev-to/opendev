"""Interactive REPL for OpenDev."""

import logging
from typing import Optional

logger = logging.getLogger(__name__)

from prompt_toolkit import PromptSession
from prompt_toolkit.history import FileHistory
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.styles import Style
from rich.console import Console
from rich.panel import Panel

from swecli.core.runtime.approval import ApprovalManager
from swecli.core.runtime import (
    ConfigManager,
    ModeManager,
    OperationMode,
)
from swecli.core.context_engineering.history import SessionManager, UndoManager
from swecli.core.runtime.monitoring import ErrorHandler
from swecli.core.runtime.services import RuntimeService
from swecli.core.context_engineering.tools.implementations import (
    FileOperations,
    WriteTool,
    EditTool,
    BashTool,
)
from swecli.ui_textual.components.console_animations import Spinner
from swecli.ui_textual.components import StatusLine, NotificationCenter
from swecli.ui_textual.autocomplete import SwecliCompleter
from swecli.ui_textual.formatters_internal.output_formatter import OutputFormatter
from swecli.ui_textual.style_tokens import (
    CYAN,
    ERROR,
    GREY,
    PT_BG_BLACK,
    PT_BG_SELECTED,
    PT_GREEN,
    PT_GREY,
    PT_META_GREY,
    PT_ORANGE,
    WARNING,
)

# Command handlers
from swecli.repl.commands import (
    SessionCommands,
    ModeCommands,
    MCPCommands,
    HelpCommand,
    ConfigCommands,
    ToolCommands,
    AgentsCommands,
    SkillsCommands,
    PluginsCommands,
)

# UI components
from swecli.repl.ui import (
    MessagePrinter,
    InputFrame,
    PromptBuilder,
    Toolbar,
    ContextDisplay,
)

# Query processing
from swecli.repl.query_processor import QueryProcessor


class REPL:
    """Interactive REPL for AI-powered coding assistance."""

    def __init__(
        self,
        config_manager: ConfigManager,
        session_manager: SessionManager,
        *,
        is_tui: bool = False,
    ):
        """Initialize REPL.

        Args:
            config_manager: Configuration manager
            session_manager: Session manager
            is_tui: Whether running inside the TUI (disables interactive prompts)
        """
        self.config_manager = config_manager
        self.session_manager = session_manager
        self.is_tui = is_tui
        self.config = config_manager.get_config()
        self.console = Console()

        # Initialize tools and managers
        self._init_tools()
        self._init_managers()
        self._init_runtime_service()

        # Initialize UI
        self._init_ui_components()
        self._init_prompt_session()

        # Initialize command handlers
        self._init_command_handlers()

        # Initialize query processor
        self._init_query_processor()

        # Initialize hooks system
        self._init_hooks()

        self.running = True

    def _init_tools(self):
        """Initialize file operation and command tools."""
        from swecli.core.context_engineering.tools.implementations import (
            WebFetchTool,
            OpenBrowserTool,
            VLMTool,
            WebScreenshotTool,
        )
        from swecli.core.context_engineering.tools.implementations.web_search_tool import (
            WebSearchTool,
        )
        from swecli.core.context_engineering.tools.implementations.notebook_edit_tool import (
            NotebookEditTool,
        )
        from swecli.core.context_engineering.tools.implementations.ask_user_tool import AskUserTool
        from swecli.core.context_engineering.mcp.manager import MCPManager
        from swecli.core.context_engineering.tools.background_task_manager import (
            BackgroundTaskManager,
        )

        self.task_manager = BackgroundTaskManager(self.config_manager.working_dir)
        self.file_ops = FileOperations(self.config, self.config_manager.working_dir)
        self.write_tool = WriteTool(self.config, self.config_manager.working_dir)
        self.edit_tool = EditTool(self.config, self.config_manager.working_dir)
        self.bash_tool = BashTool(
            self.config, self.config_manager.working_dir, task_manager=self.task_manager
        )
        self.web_fetch_tool = WebFetchTool(self.config, self.config_manager.working_dir)
        self.web_search_tool = WebSearchTool(self.config, self.config_manager.working_dir)
        self.notebook_edit_tool = NotebookEditTool(self.config_manager.working_dir)
        self.ask_user_tool = AskUserTool()  # Uses console fallback
        self.open_browser_tool = OpenBrowserTool(self.config, self.config_manager.working_dir)
        self.vlm_tool = VLMTool(self.config, self.config_manager.working_dir)
        self.web_screenshot_tool = WebScreenshotTool(self.config, self.config_manager.working_dir)
        self.mcp_manager = MCPManager(working_dir=self.config_manager.working_dir)

    def _init_managers(self):
        """Initialize operation managers."""
        self.mode_manager = ModeManager()
        self.approval_manager = ApprovalManager(self.console)
        self.error_handler = ErrorHandler(self.console)
        self.undo_manager = UndoManager(self.config.max_undo_history)

    def _init_runtime_service(self):
        """Initialize runtime service with tool registry and agents."""
        self.runtime_service = RuntimeService(self.config_manager, self.mode_manager)
        self.runtime_suite = self.runtime_service.build_suite(
            file_ops=self.file_ops,
            write_tool=self.write_tool,
            edit_tool=self.edit_tool,
            bash_tool=self.bash_tool,
            web_fetch_tool=self.web_fetch_tool,
            web_search_tool=self.web_search_tool,
            notebook_edit_tool=self.notebook_edit_tool,
            ask_user_tool=self.ask_user_tool,
            open_browser_tool=self.open_browser_tool,
            vlm_tool=self.vlm_tool,
            web_screenshot_tool=self.web_screenshot_tool,
            mcp_manager=self.mcp_manager,
        )

        self.tool_registry = self.runtime_suite.tool_registry
        self.normal_agent = self.runtime_suite.agents.normal
        self.agent = self.normal_agent

        # Flag for plan mode request via Shift+Tab
        self._pending_plan_request = False

    def _init_ui_components(self):
        """Initialize UI components and state."""
        # UI Components
        self.spinner = Spinner(self.console)
        self.status_line = StatusLine(self.console)
        self.output_formatter = OutputFormatter(self.console, use_claude_style=True)
        self._notification_center = NotificationCenter(self.console)

        # UI state trackers
        self._last_latency_ms: Optional[int] = None
        self._context_sidebar_visible = False
        self._last_prompt: str = ""
        self._last_operation_summary: str = "—"
        self._last_error: Optional[str] = None
        self._key_bindings = self._build_key_bindings()

        # Message printer and input frame
        self.message_printer = MessagePrinter(self.console)
        self.input_frame = InputFrame(self.console)
        self.prompt_builder = PromptBuilder()
        self.toolbar = Toolbar(self.mode_manager, self.session_manager, self.config)
        self.context_display = ContextDisplay(
            self.console,
            self.mode_manager,
            self.session_manager,
            self.config_manager,
            self._notification_center,
        )

    def _init_prompt_session(self):
        """Initialize prompt session with history and autocomplete."""
        # Setup prompt session with history
        from swecli.core.paths import get_paths

        paths = get_paths(self.config_manager.working_dir)
        history_file = paths.global_history_file
        history_file.parent.mkdir(parents=True, exist_ok=True)

        # Create autocomplete for @ mentions and / commands
        self.completer = SwecliCompleter(working_dir=self.config_manager.working_dir)

        # Elegant autocomplete styling
        autocomplete_style = Style.from_dict(
            {
                "completion-menu": f"bg:{PT_BG_BLACK}",
                "completion-menu.completion": "#FFFFFF",
                "completion-menu.completion.current": f"bg:{PT_BG_SELECTED} #FFFFFF",
                "completion-menu.meta": PT_META_GREY,
                "completion-menu.completion.current.meta": "#A0A0A0",
                "mode-normal": f"bold {PT_ORANGE}",
                "mode-plan": f"bold {PT_GREEN}",
                "toolbar-text": PT_GREY,
            }
        )

        self.prompt_session: PromptSession[str] = PromptSession(
            history=FileHistory(str(history_file)),
            completer=self.completer,
            complete_while_typing=True,
            key_bindings=self._key_bindings,
            style=autocomplete_style,
            bottom_toolbar=self.toolbar.build_tokens,
        )

    def _init_command_handlers(self):
        """Initialize slash command handlers."""
        self.session_commands = SessionCommands(
            self.console,
            self.session_manager,
            self.config_manager,
        )

        self.mode_commands = ModeCommands(
            self.console,
            repl=self,
        )

        self.config_commands = ConfigCommands(
            self.console,
            self.config_manager,
            chat_app=None,  # Will be set by ReplChat
        )

        self.mcp_commands = MCPCommands(
            self.console,
            self.mcp_manager,
            refresh_runtime_callback=self._refresh_runtime_tooling,
            agent=self.agent,
        )

        self.tool_commands = ToolCommands(
            console=self.console,
            repl=self,
        )

        self.help_command = HelpCommand(
            self.console,
            self.mode_manager,
        )

        self.agents_commands = AgentsCommands(
            self.console,
            self.config_manager,
            subagent_manager=(
                self.runtime_suite.subagent_manager
                if hasattr(self.runtime_suite, "subagent_manager")
                else None
            ),
        )

        self.skills_commands = SkillsCommands(
            self.console,
            self.config_manager,
        )

        self.plugins_commands = PluginsCommands(
            self.console,
            self.config_manager,
            is_tui=self.is_tui,
        )

    def _init_query_processor(self):
        """Initialize query processor for AI interactions."""
        self.query_processor = QueryProcessor(
            self.console,
            self.session_manager,
            self.config,
            self.config_manager,
            self.mode_manager,
            self.file_ops,
            self.output_formatter,
            self.status_line,
            self._print_markdown_message,
        )
        self.query_processor.set_notification_center(self._notification_center)

    def _init_hooks(self):
        """Initialize hooks system from settings.json."""
        from swecli.core.hooks.loader import load_hooks_config
        from swecli.core.hooks.manager import HookManager

        try:
            config = load_hooks_config(self.config_manager.working_dir)
            session_id = (
                self.session_manager.current_session.id
                if self.session_manager.current_session
                else ""
            )
            self._hook_manager = HookManager(
                config=config,
                session_id=session_id,
                cwd=str(self.config_manager.working_dir),
            )
            # Wire into tool registry
            self.tool_registry.set_hook_manager(self._hook_manager)
            # Wire into query processor / react executor
            self.query_processor.set_hook_manager(self._hook_manager)
            # Wire into subagent manager
            if hasattr(self.runtime_suite, "subagent_manager") and self.runtime_suite.subagent_manager:
                self.runtime_suite.subagent_manager.set_hook_manager(self._hook_manager)
            # Wire into compactor (if available on react_executor)
            if hasattr(self.query_processor._react_executor, "_compactor"):
                compactor = self.query_processor._react_executor._compactor
                if compactor and hasattr(compactor, "set_hook_manager"):
                    compactor.set_hook_manager(self._hook_manager)
        except Exception as e:
            logger.warning("Failed to initialize hooks: %s", e)
            self._hook_manager = None

    def _refresh_runtime_tooling(self) -> None:
        """Refresh tool registry and agent metadata after MCP changes."""
        if hasattr(self.tool_registry, "set_mcp_manager"):
            self.tool_registry.set_mcp_manager(self.mcp_manager)
        self.runtime_suite.refresh_agents()

    def rebuild_agents(self) -> None:
        """Rebuild agents with updated configuration (e.g., after model/provider change).

        This is needed when the model provider changes, as the HTTP client
        needs to use the new provider's API key.
        """
        # Refresh config
        self.config = self.config_manager.get_config()

        # Rebuild agents with new config
        self.runtime_suite.rebuild_agents(
            self.config,
            self.mode_manager,
            self.config_manager.working_dir,
        )

        # Update agent references
        self.normal_agent = self.runtime_suite.agents.normal
        self.agent = self.normal_agent

    def _build_key_bindings(self) -> KeyBindings:
        """Configure prompt key bindings for high-speed workflows."""
        kb = KeyBindings()

        @kb.add("s-tab")
        def _(event) -> None:
            from swecli.core.runtime.mode_manager import OperationMode

            if self.mode_manager.current_mode == OperationMode.PLAN:
                # Switch back to normal mode
                self.mode_manager.set_mode(OperationMode.NORMAL)
                self._pending_plan_request = False
                self._notify("Switched to Normal mode.", level="info", toast=False)
            else:
                self._pending_plan_request = not self._pending_plan_request
                if self._pending_plan_request:
                    self._notify(
                        "Plan mode requested. Next query will trigger planning.",
                        level="info",
                        toast=False,
                    )
                else:
                    self._notify("Plan mode cancelled.", level="info", toast=False)

            if hasattr(self, "approval_manager"):
                self.approval_manager.reset_auto_approve()

            event.app.invalidate()

        return kb

    def _print_markdown_message(
        self,
        content: str,
        *,
        symbol: str = "⏺",
    ) -> None:
        """Render assistant content as simple plain text with a leading symbol."""
        self.message_printer.print_markdown_message(content, symbol=symbol)

    def _notify(self, message: str, level: str = "info", *, toast: bool = True) -> None:
        """Record a notification and optionally display a toast."""
        title_map = {
            "info": ("Info", "cyan"),
            "warning": ("Warning", "yellow"),
            "error": ("Error", "red"),
        }
        title, style = title_map.get(level, ("Info", "cyan"))
        self._notification_center.add(level, message)
        if toast:
            self.console.print(Panel(message, title=title, border_style=style, expand=False))

    def _show_notifications(self) -> None:
        """Render the notification center."""
        self.console.print()
        self._notification_center.render()

    def _render_context_overview(self) -> None:
        """Render a compact context sidebar above the prompt."""
        self.context_display.render(
            last_prompt=self._last_prompt,
            last_operation_summary=self._last_operation_summary,
            last_error=self._last_error,
        )

    def start(self, *, startup_type: str = "startup") -> None:
        """Start the REPL loop.

        Args:
            startup_type: How the session started ("startup", "resume", "clear").
        """
        self._print_welcome()

        # Fire SessionStart hook
        if self._hook_manager:
            from swecli.core.hooks.models import HookEvent

            self._hook_manager.run_hooks(
                HookEvent.SESSION_START, match_value=startup_type
            )

        # Connect to enabled MCP servers
        self._connect_mcp_servers()

        # Project instructions (SWECLI.md) are now included in the system prompt
        # via EnvironmentContext, so no separate reminder is needed here.

        while self.running:
            try:
                if self._context_sidebar_visible:
                    self._render_context_overview()

                self.input_frame.print_top()
                user_input = self.prompt_session.prompt(self.prompt_builder.build_tokens())
                self.input_frame.print_bottom()

                if not user_input.strip():
                    continue

                # Check for slash commands
                if user_input.startswith("/"):
                    self._handle_command(user_input)
                    continue

                self._last_prompt = user_input.strip()

                # Process as regular query
                self._process_query(user_input)

            except KeyboardInterrupt:
                self.console.print(f"\n[{WARNING}]Exiting...[/{WARNING}]")
                self.running = False
                break
            except EOFError:
                break

        self._cleanup()

    def _print_welcome(self) -> None:
        """Print compact welcome banner using shared welcome module."""
        from swecli.ui_textual.components import WelcomeMessage

        # Generate welcome content using shared module
        welcome_lines = WelcomeMessage.generate_full_welcome(
            current_mode=self.mode_manager.current_mode,
            working_dir=self.config_manager.working_dir,
        )

        # Print each line with Rich formatting
        for line in welcome_lines:
            # Apply color based on content
            if line.startswith("╔") or line.startswith("║") or line.startswith("╚"):
                self.console.print(f"[white]{line}[/white]")
            elif "Essential Commands:" in line:
                self.console.print(f"[bold white]{line}[/bold white]")
            elif "/help" in line or "/mode" in line:
                styled = line.replace("/help", f"[{CYAN}]/help[/{CYAN}]")
                styled = styled.replace("/mode plan", f"[{CYAN}]/mode plan[/{CYAN}]")
                styled = styled.replace("/mode normal", f"[{CYAN}]/mode normal[/{CYAN}]")
                self.console.print(styled)
            elif "Shortcuts:" in line:
                styled = f"[bold white]{line.split(':')[0]}:[/bold white]"
                rest = line.split(":", 1)[1] if ":" in line else ""
                styled += rest.replace("Shift+Tab", f"[{WARNING}]Shift+Tab[/{WARNING}]")
                styled = styled.replace("@file", f"[{WARNING}]@file[/{WARNING}]")
                styled = styled.replace("↑↓", f"[{WARNING}]↑↓[/{WARNING}]")
                self.console.print(styled)
            elif "Session:" in line:
                mode = self.mode_manager.current_mode.value.upper()
                mode_color = "green" if mode == "PLAN" else "yellow"
                styled = f"[bold white]{line.split(':')[0]}:[/bold white]"
                rest = line.split(":", 1)[1] if ":" in line else ""
                if mode in rest:
                    rest = rest.replace(mode, f"[{mode_color}]{mode}[/{mode_color}]")
                styled += rest
                self.console.print(styled)
            else:
                self.console.print(line)

    def _process_query(self, query: str) -> None:
        """Process a user query with AI using ReAct pattern.

        Args:
            query: User query
        """
        # Check for pending plan request
        plan_requested = self._pending_plan_request
        if plan_requested:
            self._pending_plan_request = False

        # Delegate to query processor
        result = self.query_processor.process_query(
            query,
            self.agent,
            self.tool_registry,
            self.approval_manager,
            self.undo_manager,
            plan_requested=plan_requested,
        )

        # Update state from query processor results
        self._last_operation_summary, self._last_error, self._last_latency_ms = result

    def _process_query_with_callback(self, query: str, ui_callback) -> None:
        """Process a user query with AI using ReAct pattern with UI callback for real-time updates.

        Args:
            query: User query
            ui_callback: UI callback for real-time tool display
        """
        # Check for pending plan request
        plan_requested = self._pending_plan_request
        if plan_requested:
            self._pending_plan_request = False

        # Delegate to query processor with callback
        if hasattr(self.query_processor, "process_query_with_callback"):
            result = self.query_processor.process_query_with_callback(
                query,
                self.agent,
                self.tool_registry,
                self.approval_manager,
                self.undo_manager,
                ui_callback,
                plan_requested=plan_requested,
            )
        else:
            # Fallback to normal processing if callback method doesn't exist
            result = self.query_processor.process_query(
                query,
                self.agent,
                self.tool_registry,
                self.approval_manager,
                self.undo_manager,
                plan_requested=plan_requested,
            )

        # Update state from query processor results
        self._last_operation_summary, self._last_error, self._last_latency_ms = result

    def _handle_command(self, command: str) -> None:
        """Handle slash commands.

        Args:
            command: Command string (including /)
        """
        parts = command.split(maxsplit=1)
        cmd = parts[0].lower()
        args = parts[1] if len(parts) > 1 else ""

        # Route to command handlers
        if cmd == "/help":
            self.help_command.handle(args)
        elif cmd == "/exit" or cmd == "/quit":
            self.running = False
        elif cmd == "/clear":
            self.session_commands.clear()
        elif cmd == "/mode":
            # mode_commands.switch_mode calls mode_manager.set_mode,
            # which triggers the callback to handle agent swapping
            self.mode_commands.switch_mode(args)
        elif cmd == "/models":
            self.config_commands.show_model_selector()
        elif cmd == "/mcp":
            self.mcp_commands.handle(args)
        elif cmd == "/init":
            self.tool_commands.init_codebase(command)
        elif cmd == "/agents":
            self.agents_commands.handle(args)
        elif cmd == "/skills":
            self.skills_commands.handle(args)
        elif cmd == "/plugins":
            self.plugins_commands.handle(args)
        elif cmd == "/compact":
            self.session_commands.compact()
        elif cmd == "/sound":
            from swecli.core.utils.sound import play_finish_sound
            play_finish_sound()
            self.console.print("  ⎿  [cyan]Playing test sound...[/cyan]")
        else:
            self.console.print(f"  ⎿  [{ERROR}]Unknown command[/{ERROR}]")
            self.console.print("  ⎿  Type /help for available commands")
            self.console.print("")  # Blank line for spacing

    def _connect_mcp_servers(self) -> None:
        """Connect to enabled MCP servers on startup asynchronously."""
        import threading

        def connect_in_background():
            """Background thread to connect to MCP servers."""
            try:
                # Get connection results using synchronous wrapper
                results = self.mcp_manager.connect_enabled_servers_sync()

                if results:
                    # Silently connect - no messages
                    self._refresh_runtime_tooling()
            except Exception:
                # Silently fail - user can check with /mcp list
                pass

        # Start connection in background thread - silently
        thread = threading.Thread(target=connect_in_background, daemon=True)
        thread.start()

    def _cleanup(self) -> None:
        """Cleanup resources."""
        # Fire SessionEnd hook
        if self._hook_manager:
            from swecli.core.hooks.models import HookEvent

            try:
                self._hook_manager.run_hooks(HookEvent.SESSION_END)
            except Exception:
                pass  # Best-effort on shutdown
            self._hook_manager.shutdown()

        # Disconnect from MCP servers using manager's shared loop
        try:
            self.mcp_manager.disconnect_all_sync()
        except Exception as e:
            self.console.print(
                f"[{WARNING}]Warning: Error disconnecting MCP servers: {e}[/{WARNING}]"
            )

        # Save current session
        if self.session_manager.current_session:
            self.session_manager.save_session()

        # Display session cost summary on exit
        cost_tracker = getattr(self, "_cost_tracker", None)
        if cost_tracker is None:
            # Try to find it via react_executor
            react_exec = getattr(self, "_react_executor", None)
            if react_exec:
                cost_tracker = getattr(react_exec, "_cost_tracker", None)
        if cost_tracker and cost_tracker.total_cost_usd > 0:
            cost_str = cost_tracker.format_cost()
            tokens = cost_tracker.total_input_tokens + cost_tracker.total_output_tokens
            token_str = f"{tokens / 1000:.1f}K" if tokens >= 1000 else str(tokens)
            self.console.print(
                f"\n[{GREY}]Session cost: {cost_str} | {token_str} tokens | "
                f"{cost_tracker.call_count} API calls[/{GREY}]"
            )

        self.console.print(f"[{CYAN}]Goodbye![/{CYAN}]")
