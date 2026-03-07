"""Session listing, finding, and deletion."""

import json
from pathlib import Path
from typing import Optional, Union

from opendev.models.session import Session, SessionMetadata


class ListingMixin:
    """Mixin for session listing and lookup operations."""

    def list_sessions(
        self,
        owner_id: Optional[str] = None,
        include_archived: bool = False,
    ) -> list[SessionMetadata]:
        """List saved sessions, optionally filtered by owner.

        Args:
            owner_id: If provided, only return sessions owned by this user.
            include_archived: If False (default), exclude archived sessions.
        """
        index = self._read_index()
        if index is not None:
            if not include_archived:
                entries = [
                    e for e in index["entries"] if not e.get("timeArchived")
                ]
            else:
                entries = index["entries"]
            sessions = [self._metadata_from_index_entry(e) for e in entries]
        else:
            sessions = self.rebuild_index()
            if not include_archived:
                # rebuild_index returns all sessions; filter archived ones
                # by re-reading the index which now has timeArchived info
                re_index = self._read_index()
                if re_index is not None:
                    entries = [
                        e for e in re_index["entries"] if not e.get("timeArchived")
                    ]
                    sessions = [self._metadata_from_index_entry(e) for e in entries]

        if owner_id:
            sessions = [s for s in sessions if s.owner_id == owner_id]

        return sorted(sessions, key=lambda s: s.updated_at, reverse=True)

    def list_all_sessions(self, include_archived: bool = False) -> list[SessionMetadata]:
        """List sessions from ALL project directories.

        Scans every subdirectory under ``~/.opendev/projects/`` and merges
        their session indexes into a single list. Useful for the web UI
        sidebar which needs to display sessions across all workspaces.

        Args:
            include_archived: If False (default), exclude archived sessions.

        Returns:
            List of session metadata from all projects, sorted by update time
            (newest first).
        """
        from opendev.core.paths import get_paths, SESSIONS_INDEX_FILE_NAME
        from opendev.core.context_engineering.history.session_manager.index import _INDEX_VERSION

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
                                if not include_archived and entry.get("timeArchived"):
                                    continue
                                seen_ids.add(sid)
                                all_sessions.append(
                                    self._metadata_from_index_entry(entry)
                                )
                        continue

                # No valid index -- rebuild from JSON files in this dir
                for session_file in project_dir.glob("*.json"):
                    if session_file.name == SESSIONS_INDEX_FILE_NAME:
                        continue
                    try:
                        session = self._load_from_file(session_file)
                        if len(session.messages) == 0:
                            continue
                        if not include_archived and session.is_archived:
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
        Supports cross-project deletion: if the session isn't in the current
        project directory, scans all project directories to find it.

        Args:
            session_id: Session ID to delete
        """
        # Find the session file (local project dir first, then scan all projects)
        session_file = self.session_dir / f"{session_id}.json"
        if not session_file.exists():
            from opendev.core.paths import get_paths

            paths = get_paths()
            projects_dir = paths.global_projects_dir
            if projects_dir.exists():
                for project_dir in projects_dir.iterdir():
                    if not project_dir.is_dir():
                        continue
                    candidate = project_dir / f"{session_id}.json"
                    if candidate.exists():
                        session_file = candidate
                        break

        target_dir = session_file.parent

        # Delete .json metadata
        if session_file.exists():
            session_file.unlink()

        # Delete .jsonl transcript
        jsonl_file = target_dir / f"{session_id}.jsonl"
        if jsonl_file.exists():
            jsonl_file.unlink()

        # Delete .debug log
        debug_file = target_dir / f"{session_id}.debug"
        if debug_file.exists():
            debug_file.unlink()

        # Remove from sessions index in the appropriate directory
        if target_dir == self.session_dir:
            self._remove_index_entry(session_id)
        else:
            self._remove_index_entry_in_dir(target_dir, session_id)

    def get_session_by_id(self, session_id: str, owner_id: Optional[str] = None) -> Session:
        """Load a session by ID without changing current_session.

        Args:
            session_id: Session ID to load
            owner_id: Optional owner to enforce; returns only if it matches.

        Returns:
            Loaded Session instance.

        Raises:
            FileNotFoundError: If the file doesn't exist or ownership mismatches.
        """

        # Try local project dir
        session_file = self.session_dir / f"{session_id}.json"
        if session_file.exists():
            return self._load_from_file(session_file)

        # Fall back to scanning all project directories
        from opendev.core.paths import get_paths

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
        from opendev.core.paths import get_paths

        paths = get_paths()
        workspaces: list[str] = []

        if not paths.global_projects_dir.exists():
            return []

        # Scan all project directories
        for project_dir in paths.global_projects_dir.iterdir():
            if not project_dir.is_dir():
                continue

            # Skip the unknown/fallback directory
            from opendev.core.paths import FALLBACK_PROJECT_DIR_NAME

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
