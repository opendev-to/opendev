"""Tests for MessageController._invoke_on_ui_thread simplified dispatch."""

from unittest.mock import Mock

from swecli.ui_textual.controllers.message_controller import MessageController


def _make_controller() -> MessageController:
    """Create a MessageController with a mock app."""
    mock_app = Mock()
    controller = MessageController.__new__(MessageController)
    controller.app = mock_app
    return controller


def test_invoke_on_ui_thread_delegates():
    """_invoke_on_ui_thread delegates to app.call_from_thread."""
    ctrl = _make_controller()
    callback = Mock()

    ctrl._invoke_on_ui_thread(callback)

    ctrl.app.call_from_thread.assert_called_once_with(callback)


def test_invoke_on_ui_thread_propagates_errors():
    """Errors from app.call_from_thread propagate to the caller."""
    ctrl = _make_controller()
    ctrl.app.call_from_thread.side_effect = RuntimeError("boom")

    callback = Mock()
    try:
        ctrl._invoke_on_ui_thread(callback)
        assert False, "Should have raised"
    except RuntimeError as e:
        assert str(e) == "boom"
