"""Tests for BatchTool parallel and serial execution."""

import time
from unittest.mock import MagicMock

import pytest

from swecli.core.context_engineering.tools.implementations.batch_tool import (
    MAX_PARALLEL_WORKERS,
    BatchTool,
)


class TestBatchTool:
    """Test batch tool execution modes."""

    @pytest.fixture()
    def batch_tool(self) -> BatchTool:
        return BatchTool()

    @pytest.fixture()
    def mock_registry(self) -> MagicMock:
        registry = MagicMock()
        registry.execute_tool.return_value = {"success": True, "output": "content"}
        return registry

    def test_parallel_execution(self, batch_tool: BatchTool, mock_registry: MagicMock) -> None:
        """Multiple invocations should execute concurrently."""
        invocations = [
            {"tool": "read_file", "input": {"file_path": f"/path/file_{i}.py"}} for i in range(3)
        ]
        result = batch_tool.execute(invocations, mode="parallel", tool_registry=mock_registry)
        assert result["success"]
        assert len(result["results"]) == 3
        assert mock_registry.execute_tool.call_count == 3

    def test_serial_execution(self, batch_tool: BatchTool, mock_registry: MagicMock) -> None:
        """Serial mode should execute in order."""
        invocations = [
            {"tool": "read_file", "input": {"file_path": "/a.py"}},
            {"tool": "read_file", "input": {"file_path": "/b.py"}},
        ]
        result = batch_tool.execute(invocations, mode="serial", tool_registry=mock_registry)
        assert result["success"]
        assert len(result["results"]) == 2

    def test_results_preserve_order(self, batch_tool: BatchTool) -> None:
        """Results should match input order regardless of execution time."""
        registry = MagicMock()

        def slow_then_fast(tool_name: str, tool_input: dict, **kw: object) -> dict:
            if tool_input.get("file_path") == "/slow.py":
                time.sleep(0.05)
            return {"success": True, "output": tool_input.get("file_path", "")}

        registry.execute_tool.side_effect = slow_then_fast

        invocations = [
            {"tool": "read_file", "input": {"file_path": "/slow.py"}},
            {"tool": "read_file", "input": {"file_path": "/fast.py"}},
        ]
        result = batch_tool.execute(invocations, mode="parallel", tool_registry=registry)

        assert result["success"]
        assert result["results"][0]["output"] == "/slow.py"
        assert result["results"][1]["output"] == "/fast.py"

    def test_partial_failure(self, batch_tool: BatchTool) -> None:
        """One failed invocation should not block others."""
        registry = MagicMock()

        def mixed_results(tool_name: str, tool_input: dict, **kw: object) -> dict:
            if tool_input.get("file_path") == "/bad.py":
                raise FileNotFoundError("Not found")
            return {"success": True, "output": "ok"}

        registry.execute_tool.side_effect = mixed_results

        invocations = [
            {"tool": "read_file", "input": {"file_path": "/good.py"}},
            {"tool": "read_file", "input": {"file_path": "/bad.py"}},
            {"tool": "read_file", "input": {"file_path": "/also_good.py"}},
        ]
        result = batch_tool.execute(invocations, mode="parallel", tool_registry=registry)

        assert result["success"]  # Overall batch succeeds
        assert result["results"][0]["success"]
        assert not result["results"][1]["success"]
        assert "Not found" in result["results"][1]["output"]
        assert result["results"][2]["success"]

    def test_empty_invocations(self, batch_tool: BatchTool, mock_registry: MagicMock) -> None:
        """Empty list should return empty results."""
        result = batch_tool.execute([], mode="parallel", tool_registry=mock_registry)
        assert result["success"]
        assert result["results"] == []
        assert mock_registry.execute_tool.call_count == 0

    def test_max_5_parallel_workers(self) -> None:
        """MAX_PARALLEL_WORKERS should be 5."""
        assert MAX_PARALLEL_WORKERS == 5

    def test_no_registry_returns_error(self, batch_tool: BatchTool) -> None:
        """Missing tool_registry should return error."""
        result = batch_tool.execute(
            [{"tool": "read_file", "input": {}}],
            mode="parallel",
            tool_registry=None,
        )
        assert not result["success"]
        assert "tool_registry" in result["error"]
