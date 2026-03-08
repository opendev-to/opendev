"""Main SessionManager class."""

import json
from pathlib import Path
from typing import Optional

from opendev.models.session import Session

from opendev.core.context_engineering.history.session_manager.index import IndexMixin
from opendev.core.context_engineering.history.session_manager.persistence import PersistenceMixin
from opendev.core.context_engineering.history.session_manager.listing import ListingMixin


class SessionManager(IndexMixin, PersistenceMixin, ListingMixin):
    """Manages session persistence and retrieval.

    Sessions are stored in project-scoped directories under
    ``~/.opendev/projects/{encoded-path}/``.

    A lightweight ``sessions-index.json`` file caches session metadata so that
    ``list_sessions()`` is O(1) reads instead of O(N) full-file parses. The
    index is self-healing: if it is missing or corrupted, it is transparently
    rebuilt from the individual session ``.json`` files.
    """

    def __init__(
        self,
        *,
        session_dir: Optional[Path] = None,
        working_dir: Optional[Path] = None,
    ):
        """Initialize session manager.

        Args:
            session_dir: Explicit directory override (tests, ``OPENDEV_SESSION_DIR``).
            working_dir: Working directory used to compute the project-scoped
                session directory via :func:`paths.project_sessions_dir`.

        If neither argument is given, falls back to
        ``~/.opendev/projects/-unknown-/``.
        """
        self._explicit_session_dir = session_dir is not None
        if session_dir is not None:
            self.session_dir = Path(session_dir).expanduser()
        elif working_dir is not None:
            from opendev.core.paths import get_paths

            paths = get_paths()
            self.session_dir = paths.project_sessions_dir(working_dir)
        else:
            from opendev.core.paths import get_paths, FALLBACK_PROJECT_DIR_NAME

            paths = get_paths()
            self.session_dir = paths.global_projects_dir / FALLBACK_PROJECT_DIR_NAME

        self.session_dir.mkdir(parents=True, exist_ok=True)
        self.current_session: Optional[Session] = None
        self.turn_count = 0

    def get_current_session(self) -> Optional[Session]:
        """Get the current active session."""
        return self.current_session

    @staticmethod
    def generate_title(messages: list[dict]) -> str:
        """Generate a short title from the first user message.

        Simple heuristic: extract the first sentence, truncate to 50 chars.
        No LLM call required.

        Args:
            messages: List of message dicts with 'role' and 'content' keys

        Returns:
            A concise title string (max 50 chars)
        """
        for msg in messages:
            if msg.get("role") == "user":
                content = msg.get("content", "").strip()
                if not content:
                    continue
                # Take first sentence (or first line)
                for sep in (".", "\n", "?", "!"):
                    idx = content.find(sep)
                    if 0 < idx < 80:
                        content = content[:idx]
                        break
                title = content[:50].strip()
                return title if title else "Untitled"
        return "Untitled"

    def fork_session(self, message_index: Optional[int] = None) -> Optional[Session]:
        """Fork the current session, cloning messages up to a given point.

        Creates a new child session containing a copy of the messages from the
        current session up to ``message_index``. The new session's ``parent_id``
        is set to the current session's ID and metadata records the fork origin.

        Args:
            message_index: Clone messages up to this index (0-based, inclusive).
                          If None, clones all messages.

        Returns:
            The new forked Session, or None if there is no active session.
        """
        current = self.get_current_session()
        if current is None:
            return None

        from uuid import uuid4
        from datetime import datetime

        # Determine which messages to clone
        if message_index is not None:
            messages = list(current.messages[: message_index + 1])
        else:
            messages = list(current.messages)

        # Build the new session
        new_session = Session(
            id=uuid4().hex[:12],
            created_at=datetime.now(),
            updated_at=datetime.now(),
            messages=messages,
            context_files=list(current.context_files),
            working_directory=current.working_directory,
            metadata={
                **current.metadata,
                "forked_from": current.id,
                "forked_at_message": message_index,
            },
            parent_id=current.id,
            channel=current.channel,
            channel_user_id=current.channel_user_id,
            chat_type=current.chat_type,
            thread_id=current.thread_id,
            delivery_context=dict(current.delivery_context),
            owner_id=current.owner_id,
        )

        # Generate a title for the fork
        parent_title = current.metadata.get("title", f"Session {current.id[:8]}")
        new_session.metadata["title"] = f"Fork of {parent_title}"
        new_session.slug = new_session.generate_slug()

        # Persist and switch to the new session
        self.save_session(new_session)
        self.current_session = new_session
        self.turn_count = len(new_session.messages)

        return new_session

    def set_title(self, session_id: str, title: str) -> None:
        """Set the title for a session.

        Args:
            session_id: Session ID to update
            title: Title to set (max 50 chars)
        """
        title = title[:50]

        # Update in-memory if it's the current session
        if self.current_session and self.current_session.id == session_id:
            self.current_session.metadata["title"] = title
            self.current_session.slug = self.current_session.generate_slug(title)
            self.save_session()
            return

        # Otherwise load, update, save on disk
        session_file = self.session_dir / f"{session_id}.json"
        if not session_file.exists():
            return

        with open(session_file) as f:
            data = json.load(f)

        if "metadata" not in data:
            data["metadata"] = {}
        data["metadata"]["title"] = title

        # Generate slug from the new title
        temp_session = Session(id=data.get("id", session_id), metadata=data.get("metadata", {}))
        data["slug"] = temp_session.generate_slug(title)

        with open(session_file, "w") as f:
            json.dump(data, f, indent=2, default=str)

        # Update the index for the on-disk-only path
        try:
            session = self._load_from_file(session_file)
            self._update_index_entry(session)
        except Exception:
            pass
