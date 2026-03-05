"""Approval manager for chat interface with interactive prompts."""

import asyncio
from enum import Enum


class AutonomyLevel(Enum):
    """Autonomy levels for command approval."""
    MANUAL = "Manual"
    SEMI_AUTO = "Semi-Auto"
    AUTO = "Auto"


# Safe commands that can be auto-approved in Semi-Auto mode
SAFE_COMMANDS = [
    "ls", "cat", "head", "tail", "grep", "find", "wc", "pwd",
    "echo", "which", "type", "file", "stat", "du", "df", "tree",
    "git status", "git log", "git diff", "git branch", "git show",
    "git remote", "git tag", "git stash list",
    "python --version", "python3 --version", "node --version",
    "npm --version", "cargo --version", "go version",
]


class ChatApprovalManager:
    """Approval manager for chat interface with interactive prompts."""

    def __init__(self, console, chat_app=None):
        self.console = console
        self.chat_app = chat_app  # Reference to chat application for run_in_terminal
        self.auto_approve_remaining = False
        self.approved_patterns = set()
        self.pre_approved_commands = set()  # Commands that were already approved
        self.autonomy_level = AutonomyLevel.MANUAL  # Current autonomy level

        # Initialize rules manager for Phase 3
        from swecli.core.runtime.approval import ApprovalRulesManager, RuleAction

        self.rules_manager = ApprovalRulesManager()
        self.RuleAction = RuleAction  # Store for use in methods

    def set_autonomy_level(self, level: str) -> None:
        """Set the autonomy level.

        Args:
            level: One of "Manual", "Semi-Auto", or "Auto"
        """
        level_map = {
            "Manual": AutonomyLevel.MANUAL,
            "Semi-Auto": AutonomyLevel.SEMI_AUTO,
            "Auto": AutonomyLevel.AUTO,
        }
        self.autonomy_level = level_map.get(level, AutonomyLevel.MANUAL)

    def _is_safe_command(self, command: str) -> bool:
        """Check if a command is considered safe for auto-approval.

        Args:
            command: The command string to check

        Returns:
            True if the command starts with a safe prefix
        """
        if not command:
            return False
        cmd_lower = command.strip().lower()
        return any(cmd_lower.startswith(safe.lower()) for safe in SAFE_COMMANDS)

    def _check_auto_approval(self, operation, command):
        """Check if operation should be auto-approved.

        Args:
            operation: Operation to check
            command: Command string

        Returns:
            ApprovalResult if auto-approved, None otherwise
        """
        from swecli.core.runtime.approval import ApprovalResult, ApprovalChoice
        from swecli.models.operation import OperationType

        # Only show approval for bash commands
        if operation and operation.type != OperationType.BASH_EXECUTE:
            return ApprovalResult(
                approved=True,
                choice=ApprovalChoice.APPROVE,
                apply_to_all=False,
            )

        # Check autonomy level first
        if self.autonomy_level == AutonomyLevel.AUTO:
            # Auto mode: approve everything without prompting
            return ApprovalResult(
                approved=True,
                choice=ApprovalChoice.APPROVE,
                apply_to_all=True,
            )

        if self.autonomy_level == AutonomyLevel.SEMI_AUTO:
            # Semi-Auto mode: auto-approve safe commands
            if self._is_safe_command(command):
                return ApprovalResult(
                    approved=True,
                    choice=ApprovalChoice.APPROVE,
                    apply_to_all=False,
                )

        # Check if pre-approved
        if command and command in self.pre_approved_commands:
            self.pre_approved_commands.discard(command)
            return ApprovalResult(
                approved=True,
                choice=ApprovalChoice.APPROVE,
                apply_to_all=False,
            )

        # Check pattern-based auto-approval (exact match or prefix match)
        if command and any(
            command == pattern or command.startswith(pattern + " ")
            for pattern in self.approved_patterns
        ):
            return ApprovalResult(
                approved=True,
                choice=ApprovalChoice.APPROVE_ALL,
                apply_to_all=True,
            )

        return None

    def _check_approval_rules(self, command):
        """Check and apply approval rules.

        Args:
            command: Command to check

        Returns:
            Tuple of (ApprovalResult or None, matched_rule or None)
        """
        from swecli.core.runtime.approval import ApprovalResult, ApprovalChoice

        if not command:
            return None, None

        matched_rule = self.rules_manager.evaluate_command(command)
        if not matched_rule:
            return None, None

        # Auto-approve rule
        if matched_rule.action == self.RuleAction.AUTO_APPROVE:
            self.rules_manager.add_history(command, True, rule_matched=matched_rule.id)
            # Don't show auto-approval messages in chat to reduce noise
            # self.console.print(f"[dim]✓ Auto-approved by rule: {matched_rule.name}[/dim]")
            return ApprovalResult(
                approved=True,
                choice=ApprovalChoice.APPROVE,
                apply_to_all=False,
            ), matched_rule

        # Auto-deny rule
        if matched_rule.action == self.RuleAction.AUTO_DENY:
            self.rules_manager.add_history(command, False, rule_matched=matched_rule.id)
            self.console.print(f"  ⎿  [red]Denied by rule: {matched_rule.name}[/red]")
            return ApprovalResult(
                approved=False,
                choice=ApprovalChoice.DENY,
                cancelled=True,
            ), matched_rule

        # REQUIRE_APPROVAL or REQUIRE_EDIT - continue to modal
        return None, matched_rule

    async def _show_approval_modal(self, command, working_dir):
        """Show approval modal to user.

        Args:
            command: Command to approve
            working_dir: Working directory

        Returns:
            Tuple of (approved, choice, edited_command)
        """

        if not self.chat_app:
            return self._fallback_prompt(command, working_dir, RuntimeError("Chat app unavailable"))

        # If the UI isn't running (e.g., during CLI tests) fall back to console prompt.
        if not getattr(self.chat_app, "is_running", False):
            return self._fallback_prompt(command, working_dir, RuntimeError("Chat app not running"))

        origin_loop = asyncio.get_running_loop()
        ui_loop_future: asyncio.Future[tuple[bool, str, str]]
        ui_loop_future = origin_loop.create_future()

        def invoke_modal() -> None:
            async def run_modal() -> None:
                try:
                    result = await self.chat_app.show_approval_modal(command or "", working_dir or "")
                except Exception as exc:  # pragma: no cover - defensive
                    origin_loop.call_soon_threadsafe(ui_loop_future.set_exception, exc)
                else:
                    origin_loop.call_soon_threadsafe(ui_loop_future.set_result, result)

            worker = self.chat_app.run_worker(
                run_modal(),
                name="approval-modal",
                exclusive=True,
                exit_on_error=False,
            )
            worker.wait()

        try:
            self.chat_app.call_from_thread(invoke_modal)
        except RuntimeError as exc:  # pragma: no cover - defensive
            return self._fallback_prompt(command, working_dir, exc)

        try:
            return await ui_loop_future
        except Exception as exc:
            return self._fallback_prompt(command, working_dir, exc)

    def _fallback_prompt(self, command, working_dir, error):
        """Fallback to simple text prompt when modal fails.

        Args:
            command: Command to approve
            working_dir: Working directory
            error: Exception that caused fallback

        Returns:
            Tuple of (approved, choice, edited_command)
        """
        import traceback

        print(f"\n\033[31mModal error: {error}\033[0m")
        print(f"\033[31mTraceback:\033[0m")
        traceback.print_exc()
        print("Falling back to simple prompt...\n")
        print("\n\033[1;33m╭─ Bash Command Approval ─╮\033[0m")
        print(f"\033[1;33m│\033[0m Command: \033[36m{command}\033[0m")
        print(f"\033[1;33m│\033[0m Working directory: {working_dir}")
        print("\033[1;33m╰──────────────────────────╯\033[0m\n")
        print("  \033[1;36m1.\033[0m Yes, run this command")
        print("  \033[1;36m2.\033[0m Yes, and don't ask again for similar commands")
        print("  \033[1;36m3.\033[0m No, cancel this operation\n")

        try:
            print("\033[1;33mChoose an option (1-3):\033[0m ", end="", flush=True)
            choice = input().strip()
            approved = choice in ["1", "2"]
            return approved, choice, command
        except (KeyboardInterrupt, EOFError):
            print("\n\033[33mOperation cancelled\033[0m")
            return False, "3", command

    def _show_edited_command(self, command, edited_command):
        """Show notification if command was edited.

        Args:
            command: Original command
            edited_command: Edited command
        """
        if edited_command != command:
            if self.chat_app:
                content = f"Command edited to: {edited_command}"
                ledger = getattr(self.chat_app, "_display_ledger", None)
                if ledger:
                    ledger.display_assistant_message(content, "approval_manager")
                else:
                    self.chat_app.add_assistant_message(content)
            else:
                self.console.print(f"[yellow]Command edited to:[/yellow] {edited_command}")

    def _process_approve_choice(self, command, edited_command, matched_rule):
        """Process single approval choice.

        Args:
            command: Original command
            edited_command: Edited command
            matched_rule: Matched approval rule

        Returns:
            ApprovalResult
        """
        from swecli.core.runtime.approval import ApprovalResult, ApprovalChoice

        self._show_edited_command(command, edited_command)

        self.rules_manager.add_history(
            command,
            True,
            edited_command=edited_command if edited_command != command else None,
            rule_matched=matched_rule.id if matched_rule else None,
        )

        return ApprovalResult(
            approved=True,
            choice=ApprovalChoice.APPROVE,
            edited_content=edited_command if edited_command != command else None,
            apply_to_all=False,
        )

    def _process_approve_all_choice(self, command, edited_command, matched_rule):
        """Process approve-all choice.

        Args:
            command: Original command
            edited_command: Edited command
            matched_rule: Matched approval rule

        Returns:
            ApprovalResult
        """
        from swecli.core.runtime.approval import ApprovalResult, ApprovalChoice
        from swecli.core.runtime.approval.rules import ApprovalRule, RuleType, RuleAction
        from datetime import datetime
        import uuid

        # Store pattern for future auto-approval (in-memory for this session)
        pattern_cmd = edited_command if edited_command else command
        if pattern_cmd:
            base_cmd = " ".join(pattern_cmd.split()[:2])
            # Store without trailing space - matching logic handles both exact and prefix cases
            self.approved_patterns.add(base_cmd)

            # Create persistent approval rule
            rule_id = f"user_approved_{uuid.uuid4().hex[:8]}"
            rule = ApprovalRule(
                id=rule_id,
                name=f"Auto-approve: {base_cmd}",
                description=f"User approved command starting with '{base_cmd}' on {datetime.now().strftime('%Y-%m-%d %H:%M')}",
                rule_type=RuleType.PREFIX,
                pattern=base_cmd,  # No trailing space - rule matching handles both cases
                action=RuleAction.AUTO_APPROVE,
                enabled=True,
                priority=50,
                created_at=datetime.now().isoformat(),
            )
            self.rules_manager.add_rule(rule)

        self._show_edited_command(command, edited_command)

        self.rules_manager.add_history(
            command,
            True,
            edited_command=edited_command if edited_command != command else None,
            rule_matched=matched_rule.id if matched_rule else None,
        )

        return ApprovalResult(
            approved=True,
            choice=ApprovalChoice.APPROVE_ALL,
            edited_content=edited_command if edited_command != command else None,
            apply_to_all=True,
        )

    def _process_deny_choice(self, command, matched_rule):
        """Process denial choice.

        Args:
            command: Command that was denied
            matched_rule: Matched approval rule

        Returns:
            ApprovalResult
        """
        from swecli.core.runtime.approval import ApprovalResult, ApprovalChoice

        self.rules_manager.add_history(
            command,
            False,
            rule_matched=matched_rule.id if matched_rule else None,
        )

        return ApprovalResult(
            approved=False,
            choice=ApprovalChoice.DENY,
            edited_content=None,
            cancelled=True,
        )

    async def request_approval(
        self,
        operation,
        preview: str,
        allow_edit: bool = True,
        timeout=None,
        *,
        command=None,
        working_dir=None,
        force_prompt: bool = False,
    ):
        """Request approval for an operation with interactive prompt."""
        from swecli.core.runtime.approval import ApprovalResult, ApprovalChoice
        from swecli.core.debug import get_debug_logger

        get_debug_logger().log(
            "approval_request",
            "approval",
            command=command,
            operation=operation.type.value if operation and hasattr(operation, "type") else None,
        )

        matched_rule = None

        # AUTO mode always bypasses approval, even with force_prompt
        if self.autonomy_level == AutonomyLevel.AUTO:
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="auto", choice="auto_mode"
            )
            return ApprovalResult(
                approved=True,
                choice=ApprovalChoice.APPROVE,
                apply_to_all=True,
            )

        # SEMI_AUTO mode: auto-approve safe commands even with force_prompt
        if self.autonomy_level == AutonomyLevel.SEMI_AUTO and self._is_safe_command(command):
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="auto", choice="semi_auto_safe"
            )
            return ApprovalResult(
                approved=True,
                choice=ApprovalChoice.APPROVE,
                apply_to_all=False,
            )

        if not force_prompt:
            auto_result = self._check_auto_approval(operation, command)
            if auto_result:
                get_debug_logger().log(
                    "approval_result",
                    "approval",
                    approved=auto_result.approved,
                    method="auto",
                    choice=auto_result.choice.value,
                )
                return auto_result

            rule_result, matched_rule = self._check_approval_rules(command)
            if rule_result:
                get_debug_logger().log(
                    "approval_result",
                    "approval",
                    approved=rule_result.approved,
                    method="rule",
                    choice=rule_result.choice.value,
                    rule=matched_rule.name if matched_rule else None,
                )
                return rule_result

        # Check if already interrupted before showing modal
        if self.chat_app and hasattr(self.chat_app, 'runner'):
            runner = self.chat_app.runner
            if hasattr(runner, 'query_processor'):
                task_monitor = getattr(runner.query_processor, 'task_monitor', None)
                if task_monitor and task_monitor.should_interrupt():
                    get_debug_logger().log(
                        "approval_result", "approval", approved=False, method="interrupted"
                    )
                    return ApprovalResult(
                        approved=False,
                        choice=ApprovalChoice.DENY,
                        cancelled=True,
                    )

        # Show approval modal
        approved, choice, edited_command = await self._show_approval_modal(command, working_dir)

        # Process user choice
        if choice == "1":
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="user", choice="approve"
            )
            return self._process_approve_choice(command, edited_command, matched_rule)
        elif choice == "2":
            get_debug_logger().log(
                "approval_result", "approval", approved=True, method="user", choice="approve_all"
            )
            return self._process_approve_all_choice(command, edited_command, matched_rule)
        else:
            get_debug_logger().log(
                "approval_result", "approval", approved=False, method="user", choice="deny"
            )
            return self._process_deny_choice(command, matched_rule)

    def skip_approval(self) -> bool:
        """Check if approval prompts should be skipped."""
        return self.auto_approve_remaining

    def reset_auto_approve(self) -> None:
        """Reset auto-approve setting."""
        self.auto_approve_remaining = False
