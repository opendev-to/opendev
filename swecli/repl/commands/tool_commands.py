"""Command handlers for tool-related commands (/init)."""

from pathlib import Path
from typing import TYPE_CHECKING, Any, Optional

from rich.console import Console

from swecli.core.agents.prompts import load_prompt
from swecli.models.agent_deps import AgentDependencies
from swecli.repl.commands.base import CommandHandler, CommandResult

if TYPE_CHECKING:
    from swecli.repl.repl import REPL


class ToolCommands(CommandHandler):
    """Handler for tool-related commands."""

    def __init__(
        self,
        console: Console,
        repl: "REPL",
    ):
        """Initialize tool commands handler.

        Args:
            console: Rich console for output
            repl: Reference to REPL for accessing current managers
        """
        super().__init__(console)
        self._repl = repl
        # UI callback for TUI mode - set by runner
        self.ui_callback: Optional[Any] = None

    # Properties to access managers dynamically (avoids stale references)
    @property
    def config(self) -> Any:
        return self._repl.config

    @property
    def mode_manager(self) -> Any:
        return self._repl.mode_manager

    @property
    def approval_manager(self) -> Any:
        return self._repl.approval_manager

    @property
    def undo_manager(self) -> Any:
        return self._repl.undo_manager

    @property
    def session_manager(self) -> Any:
        return self._repl.session_manager

    @property
    def agent(self) -> Any:
        return self._repl.agent

    def handle(self, args: str) -> CommandResult:
        """Handle generic command - not used as this handler supports multiple commands."""
        return CommandResult(success=False, message="Use specific methods for each command")

    def init_codebase(self, command: str) -> None:
        """Handle /init command to analyze codebase and generate OPENDEV.md.

        Runs the main agent with an init system prompt that instructs it to use
        spawn_subagent with Code-Explorer to explore the codebase, then write OPENDEV.md.

        Args:
            command: The full command string (e.g., "/init" or "/init /path/to/project")
        """
        # Parse path from command
        parts = command.strip().split()
        if len(parts) > 1:
            target_path = Path(parts[1]).expanduser().absolute()
        else:
            target_path = Path.cwd()

        # Validate path
        if not target_path.exists():
            self.print_command_header("init")
            self.print_error(f"Path does not exist: {target_path}")
            return

        if not target_path.is_dir():
            self.print_command_header("init")
            self.print_error(f"Path is not a directory: {target_path}")
            return

        # Load init system prompt and substitute path
        try:
            task_prompt = load_prompt("system/init_system_prompt")
            task_prompt = task_prompt.replace("{path}", str(target_path))
        except Exception as e:
            self.print_command_header("init")
            self.print_error(f"Failed to load init prompt: {e}")
            return

        # Create dependencies for agent execution
        deps = AgentDependencies(
            mode_manager=self.mode_manager,
            approval_manager=self.approval_manager,
            undo_manager=self.undo_manager,
            session_manager=self.session_manager,
            working_dir=target_path,
            console=self.console,
            config=self.config,
        )

        # Run main agent with the init prompt
        # The agent will use spawn_subagent to call Code-Explorer
        try:
            result = self.agent.run_sync(
                message=task_prompt,
                deps=deps,
                ui_callback=self.ui_callback,
            )

            # Check if OPENDEV.md was created
            opendev_path = target_path / "OPENDEV.md"

            # Display final message if task succeeded
            if result.get("success") and opendev_path.exists() and self.ui_callback:
                final_content = result.get("content", "").strip()
                if final_content:
                    # Agent already provided a summary - display it
                    self.ui_callback.on_assistant_message(final_content)
                else:
                    # No summary from agent - prompt LLM for one
                    from swecli.core.agents.prompts import get_reminder

                    signal = get_reminder("init_complete_signal", path=str(opendev_path))

                    summary_result = self.agent.run_sync(
                        message=signal,
                        deps=deps,
                        message_history=result.get("messages", []),
                        max_iterations=1,
                        ui_callback=self.ui_callback,
                    )
                    if summary_result.get("content"):
                        self.ui_callback.on_assistant_message(summary_result["content"])

            if not self.ui_callback:
                # Only print result in non-TUI mode (TUI shows via callback)
                self.print_command_header("init")
                if result.get("success") and opendev_path.exists():
                    self.print_success(f"Generated OPENDEV.md at {opendev_path}")
                elif result.get("success"):
                    self.print_success("Analysis complete")
                else:
                    self.print_error(result.get("error", "Unknown error"))

        except Exception as e:
            if not self.ui_callback:
                self.print_command_header("init")
            self.print_error(f"Error during initialization: {e}")
            import traceback
            traceback.print_exc()
