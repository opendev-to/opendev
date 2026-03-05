"""Session management commands for REPL."""

from typing import TYPE_CHECKING

from rich.console import Console

from swecli.repl.commands.base import CommandHandler, CommandResult

if TYPE_CHECKING:
    from swecli.core.runtime import ConfigManager
    from swecli.core.context_engineering.history import SessionManager


class SessionCommands(CommandHandler):
    """Handler for session-related commands: /clear."""

    def __init__(
        self,
        console: Console,
        session_manager: "SessionManager",
        config_manager: "ConfigManager",
    ):
        """Initialize session commands handler.

        Args:
            console: Rich console for output
            session_manager: Session manager instance
            config_manager: Configuration manager instance
        """
        super().__init__(console)
        self.session_manager = session_manager
        self.config_manager = config_manager

    def handle(self, args: str) -> CommandResult:
        """Handle session command (not used, individual methods called directly)."""
        raise NotImplementedError("Use specific method: clear()")

    def compact(self) -> CommandResult:
        """Compact conversation history to reduce context size.

        Returns:
            CommandResult indicating success
        """
        session = self.session_manager.get_current_session()
        if not session or len(session.messages) < 5:
            self.print_warning("Not enough messages to compact.")
            self.console.print()
            return CommandResult(success=False, message="Not enough messages")

        from swecli.core.context_engineering.compaction import ContextCompactor
        from swecli.core.agents.components.api.configuration import create_http_client

        config = self.config_manager.get_config()
        http_client = create_http_client(config)
        compactor = ContextCompactor(config, http_client)

        messages = session.to_api_messages()
        system_prompt = ""

        before_count = len(messages)
        compacted = compactor.compact(messages, system_prompt)
        after_count = len(compacted)

        # Store compaction point in session metadata for prepare_messages to use
        summary_msg = next(
            (m for m in compacted if m.get("content", "").startswith("[CONVERSATION SUMMARY]")),
            None,
        )
        if summary_msg:
            session.metadata["compaction_point"] = {
                "summary": summary_msg["content"],
                "at_message_count": len(session.messages),
            }
            self.session_manager.save_session()

        self.print_success(f"Compacted {before_count} → {after_count} messages")
        self.console.print()
        return CommandResult(success=True, message=f"Compacted {before_count} → {after_count}")

    def clear(self) -> CommandResult:
        """Clear current session and create a new one.

        Returns:
            CommandResult indicating success
        """
        if self.session_manager.current_session:
            self.session_manager.save_session()
            self.session_manager.create_session(
                working_directory=str(self.config_manager.working_dir)
            )
            self.print_success("Session cleared. Previous session saved.")
            self.console.print()
            return CommandResult(success=True, message="Session cleared")
        else:
            self.print_warning("No active session to clear.")
            self.console.print()
            return CommandResult(success=False, message="No active session")
