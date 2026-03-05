"""Unit tests for the task_complete tool."""

import pytest

from swecli.core.context_engineering.tools.implementations.task_complete_tool import TaskCompleteTool


class TestTaskCompleteTool:
    """Test cases for TaskCompleteTool."""

    def setup_method(self):
        """Set up test fixtures."""
        self.tool = TaskCompleteTool()

    def test_tool_name(self):
        """Verify tool has correct name."""
        assert self.tool.name == "task_complete"

    def test_tool_description(self):
        """Verify tool has a description."""
        assert self.tool.description
        assert "task_complete" in self.tool.description.lower() or "complete" in self.tool.description.lower()

    def test_execute_success(self):
        """Verify tool returns completion signal on success."""
        result = self.tool.execute(summary="Task done successfully", status="success")

        assert result["success"] is True
        assert result["_completion"] is True
        assert result["summary"] == "Task done successfully"
        assert result["status"] == "success"
        assert "output" in result

    def test_execute_partial(self):
        """Verify tool handles partial completion status."""
        result = self.tool.execute(summary="Partial work done", status="partial")

        assert result["success"] is True
        assert result["_completion"] is True
        assert result["status"] == "partial"

    def test_execute_failed(self):
        """Verify tool handles failed status."""
        result = self.tool.execute(summary="Could not complete", status="failed")

        assert result["success"] is True  # Tool execution succeeded
        assert result["_completion"] is True
        assert result["status"] == "failed"

    def test_execute_empty_summary_fails(self):
        """Verify tool rejects empty summary."""
        result = self.tool.execute(summary="", status="success")

        assert result["success"] is False
        assert "error" in result
        assert "summary" in result["error"].lower()

    def test_execute_whitespace_only_summary_fails(self):
        """Verify tool rejects whitespace-only summary."""
        result = self.tool.execute(summary="   ", status="success")

        assert result["success"] is False
        assert "error" in result

    def test_execute_invalid_status(self):
        """Verify tool rejects invalid status values."""
        result = self.tool.execute(summary="Done", status="invalid_status")

        assert result["success"] is False
        assert "error" in result
        assert "status" in result["error"].lower() or "invalid" in result["error"].lower()

    def test_execute_default_status(self):
        """Verify default status is success."""
        result = self.tool.execute(summary="Done")

        assert result["success"] is True
        assert result["status"] == "success"

    def test_summary_is_trimmed(self):
        """Verify summary whitespace is trimmed."""
        result = self.tool.execute(summary="  Task done  ", status="success")

        assert result["summary"] == "Task done"

    def test_output_contains_summary(self):
        """Verify output contains the summary text."""
        result = self.tool.execute(summary="Important task done", status="success")

        assert "Important task done" in result["output"]
        assert "success" in result["output"]


class TestTaskCompleteToolIntegration:
    """Integration tests for task_complete with tool registry."""

    def test_tool_registered_in_registry(self):
        """Verify task_complete is registered in tool registry."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()

        assert "task_complete" in registry._handlers

    def test_execute_via_registry(self):
        """Verify task_complete can be executed via registry."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        result = registry.execute_tool(
            "task_complete",
            {"summary": "Registry test complete", "status": "success"},
        )

        assert result["success"] is True
        assert result["_completion"] is True
        assert result["summary"] == "Registry test complete"

    def test_tool_in_plan_mode_allowed(self):
        """Verify task_complete is allowed in plan mode."""
        from swecli.core.agents.components import PLANNING_TOOLS

        assert "task_complete" in PLANNING_TOOLS
