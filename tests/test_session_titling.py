"""Tests for session titling feature."""

from pathlib import Path


from swecli.core.context_engineering.history.session_manager import SessionManager


class TestSessionTitling:
    """Tests for session title generation and persistence."""

    def test_generate_title_from_message(self) -> None:
        """Should extract concise title from first user message."""
        messages = [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Fix the login page redirect bug"},
        ]
        title = SessionManager.generate_title(messages)
        assert "login" in title.lower() or "redirect" in title.lower() or "fix" in title.lower()

    def test_title_max_50_chars(self) -> None:
        """Title should not exceed 50 characters."""
        messages = [
            {"role": "user", "content": "x" * 200},
        ]
        title = SessionManager.generate_title(messages)
        assert len(title) <= 50

    def test_title_stored_in_metadata(self, tmp_path: Path) -> None:
        """Title should persist in session JSON."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        # Add a dummy message so save works
        from swecli.models.message import ChatMessage, Role

        session.add_message(ChatMessage(role=Role.USER, content="Hello"))
        mgr.save_session()

        mgr.set_title(session.id, "Test Title")

        # Reload and check
        loaded = mgr.load_session(session.id)
        assert loaded.metadata.get("title") == "Test Title"

    def test_list_sessions_includes_title(self, tmp_path: Path) -> None:
        """Session listing should show title alongside ID."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        from swecli.models.message import ChatMessage, Role

        session.add_message(ChatMessage(role=Role.USER, content="Hello"))
        session.metadata["title"] = "My Session"
        mgr.save_session()

        sessions = mgr.list_sessions()
        assert len(sessions) == 1
        assert sessions[0].title == "My Session"

    def test_empty_message_fallback(self) -> None:
        """Should fallback to 'Untitled' for empty messages."""
        assert SessionManager.generate_title([]) == "Untitled"
        assert SessionManager.generate_title([{"role": "user", "content": ""}]) == "Untitled"

    def test_sentence_boundary_extraction(self) -> None:
        """Should extract up to the first sentence boundary."""
        messages = [
            {"role": "user", "content": "Add user auth. Then add tests."},
        ]
        title = SessionManager.generate_title(messages)
        assert title == "Add user auth"

    def test_set_title_truncates(self, tmp_path: Path) -> None:
        """set_title should truncate to 50 chars."""
        mgr = SessionManager(session_dir=tmp_path)
        session = mgr.create_session()
        from swecli.models.message import ChatMessage, Role

        session.add_message(ChatMessage(role=Role.USER, content="Hello"))
        mgr.save_session()

        mgr.set_title(session.id, "a" * 100)
        loaded = mgr.load_session(session.id)
        assert len(loaded.metadata["title"]) == 50
