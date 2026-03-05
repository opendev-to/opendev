"""Tests for session manager convenience helpers."""

from pathlib import Path

from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.models.message import ChatMessage, Role


def test_find_latest_session(tmp_path):
    session_dir = tmp_path / "sessions"
    manager = SessionManager(session_dir=session_dir)

    repo = tmp_path / "repo"

    first = manager.create_session(str(repo))
    manager.add_message(ChatMessage(role=Role.USER, content="first"))
    manager.save_session()

    # Create a second, more recent session for the same repo
    second = manager.create_session(str(repo))
    manager.add_message(ChatMessage(role=Role.USER, content="hello"))
    manager.save_session()

    # Now project-scoped: returns the newest session in the directory
    latest = manager.find_latest_session()
    assert latest is not None
    assert latest.id == second.id

    # An empty directory has no sessions
    empty_dir = tmp_path / "empty_sessions"
    empty_manager = SessionManager(session_dir=empty_dir)
    assert empty_manager.find_latest_session() is None


def test_load_latest_session(tmp_path):
    session_dir = tmp_path / "sessions"
    manager = SessionManager(session_dir=session_dir)

    manager.create_session(str(tmp_path / "repo"))
    manager.add_message(ChatMessage(role=Role.USER, content="repo"))
    manager.save_session()

    manager.create_session(str(tmp_path / "other"))
    manager.add_message(ChatMessage(role=Role.USER, content="other"))
    manager.save_session()

    # Returns the most recent session (the "other" one)
    session = manager.load_latest_session()
    assert session is not None
    assert Path(session.working_directory).resolve() == (tmp_path / "other").resolve()
