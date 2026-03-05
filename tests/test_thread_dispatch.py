"""Tests for SWECLIChatApp.call_from_thread override and call_from_thread_nonblocking."""

import threading
from unittest.mock import Mock, patch, MagicMock

from swecli.ui_textual.chat_app import SWECLIChatApp


def _make_app() -> SWECLIChatApp:
    """Create a minimal SWECLIChatApp for testing (no Textual runtime)."""
    app = SWECLIChatApp.__new__(SWECLIChatApp)
    app._loop = MagicMock()
    app._thread_id = threading.get_ident()
    return app


class TestCallFromThreadOverride:
    """Tests for the call_from_thread override on SWECLIChatApp."""

    def test_ui_thread_executes_directly(self):
        """When called from the UI thread, execute callback directly."""
        app = _make_app()
        func = Mock(return_value=42)

        result = app.call_from_thread(func, "a", "b", key="val")

        func.assert_called_once_with("a", "b", key="val")
        assert result == 42

    def test_worker_thread_delegates_to_super(self):
        """When called from a worker thread, delegate to App.call_from_thread."""
        app = _make_app()
        app._thread_id = 0  # Different from current thread

        func = Mock(return_value=99)

        with patch("textual.app.App.call_from_thread", return_value=99) as mock_super:
            result = app.call_from_thread(func, "x")

        mock_super.assert_called_once_with(func, "x")
        assert result == 99

    def test_no_loop_executes_directly(self):
        """When _loop is None, execute callback directly."""
        app = _make_app()
        app._loop = None

        func = Mock(return_value="direct")

        result = app.call_from_thread(func, 1, 2, 3)

        func.assert_called_once_with(1, 2, 3)
        assert result == "direct"


class TestCallFromThreadNonblocking:
    """Tests for call_from_thread_nonblocking on SWECLIChatApp."""

    def test_nonblocking_ui_thread_executes_directly(self):
        """From UI thread, execute directly."""
        app = _make_app()
        func = Mock()

        app.call_from_thread_nonblocking(func, "a", key="b")

        func.assert_called_once_with("a", key="b")

    def test_nonblocking_worker_thread_schedules_on_loop(self):
        """From worker thread, schedule via call_soon_threadsafe."""
        app = _make_app()
        app._thread_id = 0  # Different from current thread
        func = Mock()

        app.call_from_thread_nonblocking(func, "x")

        app._loop.call_soon_threadsafe.assert_called_once()
        # Execute the scheduled lambda to verify it calls func
        scheduled_fn = app._loop.call_soon_threadsafe.call_args[0][0]
        scheduled_fn()
        func.assert_called_once_with("x")

    def test_nonblocking_no_loop_executes_directly(self):
        """When _loop is None, execute directly."""
        app = _make_app()
        app._loop = None

        func = Mock()

        app.call_from_thread_nonblocking(func, 1, 2)

        func.assert_called_once_with(1, 2)
