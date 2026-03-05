"""Tests for SessionDebugLogger."""

import json
import threading
from pathlib import Path

import pytest

from swecli.core.debug.session_debug_logger import (
    SessionDebugLogger,
    get_debug_logger,
    set_debug_logger,
    _truncate,
)


@pytest.fixture
def tmp_session_dir(tmp_path):
    """Create a temporary session directory."""
    session_dir = tmp_path / "sessions"
    session_dir.mkdir()
    return session_dir


@pytest.fixture
def logger(tmp_session_dir):
    """Create a real debug logger."""
    return SessionDebugLogger(tmp_session_dir, "test1234")


@pytest.fixture(autouse=True)
def cleanup_global_logger():
    """Ensure global logger is cleaned up after each test."""
    yield
    set_debug_logger(None)


class TestSessionDebugLogger:
    def test_logger_writes_jsonl(self, logger, tmp_session_dir):
        """Events are written as valid JSONL with expected fields."""
        logger.log("test_event", "test_component", key1="value1", key2=42)
        logger.log("another_event", "other", flag=True)

        debug_file = tmp_session_dir / "test1234.debug"
        assert debug_file.exists()

        lines = debug_file.read_text().strip().split("\n")
        assert len(lines) == 2

        event1 = json.loads(lines[0])
        assert event1["event"] == "test_event"
        assert event1["component"] == "test_component"
        assert event1["data"]["key1"] == "value1"
        assert event1["data"]["key2"] == 42
        assert "ts" in event1
        assert "elapsed_ms" in event1
        assert isinstance(event1["elapsed_ms"], int)

        event2 = json.loads(lines[1])
        assert event2["event"] == "another_event"
        assert event2["data"]["flag"] is True

    def test_elapsed_ms_increases(self, logger, tmp_session_dir):
        """Elapsed time increases between events."""
        import time

        logger.log("first", "test")
        time.sleep(0.05)
        logger.log("second", "test")

        debug_file = tmp_session_dir / "test1234.debug"
        lines = debug_file.read_text().strip().split("\n")
        e1 = json.loads(lines[0])
        e2 = json.loads(lines[1])

        assert e2["elapsed_ms"] >= e1["elapsed_ms"]

    def test_noop_logger_does_nothing(self, tmp_session_dir):
        """No-op logger writes no files and doesn't error."""
        noop = SessionDebugLogger.noop()
        noop.log("test_event", "test", data="should_not_write")

        # No .debug file should be created
        debug_files = list(tmp_session_dir.glob("*.debug"))
        assert len(debug_files) == 0

    def test_noop_logger_is_disabled(self):
        """No-op logger has _enabled=False."""
        noop = SessionDebugLogger.noop()
        assert noop._enabled is False

    def test_thread_safety(self, logger, tmp_session_dir):
        """Concurrent writes from multiple threads produce valid JSONL output."""
        num_threads = 10
        events_per_thread = 50
        errors = []

        def writer(thread_id):
            try:
                for i in range(events_per_thread):
                    logger.log(
                        "thread_event", "test", thread_id=thread_id, index=i
                    )
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=writer, args=(t,)) for t in range(num_threads)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        assert not errors, f"Thread errors: {errors}"

        debug_file = tmp_session_dir / "test1234.debug"
        lines = debug_file.read_text().strip().split("\n")
        assert len(lines) == num_threads * events_per_thread

        # Every line must be valid JSON
        for line in lines:
            event = json.loads(line)
            assert event["event"] == "thread_event"
            assert "thread_id" in event["data"]

    def test_data_truncation(self, logger, tmp_session_dir):
        """Large string values are truncated in log output."""
        long_string = "x" * 500
        logger.log("test", "test", big_value=long_string)

        debug_file = tmp_session_dir / "test1234.debug"
        event = json.loads(debug_file.read_text().strip())
        truncated = event["data"]["big_value"]

        assert len(truncated) < len(long_string)
        assert "500 chars" in truncated

    def test_truncate_helper(self):
        """_truncate function works correctly."""
        short = "hello"
        assert _truncate(short) == "hello"

        long = "x" * 300
        result = _truncate(long)
        assert len(result) < 300
        assert "300 chars" in result

        # Non-strings are passed through
        assert _truncate(42) == 42
        assert _truncate(None) is None
        assert _truncate(True) is True

    def test_file_path_property(self, logger, tmp_session_dir):
        """file_path returns the correct path."""
        assert logger.file_path == tmp_session_dir / "test1234.debug"

    def test_noop_file_path_is_none(self):
        """No-op logger has file_path of None."""
        noop = SessionDebugLogger.noop()
        assert noop.file_path is None

    def test_creates_parent_directory(self, tmp_path):
        """Logger creates parent directory if it doesn't exist."""
        nested_dir = tmp_path / "deeply" / "nested" / "sessions"
        logger = SessionDebugLogger(nested_dir, "newsession")
        logger.log("init", "test")

        assert nested_dir.exists()
        assert (nested_dir / "newsession.debug").exists()

    def test_non_serializable_data(self, logger, tmp_session_dir):
        """Non-serializable values are converted via default=str."""
        logger.log("test", "test", path=Path("/some/path"))

        debug_file = tmp_session_dir / "test1234.debug"
        event = json.loads(debug_file.read_text().strip())
        assert event["data"]["path"] == "/some/path"


class TestGlobalLogger:
    def test_get_debug_logger_returns_noop_by_default(self):
        """get_debug_logger returns no-op when no logger is set."""
        set_debug_logger(None)
        logger = get_debug_logger()
        assert logger._enabled is False

    def test_set_and_get_debug_logger(self, tmp_session_dir):
        """set_debug_logger / get_debug_logger round-trip works."""
        logger = SessionDebugLogger(tmp_session_dir, "global_test")
        set_debug_logger(logger)

        retrieved = get_debug_logger()
        assert retrieved is logger
        assert retrieved._enabled is True

    def test_set_none_clears_logger(self, tmp_session_dir):
        """Setting None restores no-op behavior."""
        logger = SessionDebugLogger(tmp_session_dir, "temp")
        set_debug_logger(logger)
        set_debug_logger(None)

        retrieved = get_debug_logger()
        assert retrieved._enabled is False


class TestDeleteSessionCleanup:
    def test_delete_session_removes_debug_file(self, tmp_session_dir):
        """SessionManager.delete_session removes both .json and .debug files."""
        from swecli.core.context_engineering.history import SessionManager

        sm = SessionManager(session_dir=tmp_session_dir)
        session = sm.create_session(working_directory="/tmp")

        # Add a message so save works
        from swecli.models.message import ChatMessage, Role

        sm.add_message(ChatMessage(role=Role.USER, content="test"), auto_save_interval=1)

        # Create a debug file for this session
        debug_file = tmp_session_dir / f"{session.id}.debug"
        debug_file.write_text('{"event":"test"}\n')

        session_file = tmp_session_dir / f"{session.id}.json"
        assert session_file.exists()
        assert debug_file.exists()

        sm.delete_session(session.id)

        assert not session_file.exists()
        assert not debug_file.exists()

    def test_delete_session_without_debug_file(self, tmp_session_dir):
        """delete_session works fine when no .debug file exists."""
        from swecli.core.context_engineering.history import SessionManager

        sm = SessionManager(session_dir=tmp_session_dir)
        session = sm.create_session(working_directory="/tmp")

        from swecli.models.message import ChatMessage, Role

        sm.add_message(ChatMessage(role=Role.USER, content="test"), auto_save_interval=1)

        session_file = tmp_session_dir / f"{session.id}.json"
        assert session_file.exists()

        sm.delete_session(session.id)
        assert not session_file.exists()
