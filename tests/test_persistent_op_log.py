"""Tests for persistent operation log (JSONL)."""

import json
from pathlib import Path


from swecli.core.context_engineering.history.undo_manager import UndoManager
from swecli.models.operation import Operation, OperationType


def _make_operation(
    op_type: OperationType = OperationType.FILE_EDIT, target: str = "/a.py"
) -> Operation:
    return Operation(type=op_type, target=target)


class TestPersistentOperationLog:
    """Tests for JSONL operation persistence."""

    def test_operations_saved_to_disk(self, tmp_path: Path) -> None:
        """Operations should be appended to operations.jsonl."""
        mgr = UndoManager(session_dir=tmp_path)
        mgr.record_operation(_make_operation())
        mgr.record_operation(_make_operation(target="/b.py"))

        log_file = tmp_path / "operations.jsonl"
        assert log_file.exists()

        lines = log_file.read_text().strip().splitlines()
        assert len(lines) == 2

        record = json.loads(lines[0])
        assert record["type"] == "file_edit"
        assert record["path"] == "/a.py"
        assert "timestamp" in record

    def test_load_operations(self, tmp_path: Path) -> None:
        """Should load operations from JSONL file."""
        mgr = UndoManager(session_dir=tmp_path)
        mgr.record_operation(_make_operation(OperationType.FILE_WRITE, "/new.py"))
        mgr.record_operation(_make_operation(OperationType.BASH_EXECUTE, "ls -la"))

        ops = UndoManager.load_operations(tmp_path)
        assert len(ops) == 2
        assert ops[0]["type"] == "file_write"
        assert ops[1]["type"] == "bash_execute"

    def test_missing_session_dir(self, tmp_path: Path) -> None:
        """Should create session dir if it doesn't exist."""
        nested = tmp_path / "deep" / "nested" / "dir"
        mgr = UndoManager(session_dir=nested)
        mgr.record_operation(_make_operation())

        assert nested.exists()
        assert (nested / "operations.jsonl").exists()

    def test_no_session_dir_skips_logging(self) -> None:
        """Should work fine without session_dir (no crash)."""
        mgr = UndoManager()
        mgr.record_operation(_make_operation())
        assert len(mgr.history) == 1

    def test_load_empty_log(self, tmp_path: Path) -> None:
        """Should return empty list when no log exists."""
        ops = UndoManager.load_operations(tmp_path)
        assert ops == []

    def test_corrupted_lines_skipped(self, tmp_path: Path) -> None:
        """Should skip corrupted JSON lines gracefully."""
        log_file = tmp_path / "operations.jsonl"
        log_file.write_text('{"type": "file_edit"}\nnot json\n{"type": "bash_execute"}\n')

        ops = UndoManager.load_operations(tmp_path)
        assert len(ops) == 2
