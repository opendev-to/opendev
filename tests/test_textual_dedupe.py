import types
from types import SimpleNamespace

import pytest
from rich.text import Text

from swecli.ui_textual.chat_app import SWECLIChatApp
from swecli.ui_textual.managers.console_buffer_manager import ConsoleBufferManager


def make_app() -> SWECLIChatApp:
    app = SWECLIChatApp.__new__(SWECLIChatApp)
    app._queued_console_renderables = []
    app._spinner_active = False
    app._buffer_console_output = False
    app._last_assistant_lines = set()
    app._last_rendered_assistant = None
    app._last_assistant_normalized = None
    app._pending_assistant_normalized = None
    app._spinner = SimpleNamespace(active=False)
    app._console_buffer = ConsoleBufferManager(app)
    # _tool_summary must not exist (hasattr check) or be a proper mock
    # Delete the attribute so hasattr returns False
    # (ConsoleBufferManager checks hasattr before calling)
    return app


def test_should_suppress_exact_string():
    app = make_app()
    message = "⏺ Hello! I'm ready to help with your software engineering tasks."
    app.record_assistant_message(message)

    assert app._should_suppress_renderable(message)
    assert app._should_suppress_renderable(Text(message))


def test_should_suppress_with_ansi_and_wrap():
    app = make_app()
    message = "⏺ Hello! I'm ready to help with your software engineering tasks."
    app.record_assistant_message(message)

    ansi_message = "\033[32m⏺\033[0m Hello! I'm ready to help with your software engineering tasks."
    multiline = "⏺ Hello! I'm ready to help with your software\nengineering tasks."

    assert app._should_suppress_renderable(ansi_message)
    assert app._should_suppress_renderable(multiline)


def test_does_not_suppress_different_text():
    app = make_app()
    message = "⏺ Hello!"
    app.record_assistant_message(message)

    different = "⏺ Different content"
    assert not app._should_suppress_renderable(different)
