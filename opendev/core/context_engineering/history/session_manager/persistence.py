"""Session creation, loading, and saving."""

import json
from datetime import datetime
from pathlib import Path
from typing import Optional

from opendev.core.context_engineering.history.file_locks import exclusive_session_lock
from opendev.models.message import ChatMessage
from opendev.models.session import Session


class PersistenceMixin:
    """Mixin for session CRUD operations."""

    def create_session(
        self,
        working_directory: Optional[str] = None,
        channel: str = "cli",
        channel_user_id: str = "",
        chat_type: str = "direct",
        thread_id: Optional[str] = None,
        delivery_context: Optional[dict] = None,
        workspace_confirmed: bool = True,
        owner_id: Optional[str] = None,
    ) -> Session:
        """Create a new session.

        Args:
            working_directory: Working directory for the session
            channel: Channel name ("cli", "web", "telegram", "whatsapp")
            channel_user_id: Channel-specific user identifier
            chat_type: Chat type ("direct", "group")
            thread_id: Thread ID for threaded channels
            delivery_context: Where to send responses (channel-specific)
            workspace_confirmed: Whether workspace has been selected (False for channels)

        Returns:
            New session instance
        """
        session = Session(
            working_directory=working_directory,
            channel=channel,
            channel_user_id=channel_user_id,
            chat_type=chat_type,
            thread_id=thread_id,
            delivery_context=delivery_context or {},
            last_activity=datetime.now(),
            workspace_confirmed=workspace_confirmed,
            owner_id=owner_id,
        )
        self.current_session = session
        self.turn_count = 0
        return session

    @staticmethod
    def _load_from_file(path: Path) -> Session:
        """Load a session from a JSON or JSONL file.

        Tries JSONL format first (for multi-channel), falls back to legacy JSON.

        Args:
            path: Path to the session file (with .json or .jsonl extension).

        Returns:
            Loaded Session instance.

        Raises:
            FileNotFoundError: If the file doesn't exist.
        """
        # Try JSONL format first
        jsonl_path = path.with_suffix(".jsonl")
        if jsonl_path.exists():
            # Load session metadata from JSON (if exists) or reconstruct
            json_path = path.with_suffix(".json")
            if json_path.exists():
                with open(json_path) as f:
                    data = json.load(f)
                # Load messages from JSONL transcript
                messages = []
                with open(jsonl_path, encoding="utf-8") as f:
                    for line in f:
                        line = line.strip()
                        if line:
                            try:
                                msg_data = json.loads(line)
                                messages.append(ChatMessage(**msg_data))
                            except Exception:
                                continue
                data["messages"] = messages
                return Session(**data)

        # Fall back to legacy JSON format
        if not path.exists():
            raise FileNotFoundError(f"Session file not found: {path}")

        with open(path) as f:
            data = json.load(f)

        return Session(**data)

    def load_session(self, session_id: str, owner_id: Optional[str] = None) -> Session:
        """Load a session from disk, enforcing ownership when provided."""
        session_file = self.session_dir / f"{session_id}.json"
        if session_file.exists():
            session = self._load_from_file(session_file)
            if owner_id and session.owner_id != owner_id:
                raise FileNotFoundError(f"Session {session_id} not found")
            self.current_session = session
            self.turn_count = len(session.messages)
            return session

        # Fall back to searching all project directories
        from opendev.core.paths import get_paths

        paths = get_paths()
        projects_dir = paths.global_projects_dir
        if projects_dir.exists():
            for project_dir in projects_dir.iterdir():
                if not project_dir.is_dir():
                    continue
                candidate = project_dir / f"{session_id}.json"
                if candidate.exists():
                    session = self._load_from_file(candidate)
                    if owner_id and session.owner_id != owner_id:
                        continue
                    self.current_session = session
                    self.turn_count = len(session.messages)
                    return session

        raise FileNotFoundError(f"Session {session_id} not found")

    def _resolve_session_dir(self, session: Session) -> Path:
        """Get the correct project-scoped storage directory for a session."""
        if not getattr(self, "_explicit_session_dir", False) and session.working_directory:
            from opendev.core.paths import get_paths

            paths = get_paths()
            target = paths.project_sessions_dir(Path(session.working_directory))
            target.mkdir(parents=True, exist_ok=True)
            return target
        return self.session_dir

    def save_session(
        self,
        session: Optional[Session] = None,
        use_jsonl: bool = True,
        force: bool = False,
    ) -> None:
        """Save session to disk.

        Only saves sessions that have at least one message to avoid
        cluttering the session list with empty test sessions.

        For multi-channel mode, uses JSONL format:
        - Session metadata saved to {session_id}.json (without messages)
        - Messages saved to {session_id}.jsonl (append-only, one per line)

        Args:
            session: Session to save (defaults to current session)
            use_jsonl: Use JSONL format for messages (default True for multi-channel)
            force: If True, save even if the session has no messages
        """
        session = session or self.current_session
        if not session:
            return

        # Only save sessions with at least one message (unless forced)
        if not force and len(session.messages) == 0:
            return

        target_dir = self._resolve_session_dir(session)
        session_file = target_dir / f"{session.id}.json"
        jsonl_file = target_dir / f"{session.id}.jsonl"

        # Auto-generate title before writing (single write)
        if not session.metadata.get("title"):
            msg_dicts = [
                {"role": m.role.value, "content": m.content}
                for m in session.messages
                if not m.metadata.get("display_hidden")
            ]
            title = self.generate_title(msg_dicts)
            if title != "Untitled":
                session.metadata["title"] = title
                session.slug = session.generate_slug(title)

        if use_jsonl:
            # JSONL mode: separate metadata and transcript
            # 1. Save metadata (without messages) to .json
            session_data = session.model_dump()
            messages = session_data.pop("messages", [])

            with exclusive_session_lock(session_file, timeout=10.0):
                with open(session_file, "w") as f:
                    json.dump(session_data, f, indent=2, default=str)

            # 2. Save messages to .jsonl (rewrite entire transcript)
            # For incremental appends, use append_message_to_transcript() instead
            with exclusive_session_lock(jsonl_file, timeout=10.0):
                with open(jsonl_file, "w", encoding="utf-8") as f:
                    for msg in messages:
                        json.dump(msg, f, default=str)
                        f.write("\n")
        else:
            # Legacy mode: full session in single .json file
            with exclusive_session_lock(session_file, timeout=10.0):
                with open(session_file, "w") as f:
                    json.dump(session.model_dump(), f, indent=2, default=str)

        # Update the sessions index
        if target_dir != self.session_dir:
            self._update_index_entry_in_dir(target_dir, session)
        else:
            self._update_index_entry(session)

    def add_message(self, message: ChatMessage, auto_save_interval: int = 5) -> None:
        """Add a message to the current session and auto-save if needed.

        Args:
            message: Message to add
            auto_save_interval: Save every N turns (<= 0 disables auto-save)
        """
        if not self.current_session:
            raise ValueError("No active session")

        self.current_session.add_message(message)
        self.turn_count += 1

        # Auto-save (only when interval is positive)
        if auto_save_interval and auto_save_interval > 0:
            if self.turn_count % auto_save_interval == 0:
                self.save_session()

    def append_message_to_transcript(self, session_id: str, message: ChatMessage) -> None:
        """Append a single message to the JSONL transcript (cross-process safe).

        This is the preferred method for adding messages in multi-channel mode,
        as it allows concurrent appends without reading/rewriting the entire file.

        Args:
            session_id: Session ID
            message: Message to append

        Example:
            manager.append_message_to_transcript(session.id, ChatMessage(...))
        """
        jsonl_file = self.session_dir / f"{session_id}.jsonl"

        # Acquire exclusive lock for cross-process safety
        with exclusive_session_lock(jsonl_file, timeout=10.0):
            with open(jsonl_file, "a", encoding="utf-8") as f:
                # Serialize message to JSON (one line)
                json.dump(message.model_dump(), f, default=str)
                f.write("\n")

    def load_transcript(self, session_id: str) -> list[ChatMessage]:
        """Load all messages from JSONL transcript.

        Args:
            session_id: Session ID

        Returns:
            List of ChatMessage objects from the transcript

        Raises:
            FileNotFoundError: If transcript file doesn't exist
        """
        jsonl_file = self.session_dir / f"{session_id}.jsonl"

        if not jsonl_file.exists():
            # Try legacy .json format
            json_file = self.session_dir / f"{session_id}.json"
            if json_file.exists():
                # Load from legacy format and return messages
                session = self._load_from_file(json_file)
                return session.messages
            raise FileNotFoundError(f"Transcript not found for session {session_id}")

        messages = []
        with open(jsonl_file, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line:
                    try:
                        msg_data = json.loads(line)
                        messages.append(ChatMessage(**msg_data))
                    except Exception:
                        # Skip corrupted lines
                        continue

        return messages
