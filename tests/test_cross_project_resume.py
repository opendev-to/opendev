"""Verify CLI --resume finds sessions from any project directory."""

from unittest.mock import patch, MagicMock
from pathlib import Path

from opendev.core.context_engineering.history.session_manager import SessionManager
from opendev.models.message import ChatMessage, Role


def test_list_all_sessions_finds_cross_project(tmp_path):
    """Sessions created under project-a are visible from project-b."""
    projects_dir = tmp_path / "projects"
    projects_dir.mkdir()

    # Create a session under project-a
    sm_a = SessionManager(session_dir=projects_dir / "project-a")
    sess = sm_a.create_session(str(tmp_path / "project-a"))
    sm_a.add_message(ChatMessage(role=Role.USER, content="hello"))
    sm_a.save_session()
    created_id = sess.id

    # From project-b, list_all_sessions should find it
    sm_b = SessionManager(session_dir=projects_dir / "project-b")

    # Mock get_paths so list_all_sessions scans our tmp projects dir
    mock_paths = MagicMock()
    mock_paths.global_projects_dir = projects_dir

    with patch("opendev.core.paths.get_paths", return_value=mock_paths):
        all_sessions = sm_b.list_all_sessions()

    ids = [s.id for s in all_sessions]
    assert created_id in ids


def test_list_sessions_does_not_find_cross_project(tmp_path):
    """list_sessions (project-scoped) should NOT find other projects."""
    projects_dir = tmp_path / "projects"
    projects_dir.mkdir()

    sm_a = SessionManager(session_dir=projects_dir / "project-a")
    sess = sm_a.create_session(str(tmp_path / "project-a"))
    sm_a.add_message(ChatMessage(role=Role.USER, content="hello"))
    sm_a.save_session()

    sm_b = SessionManager(session_dir=projects_dir / "project-b")
    local_sessions = sm_b.list_sessions()
    ids = [s.id for s in local_sessions]
    assert sess.id not in ids


def test_cli_pick_session_uses_list_all_sessions(tmp_path):
    """_pick_session_interactively calls list_all_sessions, not list_sessions."""
    from opendev.cli.main import _pick_session_interactively

    mock_sm = MagicMock()
    mock_sm.list_all_sessions.return_value = []

    # SessionManager is imported locally inside _pick_session_interactively
    with patch(
        "opendev.core.context_engineering.history.session_manager.SessionManager",
        return_value=mock_sm,
    ) as mock_cls, patch(
        "opendev.core.context_engineering.history.SessionManager",
        mock_cls,
    ):
        result = _pick_session_interactively(tmp_path)

    assert result is None
    mock_sm.list_all_sessions.assert_called_once()
    mock_sm.list_sessions.assert_not_called()
