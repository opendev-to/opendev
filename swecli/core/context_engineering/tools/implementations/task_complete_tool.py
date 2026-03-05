"""Task completion tool for explicit ReAct loop termination."""

from __future__ import annotations

from typing import Any, Literal


class TaskCompleteTool:
    """Tool that signals explicit task completion.

    Instead of relying on implicit termination (no tool calls = done),
    agents must call this tool to end the ReAct loop. This provides:
    - Explicit completion signal (no ambiguity)
    - Required summary of what was accomplished
    - Natural error recovery (agent keeps trying until this is called)
    - Clean conversation history (no fake messages injected)
    """

    @property
    def name(self) -> str:
        return "task_complete"

    @property
    def description(self) -> str:
        return (
            "Call this tool when you have completed the user's request. "
            "You MUST call this tool to end the conversation. "
            "Provide a summary of what was accomplished."
        )

    def execute(
        self,
        summary: str,
        status: Literal["success", "partial", "failed"] = "success",
    ) -> dict[str, Any]:
        """Execute the task_complete tool.

        Args:
            summary: Summary of what was accomplished (required)
            status: Completion status - success, partial, or failed

        Returns:
            Result dict with _completion flag for loop termination
        """
        if not summary or not summary.strip():
            return {
                "success": False,
                "error": "Summary is required for task_complete",
                "output": None,
            }

        valid_statuses = {"success", "partial", "failed"}
        if status not in valid_statuses:
            return {
                "success": False,
                "error": f"Invalid status '{status}'. Must be one of: {valid_statuses}",
                "output": None,
            }

        return {
            "success": True,
            "_completion": True,  # Special flag for loop termination
            "summary": summary.strip(),
            "status": status,
            "output": f"Task completed ({status}): {summary.strip()}",
        }
