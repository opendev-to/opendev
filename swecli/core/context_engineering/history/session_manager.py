"""Session persistence and management."""

import json
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Optional, Union

from swecli.core.context_engineering.history.file_locks import exclusive_session_lock
from swecli.models.message import ChatMessage
from swecli.models.session import Session, SessionMetadata

_INDEX_VERSION = 1


class SessionManager:
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
        if session_dir is not None:
            self.session_dir = Path(session_dir).expanduser()
        elif working_dir is not None:
            from swecli.core.paths import get_paths

            paths = get_paths()
            self.session_dir = paths.project_sessions_dir(working_dir)
        else:
            from swecli.core.paths import get_paths, FALLBACK_PROJECT_DIR_NAME

            paths = get_paths()
            self.session_dir = paths.global_projects_dir / FALLBACK_PROJECT_DIR_NAME

        self.session_dir.mkdir(parents=True, exist_ok=True)
        self.current_session: Optional[Session] = None
        self.turn_count = 0

    # ========================================================================
    # Index helpers
    # ========================================================================

    @property
    def _index_path(self) -> Path:
        """Path to the sessions index file."""
        from swecli.core.paths import SESSIONS_INDEX_FILE_NAME

        return self.session_dir / SESSIONS_INDEX_FILE_NAME

    def _read_index(self) -> Optional[dict]:
        """Read the sessions index file.

        Returns:
            Parsed index dict if valid, ``None`` if missing/corrupted/wrong version.
        """
        try:
            if not self._index_path.exists():
                return None
            with open(self._index_path) as f:
                data = json.load(f)
            if not isinstance(data, dict) or data.get("version") != _INDEX_VERSION:
                return None
            if not isinstance(data.get("entries"), list):
                return None
            return data
        except (json.JSONDecodeError, OSError):
            return None

    def _write_index(self, entries: list[dict]) -> None:
        """Atomically write the sessions index file.

        Writes to a temporary file first, then renames to prevent torn reads.
        Uses exclusive lock for cross-process safety when multiple channels
        update the index concurrently.
        """
        data = {"version": _INDEX_VERSION, "entries": entries}

        # Acquire exclusive lock on index file for cross-process safety
        with exclusive_session_lock(self._index_path, timeout=10.0):
            # Write to temp file in the same directory, then rename (atomic on POSIX)
            fd, tmp_path = tempfile.mkstemp(
                dir=self.session_dir, suffix=".tmp", prefix=".sessions-index-"
            )
            try:
                with open(fd, "w") as f:
                    json.dump(data, f, indent=2, default=str)
                Path(tmp_path).replace(self._index_path)
            except Exception:
                # Clean up temp file on failure
                try:
                    Path(tmp_path).unlink(missing_ok=True)
                except Exception:
                    pass
                raise

    @staticmethod
    def _session_to_index_entry(session: Session) -> dict:
        """Convert a Session to a camelCase index entry dict."""
        return {
            "sessionId": session.id,
            "created": session.created_at.isoformat(),
            "modified": session.updated_at.isoformat(),
            "messageCount": len(session.messages),
            "totalTokens": session.total_tokens(),
            "title": session.metadata.get("title"),
            "summary": session.metadata.get("summary"),
            "tags": session.metadata.get("tags", []),
            "workingDirectory": session.working_directory,
            "channel": session.channel,
            "channelUserId": session.channel_user_id,
            "threadId": session.thread_id,
        }

    @staticmethod
    def _metadata_from_index_entry(entry: dict) -> SessionMetadata:
        """Convert a camelCase index entry dict to a SessionMetadata."""
        return SessionMetadata(
            id=entry["sessionId"],
            created_at=datetime.fromisoformat(entry["created"]),
            updated_at=datetime.fromisoformat(entry["modified"]),
            message_count=entry.get("messageCount", 0),
            total_tokens=entry.get("totalTokens", 0),
            title=entry.get("title"),
            summary=entry.get("summary"),
            tags=entry.get("tags", []),
            working_directory=entry.get("workingDirectory"),
            channel=entry.get("channel", "cli"),
            channel_user_id=entry.get("channelUserId", ""),
            thread_id=entry.get("threadId"),
        )

    def _update_index_entry(self, session: Session) -> None:
        """Upsert a single session entry in the index."""
        index = self._read_index()
        if index is None:
            # Index missing/corrupted — rebuild it entirely
            self.rebuild_index()
            index = self._read_index()
            if index is None:
                return  # Rebuild itself failed — nothing we can do

        new_entry = self._session_to_index_entry(session)
        entries = index["entries"]

        # Replace existing entry or append
        for i, entry in enumerate(entries):
            if entry.get("sessionId") == session.id:
                entries[i] = new_entry
                self._write_index(entries)
                return

        entries.append(new_entry)
        self._write_index(entries)

    def _remove_index_entry(self, session_id: str) -> None:
        """Remove a single session entry from the index."""
        index = self._read_index()
        if index is None:
            return  # Nothing to remove from

        entries = [e for e in index["entries"] if e.get("sessionId") != session_id]
        self._write_index(entries)

    def migrate_json_to_jsonl(self) -> int:
        """Migrate existing .json sessions to .jsonl format (one-time migration).

        This converts legacy full-JSON sessions to the new format:
        - Session metadata → {session_id}.json (without messages)
        - Messages → {session_id}.jsonl (one per line)

        The original .json files are renamed to .json.bak for safety.

        Returns:
            Number of sessions migrated
        """
        from swecli.core.paths import SESSIONS_INDEX_FILE_NAME

        migrated_count = 0

        for json_file in self.session_dir.glob("*.json"):
            # Skip index and backup files
            if json_file.name == SESSIONS_INDEX_FILE_NAME or json_file.suffix == ".bak":
                continue

            jsonl_file = json_file.with_suffix(".jsonl")

            # Skip if already migrated (JSONL file exists)
            if jsonl_file.exists():
                continue

            try:
                # Load legacy session
                with open(json_file) as f:
                    data = json.load(f)

                session_id = data.get("id")
                if not session_id:
                    continue

                messages = data.get("messages", [])

                # Create JSONL transcript
                with open(jsonl_file, "w", encoding="utf-8") as f:
                    for msg in messages:
                        json.dump(msg, f, default=str)
                        f.write("\n")

                # Update JSON file (remove messages)
                data_without_messages = data.copy()
                data_without_messages["messages"] = []

                with open(json_file, "w") as f:
                    json.dump(data_without_messages, f, indent=2, default=str)

                # Backup original file
                backup_file = json_file.with_suffix(".json.bak")
                if not backup_file.exists():
                    # Restore original for backup
                    with open(backup_file, "w") as f:
                        json.dump(data, f, indent=2, default=str)

                migrated_count += 1

            except Exception as e:
                # Log error but continue with other files
                import logging

                logger = logging.getLogger(__name__)
                logger.warning(f"Failed to migrate {json_file}: {e}")
                continue

        return migrated_count

    def rebuild_index(self) -> list[SessionMetadata]:
        """Rebuild the index from individual session files.

        This is the self-healing path: called when the index is missing or
        corrupted. It globs all ``.json`` files (excluding the index itself),
        loads each session (from .json + .jsonl if available), and recreates the index.

        Returns:
            List of ``SessionMetadata`` for all valid, non-empty sessions.
        """
        from swecli.core.paths import SESSIONS_INDEX_FILE_NAME

        entries: list[dict] = []
        metadata_list: list[SessionMetadata] = []
        processed_ids: set[str] = set()

        # Process all .json files (both legacy and JSONL-mode metadata)
        for session_file in self.session_dir.glob("*.json"):
            # Skip the index file itself
            if session_file.name == SESSIONS_INDEX_FILE_NAME:
                continue

            # Extract session ID from filename
            session_id = session_file.stem

            # Skip if already processed
            if session_id in processed_ids:
                continue
            processed_ids.add(session_id)

            try:
                session = self._load_from_file(session_file)

                # Skip empty sessions
                if len(session.messages) == 0:
                    try:
                        session_file.unlink()
                        # Also remove JSONL file if it exists
                        jsonl_file = session_file.with_suffix(".jsonl")
                        if jsonl_file.exists():
                            jsonl_file.unlink()
                    except Exception:
                        pass
                    continue

                entry = self._session_to_index_entry(session)
                entries.append(entry)
                metadata_list.append(self._metadata_from_index_entry(entry))
            except Exception:
                continue  # Skip corrupted files

        self._write_index(entries)
        return sorted(metadata_list, key=lambda s: s.updated_at, reverse=True)

    # ========================================================================
    # Core session operations
    # ========================================================================

    def create_session(
        self,
        working_directory: Optional[str] = None,
        channel: str = "cli",
        channel_user_id: str = "",
        chat_type: str = "direct",
        thread_id: Optional[str] = None,
        delivery_context: Optional[dict] = None,
        workspace_confirmed: bool = True,
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
        )
        self.current_session = session
        self.turn_count = 0
        return session

    def find_session_by_channel_user(
        self,
        channel: str,
        user_id: str,
        thread_id: Optional[str] = None,
    ) -> Optional[SessionMetadata]:
        """Find active session for a channel+user combination.

        Used by multi-channel routers to resolve incoming messages to existing sessions.

        Args:
            channel: Channel name ("telegram", "whatsapp", "web", "cli")
            user_id: Channel-specific user identifier
            thread_id: Optional thread ID for threaded channels

        Returns:
            SessionMetadata if found, None otherwise
        """
        for session in self.list_sessions():
            if (
                session.channel == channel
                and session.channel_user_id == user_id
                and (thread_id is None or session.thread_id == thread_id)
            ):
                return session
        return None

    def list_user_workspaces(self) -> list[str]:
        """List all workspaces that have OpenDev sessions.

        Used by channel adapters to prompt users for workspace selection.

        Returns:
            List of workspace directory paths that have sessions
        """
        from swecli.core.paths import get_paths

        paths = get_paths()
        workspaces: list[str] = []

        if not paths.global_projects_dir.exists():
            return []

        # Scan all project directories
        for project_dir in paths.global_projects_dir.iterdir():
            if not project_dir.is_dir():
                continue

            # Skip the unknown/fallback directory
            from swecli.core.paths import FALLBACK_PROJECT_DIR_NAME

            if project_dir.name == FALLBACK_PROJECT_DIR_NAME:
                continue

            # Check if this project has any sessions
            session_files = list(project_dir.glob("*.json"))
            if session_files:
                # Decode project directory name back to original path
                # (For now, just use the directory name as-is)
                # TODO: Implement proper decoding of URL-encoded paths
                workspaces.append(project_dir.name)

        return sorted(workspaces)

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

    def load_session(self, session_id: str) -> Session:
        """Load a session from disk.

        Searches the local project directory first, then falls back to
        scanning all project directories (for ``--resume`` across projects).

        Args:
            session_id: Session ID to load

        Returns:
            Loaded session

        Raises:
            FileNotFoundError: If session file doesn't exist
        """
        # Try local project dir first
        session_file = self.session_dir / f"{session_id}.json"
        if session_file.exists():
            session = self._load_from_file(session_file)
            self.current_session = session
            self.turn_count = len(session.messages)
            return session

        # Fall back to searching all project directories
        from swecli.core.paths import get_paths

        paths = get_paths()
        projects_dir = paths.global_projects_dir
        if projects_dir.exists():
            for project_dir in projects_dir.iterdir():
                if not project_dir.is_dir():
                    continue
                candidate = project_dir / f"{session_id}.json"
                if candidate.exists():
                    session = self._load_from_file(candidate)
                    self.current_session = session
                    self.turn_count = len(session.messages)
                    return session

        raise FileNotFoundError(f"Session {session_id} not found")

    def save_session(self, session: Optional[Session] = None, use_jsonl: bool = True) -> None:
        """Save session to disk.

        Only saves sessions that have at least one message to avoid
        cluttering the session list with empty test sessions.

        For multi-channel mode, uses JSONL format:
        - Session metadata saved to {session_id}.json (without messages)
        - Messages saved to {session_id}.jsonl (append-only, one per line)

        Args:
            session: Session to save (defaults to current session)
            use_jsonl: Use JSONL format for messages (default True for multi-channel)
        """
        session = session or self.current_session
        if not session:
            return

        # Only save sessions with at least one message
        if len(session.messages) == 0:
            return

        session_file = self.session_dir / f"{session.id}.json"
        jsonl_file = self.session_dir / f"{session.id}.jsonl"

        # Auto-generate title before writing (single write)
        if not session.metadata.get("title"):
            msg_dicts = [
                {"role": m.role.value, "content": m.content} for m in session.messages
            ]
            title = self.generate_title(msg_dicts)
            if title != "Untitled":
                session.metadata["title"] = title

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

    def list_sessions(self) -> list[SessionMetadata]:
        """List all saved sessions in this project directory.

        Reads from the sessions index for O(1) performance. Falls back to
        a full rebuild if the index is missing or corrupted.

        Returns:
            List of session metadata, sorted by update time (newest first)
        """
        index = self._read_index()
        if index is not None:
            sessions = [self._metadata_from_index_entry(e) for e in index["entries"]]
            return sorted(sessions, key=lambda s: s.updated_at, reverse=True)

        # Index missing or corrupted — rebuild (returns sorted)
        return self.rebuild_index()

    def list_all_sessions(self) -> list[SessionMetadata]:
        """List sessions from ALL project directories.

        Scans every subdirectory under ``~/.opendev/projects/`` and merges
        their session indexes into a single list. Useful for the web UI
        sidebar which needs to display sessions across all workspaces.

        Returns:
            List of session metadata from all projects, sorted by update time
            (newest first).
        """
        from swecli.core.paths import get_paths, SESSIONS_INDEX_FILE_NAME

        paths = get_paths()
        projects_dir = paths.global_projects_dir
        if not projects_dir.exists():
            return []

        all_sessions: list[SessionMetadata] = []
        seen_ids: set[str] = set()

        for project_dir in projects_dir.iterdir():
            if not project_dir.is_dir():
                continue

            index_path = project_dir / SESSIONS_INDEX_FILE_NAME
            try:
                if index_path.exists():
                    with open(index_path) as f:
                        data = json.load(f)
                    if (
                        isinstance(data, dict)
                        and data.get("version") == _INDEX_VERSION
                        and isinstance(data.get("entries"), list)
                    ):
                        for entry in data["entries"]:
                            sid = entry.get("sessionId")
                            if sid and sid not in seen_ids:
                                seen_ids.add(sid)
                                all_sessions.append(
                                    self._metadata_from_index_entry(entry)
                                )
                        continue

                # No valid index — rebuild from JSON files in this dir
                for session_file in project_dir.glob("*.json"):
                    if session_file.name == SESSIONS_INDEX_FILE_NAME:
                        continue
                    try:
                        session = self._load_from_file(session_file)
                        if len(session.messages) == 0:
                            continue
                        sid = session.id
                        if sid not in seen_ids:
                            seen_ids.add(sid)
                            entry = self._session_to_index_entry(session)
                            all_sessions.append(
                                self._metadata_from_index_entry(entry)
                            )
                    except Exception:
                        continue
            except Exception:
                continue

        return sorted(all_sessions, key=lambda s: s.updated_at, reverse=True)

    def find_latest_session(
        self, working_directory: Union[Path, str, None] = None
    ) -> Optional[SessionMetadata]:
        """Find the most recently updated session.

        Since sessions are now project-scoped, this simply returns the newest
        session in the directory. The *working_directory* parameter is accepted
        for backward-compatibility but is no longer used for filtering.
        """
        sessions = self.list_sessions()
        return sessions[0] if sessions else None

    def load_latest_session(
        self, working_directory: Union[Path, str, None] = None
    ) -> Optional[Session]:
        """Load the most recent session."""
        metadata = self.find_latest_session(working_directory)
        if not metadata:
            return None
        return self.load_session(metadata.id)

    def delete_session(self, session_id: str) -> None:
        """Delete a session and its associated debug log.

        Also removes the session from the sessions index.

        Args:
            session_id: Session ID to delete
        """
        session_file = self.session_dir / f"{session_id}.json"
        if session_file.exists():
            session_file.unlink()

        # Also remove the debug log if present
        debug_file = self.session_dir / f"{session_id}.debug"
        if debug_file.exists():
            debug_file.unlink()

        # Remove from sessions index
        self._remove_index_entry(session_id)

    def get_session_by_id(self, session_id: str) -> Session:
        """Load a session by ID without changing current_session.

        Unlike load_session(), this has no side effects on manager state.
        Used by web agent executor threads for concurrent access.

        Args:
            session_id: Session ID to load

        Returns:
            Loaded Session instance

        Raises:
            FileNotFoundError: If session not found
        """
        # Check in-memory current_session first (handles unsaved new sessions)
        if self.current_session and self.current_session.id == session_id:
            return self.current_session

        # Try local project dir
        session_file = self.session_dir / f"{session_id}.json"
        if session_file.exists():
            return self._load_from_file(session_file)

        # Fall back to scanning all project directories
        from swecli.core.paths import get_paths

        paths = get_paths()
        projects_dir = paths.global_projects_dir
        if projects_dir.exists():
            for project_dir in projects_dir.iterdir():
                if not project_dir.is_dir():
                    continue
                candidate = project_dir / f"{session_id}.json"
                if candidate.exists():
                    return self._load_from_file(candidate)

        raise FileNotFoundError(f"Session {session_id} not found")

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

        with open(session_file, "w") as f:
            json.dump(data, f, indent=2, default=str)

        # Update the index for the on-disk-only path
        try:
            session = self._load_from_file(session_file)
            self._update_index_entry(session)
        except Exception:
            pass
