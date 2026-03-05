"""Session manager interface for persistence orchestration."""

from __future__ import annotations

from pathlib import Path
from typing import Union, Protocol, Sequence

from swecli.models.message import ChatMessage
from swecli.models.session import Session, SessionMetadata


class SessionManagerInterface(Protocol):
    """Operations that a session manager implementation must support."""

    session_dir: Path

    def create_session(self, working_directory: Union[str, None] = None) -> Session:
        """Create a new session scoped to the working directory."""

    def load_session(self, session_id: str) -> Session:
        """Load a session by identifier, raising if it is missing."""

    def save_session(self, session: Union[Session, None] = None) -> None:
        """Persist the provided (or current) session to disk."""

    def add_message(self, message: ChatMessage, auto_save_interval: int = 5) -> None:
        """Append a message and trigger auto-save when required."""

    def list_sessions(self) -> Sequence[SessionMetadata]:
        """Return metadata for all stored sessions."""

    def delete_session(self, session_id: str) -> None:
        """Remove a session from storage."""

    def get_current_session(self) -> Union[Session, None]:
        """Return the currently active session if any."""
