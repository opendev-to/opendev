"""Shared state manager for web UI and terminal REPL."""

from __future__ import annotations

import queue as queue_mod
import threading
from typing import Any, Dict, List, Optional
from threading import Lock

from swecli.core.runtime import ConfigManager, ModeManager
from swecli.core.context_engineering.history import SessionManager, UndoManager
from swecli.core.runtime.approval import ApprovalManager
from swecli.models.message import ChatMessage


# Type imports
from typing import TYPE_CHECKING
if TYPE_CHECKING:
    from swecli.core.context_engineering.mcp.manager import MCPManager


class WebState:
    """Shared state between CLI and web UI.

    This class maintains a single source of truth for:
    - Current session
    - Configuration
    - Message history
    - Agent state

    Thread-safe for concurrent access from REPL and web server.
    """

    def __init__(
        self,
        config_manager: ConfigManager,
        session_manager: SessionManager,
        mode_manager: ModeManager,
        approval_manager: ApprovalManager,
        undo_manager: UndoManager,
        mcp_manager: Optional["MCPManager"] = None,
    ):
        self.config_manager = config_manager
        self.session_manager = session_manager
        self.mode_manager = mode_manager
        self.approval_manager = approval_manager
        self.undo_manager = undo_manager
        self.mcp_manager = mcp_manager

        # Thread safety
        self._lock = Lock()

        # Connected WebSocket clients
        self._ws_clients: List[Any] = []

        # Pending approval requests
        self._pending_approvals: Dict[str, Dict[str, Any]] = {}

        # Interrupt flag for stopping ongoing tasks
        self._interrupt_requested = False

        # Autonomy level for approval management
        self._autonomy_level: str = "Manual"

        # Pending ask-user requests
        self._pending_ask_users: Dict[str, Dict[str, Any]] = {}

        # Thinking level (matches TUI: Off, Low, Medium, High)
        self._thinking_level: str = "Medium"

        # Pending plan approval requests
        self._pending_plan_approvals: Dict[str, Dict[str, Any]] = {}

        # Running sessions: session_id -> "running"
        self._running_sessions: Dict[str, str] = {}

        # Live message injection queues: session_id -> Queue
        self._injection_queues: Dict[str, queue_mod.Queue[str]] = {}

    def add_ws_client(self, client: Any) -> None:
        """Add a WebSocket client."""
        with self._lock:
            if client not in self._ws_clients:
                self._ws_clients.append(client)

    def remove_ws_client(self, client: Any) -> None:
        """Remove a WebSocket client."""
        with self._lock:
            if client in self._ws_clients:
                self._ws_clients.remove(client)

    def get_ws_clients(self) -> List[Any]:
        """Get all connected WebSocket clients."""
        with self._lock:
            return self._ws_clients.copy()

    def get_messages(self) -> List[ChatMessage]:
        """Get current session messages."""
        session = self.session_manager.get_current_session()
        if session:
            return session.messages
        return []

    def add_message(self, message: ChatMessage) -> None:
        """Add a message to current session."""
        self.session_manager.add_message(message)

    def get_current_session_id(self) -> Optional[str]:
        """Get current session ID."""
        session = self.session_manager.get_current_session()
        return session.id if session else None

    def list_sessions(self) -> List[Dict[str, Any]]:
        """List all available sessions across all projects."""
        return [
            {
                "id": s.id,
                "working_dir": s.working_directory or "",
                "created_at": s.created_at.isoformat(),
                "updated_at": s.updated_at.isoformat(),
                "message_count": s.message_count,
                "total_tokens": s.total_tokens,
                "title": s.title,
            }
            for s in self.session_manager.list_all_sessions()
        ]

    def resume_session(self, session_id: str) -> bool:
        """Resume a specific session."""
        try:
            self.session_manager.load_session(session_id)
            return True
        except Exception:
            return False

    def add_pending_approval(
        self,
        approval_id: str,
        tool_name: str,
        arguments: Dict[str, Any],
        session_id: Optional[str] = None,
        event: Optional[threading.Event] = None,
    ) -> None:
        """Add a pending approval request."""
        with self._lock:
            self._pending_approvals[approval_id] = {
                "tool_name": tool_name,
                "arguments": arguments,
                "resolved": False,
                "approved": None,
                "session_id": session_id,
                "_event": event,
            }

    def resolve_approval(self, approval_id: str, approved: bool, auto_approve: bool = False) -> bool:
        """Resolve a pending approval request."""
        print(f"[State] resolve_approval called: id={approval_id}, approved={approved}")
        with self._lock:
            if approval_id in self._pending_approvals:
                print(f"[State] Found approval in pending list, marking as resolved")
                self._pending_approvals[approval_id]["resolved"] = True
                self._pending_approvals[approval_id]["approved"] = approved
                self._pending_approvals[approval_id]["auto_approve"] = auto_approve
                event = self._pending_approvals[approval_id].get("_event")
                if event:
                    event.set()
                return True
            print(f"[State] Approval {approval_id} NOT FOUND in pending list!")
            print(f"[State] Current pending approvals: {list(self._pending_approvals.keys())}")
            return False

    def get_pending_approval(self, approval_id: str) -> Optional[Dict[str, Any]]:
        """Get a pending approval request."""
        with self._lock:
            return self._pending_approvals.get(approval_id)

    def clear_approval(self, approval_id: str) -> None:
        """Clear a resolved approval."""
        with self._lock:
            self._pending_approvals.pop(approval_id, None)

    def request_interrupt(self) -> None:
        """Request interruption of ongoing task."""
        with self._lock:
            self._interrupt_requested = True

    def clear_interrupt(self) -> None:
        """Clear the interrupt flag."""
        with self._lock:
            self._interrupt_requested = False

    def is_interrupt_requested(self) -> bool:
        """Check if interrupt has been requested."""
        with self._lock:
            return self._interrupt_requested

    # --- Autonomy level ---

    def get_autonomy_level(self) -> str:
        """Get current autonomy level."""
        with self._lock:
            return self._autonomy_level

    def set_autonomy_level(self, level: str) -> None:
        """Set autonomy level."""
        with self._lock:
            self._autonomy_level = level

    # --- Thinking level ---

    def get_thinking_level(self) -> str:
        """Get current thinking level."""
        with self._lock:
            return self._thinking_level

    def set_thinking_level(self, level: str) -> None:
        """Set thinking level."""
        with self._lock:
            self._thinking_level = level

    # --- Running sessions ---

    def set_session_running(self, session_id: str) -> None:
        """Mark a session as having a running agent."""
        with self._lock:
            self._running_sessions[session_id] = "running"

    def set_session_idle(self, session_id: str) -> None:
        """Mark a session as idle (no running agent)."""
        with self._lock:
            self._running_sessions.pop(session_id, None)

    def is_session_running(self, session_id: str) -> bool:
        """Check if a session has a running agent."""
        with self._lock:
            return session_id in self._running_sessions

    # --- Injection queues ---

    def get_injection_queue(self, session_id: str) -> queue_mod.Queue[str]:
        """Get or create the injection queue for a session."""
        with self._lock:
            if session_id not in self._injection_queues:
                self._injection_queues[session_id] = queue_mod.Queue(maxsize=10)
            return self._injection_queues[session_id]

    def clear_injection_queue(self, session_id: str) -> None:
        """Remove the injection queue for a session."""
        with self._lock:
            self._injection_queues.pop(session_id, None)

    # --- Ask-user state ---

    def add_pending_ask_user(
        self,
        request_id: str,
        data: Dict[str, Any],
        session_id: Optional[str] = None,
        event: Optional[threading.Event] = None,
    ) -> None:
        """Add a pending ask-user request."""
        with self._lock:
            self._pending_ask_users[request_id] = {
                "data": data,
                "resolved": False,
                "answers": None,
                "cancelled": False,
                "session_id": session_id,
                "_event": event,
            }

    def resolve_ask_user(
        self, request_id: str, answers: Optional[Dict], cancelled: bool = False
    ) -> bool:
        """Resolve a pending ask-user request."""
        with self._lock:
            if request_id in self._pending_ask_users:
                self._pending_ask_users[request_id]["resolved"] = True
                self._pending_ask_users[request_id]["answers"] = answers
                self._pending_ask_users[request_id]["cancelled"] = cancelled
                event = self._pending_ask_users[request_id].get("_event")
                if event:
                    event.set()
                return True
            return False

    def get_pending_ask_user(self, request_id: str) -> Optional[Dict[str, Any]]:
        """Get a pending ask-user request."""
        with self._lock:
            return self._pending_ask_users.get(request_id)

    def clear_ask_user(self, request_id: str) -> None:
        """Clear a resolved ask-user request."""
        with self._lock:
            self._pending_ask_users.pop(request_id, None)

    # --- Plan approval state ---

    def add_pending_plan_approval(
        self,
        request_id: str,
        data: Dict[str, Any],
        session_id: Optional[str] = None,
        event: Optional[threading.Event] = None,
    ) -> None:
        """Add a pending plan approval request."""
        with self._lock:
            self._pending_plan_approvals[request_id] = {
                "data": data,
                "resolved": False,
                "action": None,
                "feedback": "",
                "session_id": session_id,
                "_event": event,
            }

    def resolve_plan_approval(
        self, request_id: str, action: str, feedback: str = ""
    ) -> bool:
        """Resolve a pending plan approval request."""
        with self._lock:
            if request_id in self._pending_plan_approvals:
                self._pending_plan_approvals[request_id]["resolved"] = True
                self._pending_plan_approvals[request_id]["action"] = action
                self._pending_plan_approvals[request_id]["feedback"] = feedback
                event = self._pending_plan_approvals[request_id].get("_event")
                if event:
                    event.set()
                return True
            return False

    def get_pending_plan_approval(self, request_id: str) -> Optional[Dict[str, Any]]:
        """Get a pending plan approval request."""
        with self._lock:
            return self._pending_plan_approvals.get(request_id)

    def clear_plan_approval(self, request_id: str) -> None:
        """Clear a resolved plan approval request."""
        with self._lock:
            self._pending_plan_approvals.pop(request_id, None)

    def get_git_branch(self) -> Optional[str]:
        """Get current git branch for the working directory."""
        import subprocess
        try:
            session = self.session_manager.get_current_session()
            cwd = session.working_directory if session else None
            result = subprocess.run(
                ["git", "rev-parse", "--abbrev-ref", "HEAD"],
                capture_output=True, text=True, cwd=cwd, timeout=3,
            )
            if result.returncode == 0:
                return result.stdout.strip()
        except Exception:
            pass
        return None


# Global state instance (will be initialized when web server starts)
_state: Optional[WebState] = None


def init_state(
    config_manager: ConfigManager,
    session_manager: SessionManager,
    mode_manager: ModeManager,
    approval_manager: ApprovalManager,
    undo_manager: UndoManager,
    mcp_manager: Optional["MCPManager"] = None,
) -> WebState:
    """Initialize the global state instance."""
    global _state
    _state = WebState(
        config_manager,
        session_manager,
        mode_manager,
        approval_manager,
        undo_manager,
        mcp_manager,
    )
    return _state


def get_state() -> WebState:
    """Get the global state instance."""
    if _state is None:
        # Auto-initialize with default managers for standalone server
        from pathlib import Path
        from swecli.core.runtime import ConfigManager, ModeManager
        from swecli.core.context_engineering.history import SessionManager, UndoManager
        from swecli.core.runtime.approval import ApprovalManager
        from swecli.core.context_engineering.mcp.manager import MCPManager
        from swecli.core.paths import get_paths
        from rich.console import Console

        console = Console()
        working_dir = Path.cwd()
        paths = get_paths(working_dir)

        config_manager = ConfigManager(working_dir)
        session_manager = SessionManager(working_dir=working_dir)
        mode_manager = ModeManager()
        approval_manager = ApprovalManager(console)
        undo_manager = UndoManager(50)

        # Initialize MCP manager
        mcp_manager = MCPManager(working_dir)

        # Don't create session on startup - let user create via UI

        return init_state(
            config_manager,
            session_manager,
            mode_manager,
            approval_manager,
            undo_manager,
            mcp_manager,
        )
    return _state


async def broadcast_to_all_clients(message: Dict[str, Any]) -> None:
    """Broadcast a message to all connected WebSocket clients.

    Args:
        message: Message to broadcast (will be JSON-serialized)
    """
    state = get_state()
    clients = state.get_ws_clients()

    import json

    for client in clients:
        try:
            await client.send_text(json.dumps(message))
        except Exception:
            # Client disconnected, will be cleaned up by WebSocket handler
            pass
