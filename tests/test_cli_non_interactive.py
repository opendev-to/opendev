"""Tests for the CLI non-interactive execution path."""

from types import SimpleNamespace

import pytest
from rich.console import Console

from swecli.cli import _run_non_interactive
from swecli.core.runtime.approval import ApprovalManager
from swecli.core.runtime import ConfigManager
from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.core.context_engineering.history.undo_manager import UndoManager
from swecli.core.runtime.services import RuntimeService
from swecli.models.message import ChatMessage, Role


class _StubAgent:
    def __init__(self, response):
        self.response = response
        self.calls = []

    def run_sync(self, message, deps, message_history=None):  # noqa: D401
        self.calls.append(
            {
                "message": message,
                "deps": deps,
                "history": list(message_history or []),
            }
        )
        return self.response


def _setup_config(tmp_path, monkeypatch):
    config_manager = ConfigManager(tmp_path)
    config = config_manager.get_config()
    config.api_key = "dummy"
    config.opendev_dir = str(tmp_path / ".opendev")
    config.session_dir = str(tmp_path / ".opendev" / "sessions")
    config.log_dir = str(tmp_path / ".opendev" / "logs")
    config.permissions.bash.enabled = True
    config_manager._config = config  # Reassign cached config

    session_manager = SessionManager(session_dir=tmp_path / "sessions")
    session_manager.create_session(str(tmp_path))

    # Avoid touching the real console output by patching Console.print
    printed = []

    def _capture_print(self, *args, **kwargs):  # noqa: D401
        if args:
            printed.append(args[0])

    monkeypatch.setattr(Console, "print", _capture_print, raising=False)

    return config_manager, session_manager, printed


def test_run_non_interactive_success(monkeypatch, tmp_path):
    config_manager, session_manager, printed = _setup_config(tmp_path, monkeypatch)

    agent = _StubAgent({"success": True, "content": "response text"})

    suite = SimpleNamespace(
        tool_registry=object(),
        agents=SimpleNamespace(normal=agent, planning=agent),
    )
    suite.refresh_agents = lambda: None

    monkeypatch.setattr(RuntimeService, "build_suite", lambda self, **kwargs: suite)

    _run_non_interactive(config_manager, session_manager, "hello world")

    assert agent.calls[0]["message"] == "hello world"
    messages = session_manager.current_session.messages
    assert len(messages) == 2
    assert messages[0].role == Role.USER
    assert messages[1].role == Role.ASSISTANT
    assert messages[1].content == "response text"
    assert "response text" in printed


def test_run_non_interactive_failure(monkeypatch, tmp_path):
    config_manager, session_manager, printed = _setup_config(tmp_path, monkeypatch)

    agent = _StubAgent({"success": False, "error": "boom"})
    suite = SimpleNamespace(
        tool_registry=object(),
        agents=SimpleNamespace(normal=agent, planning=agent),
    )
    suite.refresh_agents = lambda: None
    monkeypatch.setattr(RuntimeService, "build_suite", lambda self, **kwargs: suite)

    with pytest.raises(SystemExit) as excinfo:
        _run_non_interactive(config_manager, session_manager, "hello world")

    assert excinfo.value.code == 1
    assert not session_manager.current_session.messages
    assert any("boom" in str(item) for item in printed)
