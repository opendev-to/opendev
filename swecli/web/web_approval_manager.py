"""Web-based approval manager for WebSocket clients."""

from __future__ import annotations

import asyncio
import threading
import uuid
from typing import Any, Optional, Union

from swecli.models.operation import Operation
from swecli.web.state import get_state
from swecli.web.logging_config import logger


class ApprovalResult:
    """Result of an approval request (simplified for web)."""

    def __init__(
        self,
        approved: bool,
        choice: str = "approve",
        edited_content: Optional[str] = None,
        apply_to_all: bool = False,
        cancelled: bool = False,
    ) -> None:
        self.approved = approved
        self.choice = choice
        self.edited_content = edited_content
        self.apply_to_all = apply_to_all
        self.cancelled = cancelled


# Safe commands that can be auto-approved in Semi-Auto mode
SAFE_COMMANDS = [
    "ls", "cat", "head", "tail", "grep", "find", "wc", "pwd",
    "echo", "which", "type", "file", "stat", "du", "df", "tree",
    "git status", "git log", "git diff", "git branch", "git show",
    "git remote", "git tag", "git stash list",
    "python --version", "python3 --version", "node --version",
    "npm --version", "cargo --version", "go version",
]


class WebApprovalManager:
    """Approval manager for web UI that uses WebSocket for approval requests."""

    def __init__(
        self,
        ws_manager: Any,
        loop: asyncio.AbstractEventLoop,
        session_id: Optional[str] = None,
    ):
        """Initialize web approval manager.

        Args:
            ws_manager: WebSocket manager for broadcasting
            loop: Event loop for async operations
            session_id: Session ID for scoping broadcasts
        """
        self.ws_manager = ws_manager
        self.loop = loop
        self.session_id = session_id
        self.state = get_state()

    def request_approval(
        self,
        operation: Operation,
        preview: str,
        *,
        command: Optional[str] = None,
        working_dir: Optional[str] = None,
        allow_edit: bool = True,
        timeout: Union[Any, None] = None,
        force_prompt: bool = False,
    ) -> ApprovalResult:
        """Request approval for an operation via WebSocket.

        This is called from a sync context (agent thread), so we need to
        schedule the async broadcast and wait for response.

        Args:
            operation: Operation to approve
            preview: Preview of the operation (for display)
            command: Command being executed (for bash operations)
            working_dir: Working directory context
            allow_edit: Whether to allow editing (not supported in web)
            timeout: Custom timeout (uses default 5 minutes if None)
            force_prompt: Force prompt even if auto-approve is enabled

        Returns:
            ApprovalResult with approval status
        """
        # Check autonomy level before prompting
        autonomy = self.state.get_autonomy_level()

        if autonomy == "Auto":
            return ApprovalResult(approved=True, choice="approve")

        if autonomy == "Semi-Auto" and command:
            cmd_lower = command.strip().lower()
            for safe_cmd in SAFE_COMMANDS:
                if cmd_lower == safe_cmd or cmd_lower.startswith(safe_cmd + " "):
                    return ApprovalResult(approved=True, choice="approve")

        approval_id = str(uuid.uuid4())
        done_event = threading.Event()

        # Create approval request with preview
        # Operation has: type (enum), target (str), parameters (dict)
        tool_name = operation.type.value  # e.g., "bash_execute", "file_write"
        description = f"{tool_name}: {operation.target}"

        approval_request = {
            "id": approval_id,
            "tool_name": tool_name,
            "arguments": operation.parameters,
            "description": description,
            "preview": preview[:500] if preview else "",  # Truncate long previews
            "session_id": self.session_id,
        }

        # Store pending approval in shared state (with event for wake-up)
        self.state.add_pending_approval(
            approval_id,
            tool_name,
            operation.parameters,
            session_id=self.session_id,
            event=done_event,
        )

        # Broadcast approval request via WebSocket
        logger.info(f"Requesting approval for {tool_name}: {approval_request}")
        future = asyncio.run_coroutine_threadsafe(
            self.ws_manager.broadcast({
                "type": "approval_required",
                "data": approval_request,
            }),
            self.loop
        )

        # Wait for broadcast to complete
        try:
            future.result(timeout=5)
            logger.info(f"Approval request broadcasted successfully: {approval_id}")
        except Exception as e:
            logger.error(f"Failed to broadcast approval request: {e}")
            self.state.clear_approval(approval_id)
            return ApprovalResult(approved=False, choice="deny", cancelled=True)

        # Wait for approval response using Event (no polling)
        wait_timeout = timeout if timeout else 300  # 5 minutes default
        logger.info(f"Waiting for approval response (timeout: {wait_timeout}s)...")

        if not done_event.wait(timeout=wait_timeout):
            # Timeout - default to deny
            logger.warning(f"Approval request {approval_id} timed out after {wait_timeout}s")
            self.state.clear_approval(approval_id)
            return ApprovalResult(approved=False, choice="deny", cancelled=True)

        # Event was set — read result
        approval = self.state.get_pending_approval(approval_id)
        approved = approval["approved"]
        auto_approve = approval.get("auto_approve", False)
        self.state.clear_approval(approval_id)
        choice = (
            "approve_all"
            if (approved and auto_approve)
            else ("approve" if approved else "deny")
        )
        logger.info(f"Approval resolved: {approval_id} - {'approved' if approved else 'denied'}")
        return ApprovalResult(approved=approved, choice=choice, apply_to_all=auto_approve)

    def reset_auto_approve(self) -> None:
        """Reset auto-approve state (for compatibility with ApprovalManager interface)."""
        # Web approval manager doesn't have auto-approve state
        pass

    def check_rules(self, operation: Operation) -> Optional[bool]:
        """Check auto-approval rules.

        For now, return None to always require explicit approval.
        In the future, this can check user-configured rules.

        Args:
            operation: Operation to check

        Returns:
            True to auto-approve, False to auto-deny, None to require user approval
        """
        # TODO: Implement rule checking based on user preferences
        # For now, always require approval
        return None
