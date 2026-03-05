"""Slash command routing for the Textual chat app."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:  # pragma: no cover
    from swecli.ui_textual.chat_app import SWECLIChatApp


class CommandRouter:
    """Handle slash commands issued from the Textual chat input."""

    def __init__(self, app: "SWECLIChatApp") -> None:
        self.app = app

    async def handle(self, command: str) -> bool:
        """Dispatch a slash command. Returns True if handled locally."""

        parts = command.split()
        cmd = parts[0].lower()
        conversation = getattr(self.app, "conversation", None)

        if cmd == "/help":
            if conversation is None:
                return True
            self._render_help(conversation)
            return True

        if cmd == "/clear":
            if conversation is not None:
                conversation.clear()
                conversation.add_system_message("Conversation cleared.")
            return True

        if cmd == "/sound":
            from swecli.core.utils.sound import play_finish_sound
            play_finish_sound()
            if conversation is not None:
                conversation.add_system_message("Playing test sound...")
            return True

        if cmd == "/demo":
            if conversation is not None:
                self._render_demo(conversation)
            return True

        if cmd == "/models":
            # Open model picker directly
            await self.app._start_model_picker()
            return True

        if cmd == "/agents":
            args = " ".join(parts[1:]) if len(parts) > 1 else ""
            if args.lower().startswith("create"):
                await self.app._agent_creator.start()
                return True
            # Fall through for other subcommands (list, edit, delete)
            return False

        if cmd == "/skills":
            args = " ".join(parts[1:]) if len(parts) > 1 else ""
            if args.lower().startswith("create"):
                await self.app._skill_creator.start()
                return True
            # Fall through for other subcommands (list, edit, delete, test)
            return False

        if cmd == "/scroll":
            if conversation is not None:
                self._render_scroll_demo(conversation)
            return True

        if cmd == "/quit":
            self.app.exit()
            return True

        # Background task commands
        if cmd == "/tasks":
            if conversation is not None:
                await self._handle_tasks(conversation)
            return True

        if cmd == "/task" and len(parts) > 1:
            if conversation is not None:
                task_id = parts[1]
                await self._handle_task_output(conversation, task_id)
            return True

        if cmd == "/kill" and len(parts) > 1:
            if conversation is not None:
                task_id = parts[1]
                await self._handle_kill_task(conversation, task_id)
            return True

        if cmd == "/permissions":
            if conversation is not None:
                subcommand = parts[1] if len(parts) > 1 else "list"
                await self._handle_permissions(conversation, subcommand)
            return True

        if cmd == "/undo":
            if conversation is not None:
                await self._handle_undo(conversation)
            return True

        return False

    def _render_help(self, conversation) -> None:
        conversation.add_system_message("Available commands:")
        conversation.add_system_message("  /help - Show this help")
        conversation.add_system_message("  /clear - Clear conversation")
        conversation.add_system_message("  /sound - Play test notification sound")
        conversation.add_system_message("  /demo - Show demo messages")
        conversation.add_system_message("  /scroll - Generate many messages (test scrolling)")
        conversation.add_system_message("  /models - Configure model slots")
        conversation.add_system_message("  /agents create - Create new agent with wizard")
        conversation.add_system_message("  /skills - Create and manage custom skills")
        conversation.add_system_message("  /plugins - Manage plugins and marketplaces")
        conversation.add_system_message("  /tasks - List background tasks")
        conversation.add_system_message("  /task <id> - Show task output")
        conversation.add_system_message("  /kill <id> - Kill a background task")
        conversation.add_system_message("  /permissions - View/clear persistent permission rules")
        conversation.add_system_message("  /undo - Undo the last agent action")
        conversation.add_system_message("  /quit - Exit application")
        conversation.add_system_message("")
        conversation.add_system_message("Multi-line Input:")
        conversation.add_system_message("  Enter - Send message")
        conversation.add_system_message("  Shift+Enter - New line in message")
        conversation.add_system_message("  Type multiple lines, then press Enter to send!")
        conversation.add_system_message("")
        conversation.add_system_message("Scrolling:")
        conversation.add_system_message("  Ctrl+Up - Focus conversation (then use arrow keys)")
        conversation.add_system_message("  Ctrl+Down - Focus input (for typing)")
        conversation.add_system_message("  Arrow Up/Down - Scroll line by line")
        conversation.add_system_message("  Page Up/Down - Scroll by page")
        conversation.add_system_message("")
        conversation.add_system_message("Other Shortcuts:")
        conversation.add_system_message("  Ctrl+L - Clear conversation")
        conversation.add_system_message("  Ctrl+C - Quit application")
        conversation.add_system_message("  ESC - Interrupt processing")

    def _render_demo(self, conversation) -> None:
        conversation.add_assistant_message("Here's a demo of different message types:")
        conversation.add_system_message("")

        conversation.add_tool_call("Shell", "command='ls -la'")
        conversation.add_tool_result("total 64\ndrwxr-xr-x  10 user  staff   320 Jan 27 10:00 .")

        conversation.add_system_message("")
        conversation.add_tool_call("Read", "file_path='swecli/cli.py'")
        conversation.add_tool_result("File read successfully (250 lines)")

        conversation.add_system_message("")
        conversation.add_tool_call("Write", "file_path='test.py', content='...'")
        conversation.add_tool_result("File written successfully")

        conversation.add_system_message("")
        conversation.add_error("Example error: File not found")

    def _render_scroll_demo(self, conversation) -> None:
        conversation.add_assistant_message("Generating 50 messages to test scrolling...")
        conversation.add_system_message("")
        for i in range(1, 51):
            if i % 10 == 0:
                conversation.add_system_message(f"--- Message {i} ---")
            elif i % 5 == 0:
                conversation.add_tool_call("TestTool", f"iteration={i}")
                conversation.add_tool_result(f"Result for iteration {i}")
            elif i % 3 == 0:
                conversation.add_user_message(f"Test user message {i}")
            else:
                conversation.add_assistant_message(
                    f"Test assistant message {i}: Lorem ipsum dolor sit amet, consectetur adipiscing elit."
                )
        conversation.add_system_message("")
        conversation.add_assistant_message("✓ Done! Try scrolling up with mouse wheel or Page Up.")

    async def _handle_tasks(self, conversation) -> None:
        """List all background tasks."""
        task_manager = getattr(self.app, "_task_manager", None)
        if task_manager is None:
            conversation.add_tool_call("Background Tasks", "")
            conversation.add_tool_result("No task manager available")
            return

        tasks = task_manager.get_all_tasks()

        conversation.add_tool_call("Background Tasks", "")

        if not tasks:
            conversation.add_tool_result("No background tasks")
            return

        lines = []
        for t in tasks:
            status = "running" if t.is_running else t.status.name.lower()
            runtime = f"{t.runtime_seconds:.1f}s"
            lines.append(f"{t.task_id}: {t.command[:40]} ({runtime}, {status})")

        conversation.add_tool_result("\n".join(lines))

    async def _handle_task_output(self, conversation, task_id: str) -> None:
        """Show output for a specific task."""
        task_manager = getattr(self.app, "_task_manager", None)
        if task_manager is None:
            conversation.add_tool_call(f"Task Output ({task_id})", "")
            conversation.add_tool_result("No task manager available")
            return

        task = task_manager.get_task(task_id)
        if not task:
            conversation.add_tool_call(f"Task Output ({task_id})", "")
            conversation.add_error(f"Task '{task_id}' not found")
            return

        conversation.add_tool_call(f"Task Output ({task_id})", task.command[:30])

        output = task_manager.read_output(task_id)
        if output:
            conversation.add_tool_result(output)
        else:
            conversation.add_tool_result("(no output yet)")

    async def _handle_kill_task(self, conversation, task_id: str) -> None:
        """Kill a background task."""
        task_manager = getattr(self.app, "_task_manager", None)
        if task_manager is None:
            conversation.add_tool_call(f"Kill Task ({task_id})", "")
            conversation.add_tool_result("No task manager available")
            return

        conversation.add_tool_call(f"Kill Task ({task_id})", "")

        if task_manager.kill_task(task_id):
            conversation.add_tool_result(f"Task {task_id} killed")
        else:
            conversation.add_error(f"Failed to kill task {task_id}")

    async def _handle_permissions(self, conversation, subcommand: str) -> None:
        """View or clear persistent permission rules."""
        rules_mgr = getattr(self.app, "_approval_rules_manager", None)
        if rules_mgr is None:
            # Try to get it from the runner/repl
            runner = getattr(self.app, "runner", None)
            repl = getattr(runner, "repl", None) if runner else None
            rules_mgr = getattr(repl, "_approval_rules_manager", None) if repl else None

        if rules_mgr is None:
            conversation.add_system_message("Approval rules manager not available.")
            return

        if subcommand == "clear":
            count = rules_mgr.clear_persistent_rules(scope="all")
            conversation.add_system_message(f"Cleared {count} persistent rules.")
            return

        # Default: list rules
        rules = rules_mgr.list_persistent_rules()
        if not rules:
            conversation.add_system_message("No persistent permission rules configured.")
            conversation.add_system_message("Rules are created automatically when you choose 'Always allow' during approval prompts.")
            return

        conversation.add_system_message("Persistent permission rules:")
        for r in rules:
            status = "enabled" if r["enabled"] else "disabled"
            conversation.add_system_message(
                f"  [{r['id']}] {r['name']} — {r['action']} on {r['type']}:{r['pattern']} ({status})"
            )
        conversation.add_system_message("")
        conversation.add_system_message("Use /permissions clear to remove all persistent rules.")

    async def _handle_undo(self, conversation) -> None:
        """Undo the last agent action using the snapshot system."""
        snapshot_mgr = getattr(self.app, "_snapshot_manager", None)
        if snapshot_mgr is None:
            # Try to find it via the runner
            runner = getattr(self.app, "runner", None)
            repl = getattr(runner, "repl", None) if runner else None
            react_exec = getattr(repl, "_react_executor", None) if repl else None
            snapshot_mgr = getattr(react_exec, "_snapshot_manager", None) if react_exec else None

        if snapshot_mgr is None:
            conversation.add_system_message("Snapshot system not available. Cannot undo.")
            return

        result = snapshot_mgr.undo_last()
        if result:
            conversation.add_system_message(f"Reverted: {result}")
        else:
            conversation.add_system_message("Nothing to undo (no snapshots recorded).")


__all__ = ["CommandRouter"]
