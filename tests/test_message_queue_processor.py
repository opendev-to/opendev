"""Tests for the MessageProcessor component."""

import queue
import threading
import time
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

import pytest

from swecli.ui_textual.runner_components.message_processor import MessageProcessor


class MockApp:
    """Mock Textual app for testing."""

    def __init__(self):
        self.conversation = MagicMock()
        self._ui_calls = []
        self._processing_complete_called = False
        self._spinner_started = False
        self.update_queue_indicator = MagicMock()

    def call_from_thread(self, func, *args):
        """Simulate call_from_thread by running immediately."""
        self._ui_calls.append((func, args))
        if callable(func):
            func(*args) if args else func()

    def notify_processing_error(self, error):
        self._ui_calls.append(("notify_processing_error", (error,)))

    def notify_processing_complete(self):
        self._processing_complete_called = True

    def _start_local_spinner(self):
        self._spinner_started = True


@pytest.fixture
def mock_app():
    """Create a mock Textual app."""
    return MockApp()


@pytest.fixture
def on_query():
    """Create a mock query callback."""
    mock = MagicMock(return_value=[])
    return mock


@pytest.fixture
def on_command():
    """Create a mock command callback."""
    return MagicMock()


@pytest.fixture
def on_render():
    """Create a mock render callback."""
    return MagicMock()


@pytest.fixture
def processor(mock_app, on_query, on_command, on_render):
    """Create a MessageProcessor for testing."""
    callbacks = {
        "handle_query": on_query,
        "handle_command": on_command,
        "render_responses": on_render,
        "on_error": mock_app.notify_processing_error,
        "on_command_error": mock_app.conversation.add_error,
    }
    return MessageProcessor(app=mock_app, callbacks=callbacks)


class TestMessageProcessor:
    """Test suite for MessageProcessor."""

    def test_init(self, mock_app, on_query, on_command, on_render):
        """Test processor initialization."""
        callbacks = {
            "handle_query": on_query,
            "handle_command": on_command,
            "render_responses": on_render,
        }
        processor = MessageProcessor(app=mock_app, callbacks=callbacks)
        assert processor._app is mock_app
        assert processor._callbacks == callbacks
        assert processor._processor_thread is None

    def test_get_queue_size_empty(self, processor):
        """Test queue size when empty."""
        assert processor.get_queue_size() == 0

    def test_enqueue_basic(self, processor):
        """Test basic message enqueueing."""
        processor.enqueue_message("hello")
        assert processor.get_queue_size() == 1

    def test_enqueue_multiple(self, processor):
        """Test enqueueing multiple messages."""
        processor.enqueue_message("message 1")
        processor.enqueue_message("message 2")
        processor.enqueue_message("message 3")
        assert processor.get_queue_size() == 3

    def test_enqueue_with_needs_display(self, processor):
        """Test enqueueing with needs_display flag."""
        processor.enqueue_message("hello", needs_display=True)
        assert processor.get_queue_size() == 1
        # Verify item is properly stored
        item = processor._pending.get_nowait()
        assert item == ("hello", True)

    def test_start_and_stop(self, processor):
        """Test starting and stopping the processor."""
        processor.start()
        assert processor._processor_thread is not None
        assert processor._processor_thread.is_alive()

        processor.stop()
        assert processor._processor_thread is None

    def test_start_stop_idempotent(self, processor):
        """Test that stop can be called even if not started."""
        processor.stop()  # Should not raise
        assert processor._processor_thread is None

    def test_process_query_message(self, processor, mock_app, on_query, on_render):
        """Test processing a query message."""
        on_query.return_value = [{"role": "assistant", "content": "response"}]

        processor.start()
        processor.enqueue_message("hello")

        # Wait for processing
        time.sleep(0.6)
        processor.stop()

        on_query.assert_called_once_with("hello")
        # Spinner logic is internal to _run_query in generic implementation,
        # but pure MessageProcessor just calls handle_query.
        # The mock_app._spinner_started won't be true unless handle_query sets it.
        # But wait, MessageProcessor calls call_from_thread for updates?
        # Typically the runner's _run_query does not touch UI directly, but calls app methods if needed.
        # In current runner logic, spinner is managed by app or inside _run_query logic?
        # Actually in runner.py:
        # _run_query calls self.repl._process_query(message)
        # MessageProcessor creates a thread that calls handle_query().
        # It doesn't explicitly start a spinner itself.

    def test_process_command_message(self, processor, mock_app, on_command):
        """Test processing a command message."""
        processor.start()
        processor.enqueue_message("/help")

        # Wait for processing
        time.sleep(0.6)
        processor.stop()

        on_command.assert_called_once_with("/help")

    def test_process_with_display_needed(self, processor, mock_app, on_query):
        """Test processing message that needs display."""
        on_query.return_value = []

        processor.start()
        processor.enqueue_message("queued message", needs_display=True)

        # Wait for processing
        time.sleep(0.6)
        processor.stop()

        # Verify add_user_message was called
        # processor calls app.conversation.add_user_message("queued message") if needed
        # Wait, MessageProcessor source:
        # if needs_display and hasattr(self._app, "conversation"):
        #      self._app.call_from_thread(self._app.conversation.add_user_message, message)
        call_funcs = [call[0] for call in mock_app._ui_calls]
        assert mock_app.conversation.add_user_message in call_funcs


class TestMessageProcessorErrorHandling:
    """Test suite for error handling in MessageProcessor."""

    def test_query_error_handling(self, mock_app, on_command, on_render):
        """Test error handling for query processing."""
        on_query = MagicMock(side_effect=Exception("Query failed"))
        
        callbacks = {
            "handle_query": on_query,
            "handle_command": on_command,
            "render_responses": lambda x: None,
            "on_error": mock_app.notify_processing_error,
        }

        processor = MessageProcessor(app=mock_app, callbacks=callbacks)

        processor.start()
        processor.enqueue_message("hello")

        # Wait for processing
        time.sleep(0.6)
        processor.stop()

        # Error should be notified via on_error callback
        call_funcs = [call[0] for call in mock_app._ui_calls]
        # Our mock notify_processing_error records itself in _ui_calls
        # but here we passed it as a callback directly
        # MessageProcessor calls self._callbacks["on_error"](str(e))
        # which is mock_app.notify_processing_error
        # mock_app.notify_processing_error appends to _ui_calls
        assert "notify_processing_error" in [c[0] for c in mock_app._ui_calls]

    def test_command_error_handling(self, mock_app, on_query, on_render):
        """Test error handling for command processing."""
        on_command = MagicMock(side_effect=Exception("Command failed"))
        
        callbacks = {
            "handle_query": on_query,
            "handle_command": on_command,
            "on_command_error": mock_app.conversation.add_error,
            "on_error": mock_app.notify_processing_error,
        }

        processor = MessageProcessor(app=mock_app, callbacks=callbacks)

        processor.start()
        processor.enqueue_message("/badcommand")

        # Wait for processing
        time.sleep(0.6)
        processor.stop()

        # on_command_error should be called
        call_funcs = [call[0] for call in mock_app._ui_calls]
        # MessageProcessor calls self._app.call_from_thread(on_command_error, str(e))
        # But wait, does it?
        # In current implementation, on_command_error is called directly?
        # Let's check source code logic.
        # typically: self._app.call_from_thread(self._callbacks["on_command_error"], str(e))
        # mock_app.conversation.add_error is a MagicMock, so we can verify if it was passed to call_from_thread
        assert mock_app.conversation.add_error in call_funcs
