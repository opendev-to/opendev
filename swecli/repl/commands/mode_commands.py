"""Mode commands for REPL."""

from typing import TYPE_CHECKING, Any

from rich.console import Console

from swecli.repl.commands.base import CommandHandler, CommandResult

if TYPE_CHECKING:
    from swecli.repl.repl import REPL


class ModeCommands(CommandHandler):
    """Handler for mode-related commands: /mode."""

    def __init__(
        self,
        console: Console,
        repl: "REPL",
    ):
        """Initialize mode commands handler.

        Args:
            console: Rich console for output
            repl: Reference to REPL for accessing current managers
        """
        super().__init__(console)
        self._repl = repl

    # Properties to access managers dynamically (avoids stale references)
    @property
    def mode_manager(self) -> Any:
        return self._repl.mode_manager

    @property
    def approval_manager(self) -> Any:
        return self._repl.approval_manager

    def handle(self, args: str) -> CommandResult:
        """Handle mode command (not used, individual methods called directly)."""
        raise NotImplementedError("Use specific method: switch_mode()")

    def switch_mode(self, mode_name: str) -> CommandResult:
        """Switch operation mode.

        /mode plan — sets pending plan request flag (next query triggers planning)
        /mode normal — interrupts planner if active, clears plan request
        /mode — shows current status

        Args:
            mode_name: Mode to switch to (normal/plan) or empty to show current

        Returns:
            CommandResult indicating success or failure
        """
        from swecli.core.runtime.mode_manager import OperationMode

        if not mode_name:
            # Show current status
            in_plan_mode = self.mode_manager.current_mode == OperationMode.PLAN
            plan_requested = getattr(self._repl, "_pending_plan_request", False)
            if in_plan_mode:
                status = "PLAN MODE (active)"
            elif plan_requested:
                status = "PLAN REQUESTED (next query triggers planning)"
            else:
                status = "NORMAL"
            self.print_result_only(f"Current: {status}")
            self.print_result_only("[dim]Available: normal, plan[/dim]")
            return CommandResult(success=True)

        mode_name = mode_name.strip().lower()

        if mode_name == "plan":
            if self.mode_manager.current_mode == OperationMode.PLAN:
                self.print_result_only("Already in plan mode.")
                return CommandResult(success=True, message="Already in plan mode")

            self._repl._pending_plan_request = True
            self.print_success("Plan mode requested. Next query will trigger planning.")
            return CommandResult(success=True, message="Plan mode requested")

        elif mode_name == "normal":
            if self.mode_manager.current_mode == OperationMode.PLAN:
                self.mode_manager.set_mode(OperationMode.NORMAL)
                self.print_success("Switched to Normal mode.")
            self._repl._pending_plan_request = False
            self.print_success("Normal mode")
            return CommandResult(success=True, message="Normal mode")

        else:
            self.print_error(f"Unknown mode: {mode_name}")
            self.print_result_only("[dim]Available: normal, plan[/dim]")
            return CommandResult(success=False, message=f"Unknown mode: {mode_name}")
