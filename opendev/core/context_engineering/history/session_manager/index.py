"""Session index management for fast metadata lookups."""

import json
import os
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Optional

from opendev.core.context_engineering.history.file_locks import exclusive_session_lock
from opendev.models.session import Session, SessionMetadata


_INDEX_VERSION = 1


class IndexMixin:
    """Mixin for session index operations.

    Provides methods for reading, writing, and maintaining the sessions index
    file that caches session metadata for fast lookups.
    """

    @property
    def _index_path(self) -> Path:
        """Path to the sessions index file."""
        from opendev.core.paths import SESSIONS_INDEX_FILE_NAME

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
            "hasSessionModel": bool(session.metadata.get("session_model")),
            "channel": session.channel,
            "channelUserId": session.channel_user_id,
            "threadId": session.thread_id,
            "ownerId": session.owner_id,
            "summaryAdditions": session.summary_additions,
            "summaryDeletions": session.summary_deletions,
            "summaryFiles": session.summary_files,
            "timeArchived": session.time_archived.isoformat() if session.time_archived else None,
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
            has_session_model=entry.get("hasSessionModel", False),
            channel=entry.get("channel", "cli"),
            channel_user_id=entry.get("channelUserId", ""),
            thread_id=entry.get("threadId"),
            owner_id=entry.get("ownerId"),
            summary_additions=entry.get("summaryAdditions", 0),
            summary_deletions=entry.get("summaryDeletions", 0),
            summary_files=entry.get("summaryFiles", 0),
        )

    def _update_index_entry(self, session: Session) -> None:
        """Upsert a single session entry in the index."""
        index = self._read_index()
        if index is None:
            # Index missing/corrupted -- rebuild it entirely
            self.rebuild_index()
            index = self._read_index()
            if index is None:
                return  # Rebuild itself failed -- nothing we can do

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

    def _update_index_entry_in_dir(self, directory: Path, session: Session) -> None:
        """Upsert a session entry in the index of a specific directory.

        Used when a session's working_directory resolves to a different
        project directory than self.session_dir.
        """
        from opendev.core.paths import SESSIONS_INDEX_FILE_NAME

        index_path = directory / SESSIONS_INDEX_FILE_NAME
        new_entry = self._session_to_index_entry(session)

        # Read existing index or create a new one
        try:
            if index_path.exists():
                with open(index_path) as f:
                    data = json.load(f)
                if (
                    not isinstance(data, dict)
                    or data.get("version") != _INDEX_VERSION
                    or not isinstance(data.get("entries"), list)
                ):
                    data = {"version": _INDEX_VERSION, "entries": []}
            else:
                data = {"version": _INDEX_VERSION, "entries": []}
        except (json.JSONDecodeError, OSError):
            data = {"version": _INDEX_VERSION, "entries": []}

        entries = data["entries"]

        # Replace existing or append
        for i, entry in enumerate(entries):
            if entry.get("sessionId") == session.id:
                entries[i] = new_entry
                break
        else:
            entries.append(new_entry)

        data["entries"] = entries

        # Atomic write
        fd, tmp_path = tempfile.mkstemp(dir=directory, suffix=".tmp", prefix=".idx-")
        try:
            with os.fdopen(fd, "w") as tmp:
                json.dump(data, tmp, indent=2, default=str)
            Path(tmp_path).replace(index_path)
        except Exception:
            Path(tmp_path).unlink(missing_ok=True)
            raise

    def _remove_index_entry(self, session_id: str) -> None:
        """Remove a single session entry from the index."""
        index = self._read_index()
        if index is None:
            return  # Nothing to remove from

        entries = [e for e in index["entries"] if e.get("sessionId") != session_id]
        self._write_index(entries)

    def _remove_index_entry_in_dir(self, directory: Path, session_id: str) -> None:
        """Remove a session entry from the index in a specific directory.

        Used for cross-project deletes where the session lives in a different
        project directory than self.session_dir.
        """
        from opendev.core.paths import SESSIONS_INDEX_FILE_NAME

        index_path = directory / SESSIONS_INDEX_FILE_NAME
        if not index_path.exists():
            return
        try:
            with open(index_path) as f:
                data = json.load(f)
            if not isinstance(data, dict) or not isinstance(data.get("entries"), list):
                return
            entries = [e for e in data["entries"] if e.get("sessionId") != session_id]
            data["entries"] = entries
            # Atomic write
            fd, tmp_path = tempfile.mkstemp(
                dir=directory, suffix=".tmp", prefix=".idx-"
            )
            try:
                with os.fdopen(fd, "w") as tmp:
                    json.dump(data, tmp)
                Path(tmp_path).replace(index_path)
            except Exception:
                Path(tmp_path).unlink(missing_ok=True)
                raise
        except Exception:
            pass

    def migrate_json_to_jsonl(self) -> int:
        """Migrate existing .json sessions to .jsonl format (one-time migration).

        This converts legacy full-JSON sessions to the new format:
        - Session metadata -> {session_id}.json (without messages)
        - Messages -> {session_id}.jsonl (one per line)

        The original .json files are renamed to .json.bak for safety.

        Returns:
            Number of sessions migrated
        """
        from opendev.core.paths import SESSIONS_INDEX_FILE_NAME

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
        from opendev.core.paths import SESSIONS_INDEX_FILE_NAME

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
