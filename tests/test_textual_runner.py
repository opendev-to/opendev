"""Tests for TextualRunner integration scaffolding."""

from pathlib import Path
from types import SimpleNamespace

from rich.console import Console

from swecli.models.config import AppConfig
from swecli.models.message import ChatMessage, Role
from swecli.core.runtime.config import ConfigManager
from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.ui_textual.runner import TextualRunner


class DummyConfigManager(ConfigManager):
    """Config manager that keeps all state under the temporary working directory."""

    def load_config(self) -> AppConfig:
        config = AppConfig()
        base = Path(self.working_dir) / ".swecli-test"
        config.opendev_dir = str(base)
        config.session_dir = str(base / "sessions")
        config.log_dir = str(base / "logs")
        config.command_dir = ".swecli-test/commands"
        config.model_provider = "openai"
        config.model = "gpt-4o-mini"
        config.api_key = "test-key-for-unit-tests"
        self._config = config
        return config

    def ensure_directories(self) -> None:
        config = self.get_config()
        Path(config.opendev_dir).mkdir(parents=True, exist_ok=True)
        Path(config.session_dir).mkdir(parents=True, exist_ok=True)
        Path(config.log_dir).mkdir(parents=True, exist_ok=True)


class DummyRepl:
    """Minimal REPL stub exercising the runner plumbing."""

    def __init__(self, config_manager: ConfigManager, session_manager: SessionManager):
        self.console = Console(record=True)
        self.config_manager = config_manager
        self.config = config_manager.get_config()
        self.session_manager = session_manager
        from swecli.core.runtime import OperationMode
        self.mode_manager = SimpleNamespace(
            set_mode=lambda mode: None,
            current_mode=OperationMode.NORMAL,
            has_pending_plan=lambda: False,
        )
        self.approval_manager = SimpleNamespace(chat_app=None, pre_approved_commands=set())
        self.output_formatter = SimpleNamespace(
            format_tool_result=lambda **_: "tool-output"
        )
        self.tool_registry = SimpleNamespace()
        self.task_manager = SimpleNamespace(
            get_running_tasks=lambda: [],
            add_listener=lambda cb: None,
            remove_listener=lambda cb: None,
        )
        self.cleaned = False

    def _process_query(self, message: str) -> None:
        session = self.session_manager.get_current_session()
        if session is None:
            session = self.session_manager.create_session()
        self.session_manager.add_message(ChatMessage(role=Role.USER, content=message))
        self.session_manager.add_message(
            ChatMessage(role=Role.ASSISTANT, content=f"echo:{message}")
        )

    def _handle_command(self, command: str) -> None:
        self.console.print(f"handled {command}")

    def _cleanup(self) -> None:  # pragma: no cover - invoked at runner shutdown
        self.cleaned = True


def test_textual_runner_process_query(tmp_path):
    working_dir = tmp_path
    config_manager = DummyConfigManager(working_dir)
    config_manager.load_config()
    session_root = Path(config_manager.get_config().session_dir)
    session_manager = SessionManager(session_dir=session_root)
    session_manager.create_session(working_directory=str(working_dir))

    repl = DummyRepl(config_manager, session_manager)
    runner = TextualRunner(
        working_dir=working_dir,
        repl=repl,
        config_manager=config_manager,
        session_manager=session_manager,
    )

    messages = runner._run_query("hello")

    assert [m.role for m in messages] == [Role.USER, Role.ASSISTANT]
    assert messages[-1].content == "echo:hello"
    assert runner.console_bridge._console_queue.empty()
    assert repl.approval_manager.chat_app is runner.app

    runner._run_command("/help")
    queued = runner.console_bridge._console_queue.get_nowait()
    assert "handled /help" in queued
