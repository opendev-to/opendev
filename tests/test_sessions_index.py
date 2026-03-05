"""Tests for sessions-index.json feature."""

import json
from pathlib import Path

import pytest

from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.core.paths import SESSIONS_INDEX_FILE_NAME
from swecli.models.message import ChatMessage, Role


def _add_user_message(mgr: SessionManager, content: str = "Hello world") -> None:
    """Helper to add a user message and save."""
    mgr.current_session.add_message(ChatMessage(role=Role.USER, content=content))
    mgr.save_session()


class TestSessionsIndex:
    """Tests for the sessions-index.json feature."""

    def test_save_creates_index(self, tmp_path: Path) -> None:
        """save_session() creates sessions-index.json with correct entry."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        _add_user_message(mgr)

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        assert index_path.exists()

        data = json.loads(index_path.read_text())
        assert data["version"] == 1
        assert len(data["entries"]) == 1
        assert data["entries"][0]["sessionId"] == session.id
        assert data["entries"][0]["messageCount"] == 1

    def test_list_sessions_uses_index(self, tmp_path: Path) -> None:
        """list_sessions() reads from the index instead of globbing files."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        _add_user_message(mgr)

        sessions = mgr.list_sessions()
        assert len(sessions) == 1
        assert sessions[0].id == session.id

    def test_index_self_healing(self, tmp_path: Path) -> None:
        """Deleting the index file triggers a rebuild on next list_sessions()."""
        mgr = SessionManager(session_dir=tmp_path)
        mgr.create_session()
        _add_user_message(mgr)

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        assert index_path.exists()

        # Delete the index
        index_path.unlink()
        assert not index_path.exists()

        # list_sessions should rebuild it
        sessions = mgr.list_sessions()
        assert len(sessions) == 1
        assert index_path.exists()

    def test_corrupted_index_triggers_rebuild(self, tmp_path: Path) -> None:
        """Writing garbage to the index triggers a rebuild."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        _add_user_message(mgr)

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        # Write garbage
        index_path.write_text("not json at all {{{")

        sessions = mgr.list_sessions()
        assert len(sessions) == 1
        assert sessions[0].id == session.id

        # Index should be valid now
        data = json.loads(index_path.read_text())
        assert data["version"] == 1

    def test_delete_removes_from_index(self, tmp_path: Path) -> None:
        """Deleting a session removes it from the index."""
        mgr = SessionManager(session_dir=tmp_path)

        s1 = mgr.create_session()
        _add_user_message(mgr, "First session")

        s2 = mgr.create_session()
        _add_user_message(mgr, "Second session")

        sessions = mgr.list_sessions()
        assert len(sessions) == 2

        mgr.delete_session(s1.id)

        sessions = mgr.list_sessions()
        assert len(sessions) == 1
        assert sessions[0].id == s2.id

    def test_set_title_updates_index(self, tmp_path: Path) -> None:
        """set_title() updates the title in the index entry."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        _add_user_message(mgr)

        # Clear current session so set_title takes the on-disk path
        mgr.current_session = None
        mgr.set_title(session.id, "Custom Title")

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        data = json.loads(index_path.read_text())
        assert data["entries"][0]["title"] == "Custom Title"

    def test_save_auto_generates_title(self, tmp_path: Path) -> None:
        """Session with messages gets auto-title in the index."""
        mgr = SessionManager(session_dir=tmp_path)
        mgr.create_session()
        _add_user_message(mgr, "Fix the login redirect bug")

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        data = json.loads(index_path.read_text())
        title = data["entries"][0]["title"]
        assert title is not None
        assert "login" in title.lower() or "redirect" in title.lower() or "fix" in title.lower()

    def test_save_preserves_existing_title(self, tmp_path: Path) -> None:
        """Existing title is not overwritten by auto-generation."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        session.metadata["title"] = "My Custom Title"
        _add_user_message(mgr)

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        data = json.loads(index_path.read_text())
        assert data["entries"][0]["title"] == "My Custom Title"

    def test_index_excludes_empty_sessions(self, tmp_path: Path) -> None:
        """Empty sessions (no messages) are not included in the index."""
        mgr = SessionManager(session_dir=tmp_path)
        mgr.create_session()
        # Don't add messages — save_session() should be a no-op
        mgr.save_session()

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        # Index should not exist (no non-empty sessions saved)
        assert not index_path.exists()

    def test_rebuild_excludes_index_file(self, tmp_path: Path) -> None:
        """rebuild_index() doesn't try to parse sessions-index.json as a session."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        _add_user_message(mgr)

        # Rebuild should work cleanly without trying to load the index as a session
        result = mgr.rebuild_index()
        assert len(result) == 1
        assert result[0].id == session.id

        # Verify index file is valid
        data = json.loads((tmp_path / SESSIONS_INDEX_FILE_NAME).read_text())
        assert len(data["entries"]) == 1

    def test_repeated_saves_single_index_entry(self, tmp_path: Path) -> None:
        """Saving the same session 5x produces exactly 1 index entry (upsert)."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        session.add_message(ChatMessage(role=Role.USER, content="Hello"))

        for _ in range(5):
            mgr.save_session()

        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        data = json.loads(index_path.read_text())
        assert len(data["entries"]) == 1
        assert data["entries"][0]["sessionId"] == session.id

    def test_continue_flow_via_index(self, tmp_path: Path) -> None:
        """A fresh SessionManager can load_latest_session() via index (--continue)."""
        mgr1 = SessionManager(session_dir=tmp_path)
        session = mgr1.create_session()
        _add_user_message(mgr1, "First message for continue test")

        # Simulate a new CLI invocation pointing at the same session dir
        mgr2 = SessionManager(session_dir=tmp_path)
        loaded = mgr2.load_latest_session()

        assert loaded is not None
        assert loaded.id == session.id
        assert len(loaded.messages) == 1
        assert loaded.messages[0].content == "First message for continue test"

    def test_update_index_entry_with_missing_index(self, tmp_path: Path) -> None:
        """_update_index_entry() with no index file still includes the session."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        session.add_message(ChatMessage(role=Role.USER, content="Test"))

        # Write the session file manually (without going through save_session)
        session_file = tmp_path / f"{session.id}.json"
        with open(session_file, "w") as f:
            json.dump(session.model_dump(), f, indent=2, default=str)

        # Ensure no index exists
        index_path = tmp_path / SESSIONS_INDEX_FILE_NAME
        assert not index_path.exists()

        # Call _update_index_entry directly — should rebuild then upsert
        mgr._update_index_entry(session)

        assert index_path.exists()
        data = json.loads(index_path.read_text())
        session_ids = [e["sessionId"] for e in data["entries"]]
        assert session.id in session_ids
