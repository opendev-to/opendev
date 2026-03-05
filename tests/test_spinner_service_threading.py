"""Tests for SpinnerService simplified thread dispatch helpers."""

from unittest.mock import Mock

from swecli.ui_textual.managers.spinner_service import SpinnerService


def _make_service() -> SpinnerService:
    """Create a SpinnerService with a mock app."""
    mock_app = Mock()
    service = SpinnerService.__new__(SpinnerService)
    service.app = mock_app
    return service


def test_run_blocking_delegates():
    """_run_blocking delegates to app.call_from_thread."""
    svc = _make_service()
    func = Mock()

    svc._run_blocking(func, "a", key="b")

    svc.app.call_from_thread.assert_called_once_with(func, "a", key="b")


def test_run_non_blocking_delegates():
    """_run_non_blocking delegates to app.call_from_thread_nonblocking."""
    svc = _make_service()
    func = Mock()

    svc._run_non_blocking(func, "x", key="y")

    svc.app.call_from_thread_nonblocking.assert_called_once_with(func, "x", key="y")
