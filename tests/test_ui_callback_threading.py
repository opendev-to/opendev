"""Tests for TextualUICallback threading behavior."""
from unittest.mock import Mock

from swecli.ui_textual.ui_callback import TextualUICallback


def test_run_on_ui_uses_call_from_thread():
    """Test _run_on_ui always uses call_from_thread."""
    mock_app = Mock()

    callback = TextualUICallback(Mock(), mock_app)
    func = Mock()

    callback._run_on_ui(func, 1, 2, key="value")

    mock_app.call_from_thread.assert_called_once_with(func, 1, 2, key="value")


def test_run_on_ui_calls_directly_when_no_app():
    """Test _run_on_ui calls func directly when no app."""
    callback = TextualUICallback(Mock(), None)
    func = Mock()

    callback._run_on_ui(func, 1, 2)

    func.assert_called_once_with(1, 2)


def test_progress_start_uses_spinner_service():
    """Test on_progress_start uses SpinnerService when available."""
    mock_app = Mock()
    mock_conversation = Mock()

    callback = TextualUICallback(mock_conversation, mock_app)

    callback.on_progress_start("Working...")

    # Uses SpinnerService since mock_app has spinner_service attribute
    mock_app.spinner_service.start.assert_called_once_with("Working...")


def test_tool_call_uses_spinner_service():
    """Test on_tool_call uses SpinnerService when available."""
    mock_app = Mock()
    mock_conversation = Mock()

    callback = TextualUICallback(mock_conversation, mock_app)

    callback.on_tool_call("test_tool", {"arg": "val"})

    # Uses SpinnerService since mock_app has spinner_service attribute
    mock_app.spinner_service.start.assert_called_once()


def test_progress_complete_uses_call_from_thread():
    """Test on_progress_complete uses _run_on_ui (call_from_thread)."""
    mock_app = Mock()
    mock_conversation = Mock()

    callback = TextualUICallback(mock_conversation, mock_app)

    callback.on_progress_complete("Done", success=True)

    # _run_on_ui always uses call_from_thread now
    assert mock_app.call_from_thread.call_count >= 1


def test_run_on_ui_non_blocking_uses_app_method():
    """Test _run_on_ui_non_blocking delegates to app.call_from_thread_nonblocking."""
    mock_app = Mock()
    callback = TextualUICallback(Mock(), mock_app)
    func = Mock()

    callback._run_on_ui_non_blocking(func, "a", key="b")

    mock_app.call_from_thread_nonblocking.assert_called_once_with(func, "a", key="b")


def test_run_on_ui_non_blocking_no_app_calls_directly():
    """Test _run_on_ui_non_blocking calls func directly when no app."""
    callback = TextualUICallback(Mock(), None)
    func = Mock()

    callback._run_on_ui_non_blocking(func, 1, 2)

    func.assert_called_once_with(1, 2)
