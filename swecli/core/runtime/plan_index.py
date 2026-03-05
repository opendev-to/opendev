"""Plan index manager for tracking plan-session-project associations.

Stores a lightweight JSON index at ~/.opendev/plans/plans-index.json
following the same atomic-write pattern as SessionManager._write_index.
"""

from __future__ import annotations

import json
import os
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


class PlanIndex:
    """Manage the plans-index.json file for plan-session-project tracking."""

    INDEX_FILE = "plans-index.json"
    VERSION = 1

    def __init__(self, plans_dir: Path):
        """Initialize plan index.

        Args:
            plans_dir: Directory containing plan files (e.g. ~/.opendev/plans/).
        """
        self._plans_dir = plans_dir
        self._index_path = plans_dir / self.INDEX_FILE

    def _read_index(self) -> dict[str, Any]:
        """Read the index file, returning default structure if missing."""
        if not self._index_path.exists():
            return {"version": self.VERSION, "entries": []}
        try:
            data = json.loads(self._index_path.read_text(encoding="utf-8"))
            if not isinstance(data, dict) or "entries" not in data:
                return {"version": self.VERSION, "entries": []}
            return data
        except (json.JSONDecodeError, OSError):
            return {"version": self.VERSION, "entries": []}

    def _write_index(self, data: dict[str, Any]) -> None:
        """Atomically write the index file (tempfile + rename)."""
        self._plans_dir.mkdir(parents=True, exist_ok=True)
        fd, tmp_path = tempfile.mkstemp(
            dir=str(self._plans_dir), suffix=".tmp", prefix=".plans-idx-"
        )
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2, ensure_ascii=False)
                f.write("\n")
            os.replace(tmp_path, str(self._index_path))
        except Exception:
            # Clean up temp file on failure
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
            raise

    def add_entry(
        self,
        name: str,
        session_id: str | None = None,
        project_path: str | None = None,
    ) -> None:
        """Add or update an entry in the plan index.

        Args:
            name: Plan name (e.g. "bold-blazing-badger").
            session_id: Associated session ID.
            project_path: Associated project working directory.
        """
        data = self._read_index()
        entries: list[dict[str, Any]] = data.get("entries", [])

        # Upsert: remove existing entry with same name
        entries = [e for e in entries if e.get("name") != name]

        entries.append({
            "name": name,
            "sessionId": session_id,
            "projectPath": project_path,
            "created": datetime.now(timezone.utc).isoformat(),
        })

        data["entries"] = entries
        self._write_index(data)

    def get_by_session(self, session_id: str) -> dict[str, Any] | None:
        """Look up plan entry by session ID.

        Args:
            session_id: Session ID to find.

        Returns:
            Entry dict or None.
        """
        data = self._read_index()
        for entry in data.get("entries", []):
            if entry.get("sessionId") == session_id:
                return entry
        return None

    def get_by_project(self, project_path: str) -> list[dict[str, Any]]:
        """List all plan entries for a project.

        Args:
            project_path: Project working directory path.

        Returns:
            List of matching entries.
        """
        data = self._read_index()
        return [e for e in data.get("entries", []) if e.get("projectPath") == project_path]

    def remove_entry(self, name: str) -> None:
        """Remove an entry by plan name.

        Args:
            name: Plan name to remove.
        """
        data = self._read_index()
        entries = data.get("entries", [])
        data["entries"] = [e for e in entries if e.get("name") != name]
        self._write_index(data)
