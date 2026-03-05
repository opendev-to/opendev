"""Template variable registry for prompt composition."""

from dataclasses import dataclass
from typing import Any, Dict, Optional


@dataclass
class ToolVariable:
    """Represents a tool in template variables."""

    name: str
    description: Optional[str] = None


@dataclass
class SystemReminderVariable:
    """System reminder context."""

    planFilePath: str
    planExists: bool


class PromptVariables:
    """Registry of template variables for prompt interpolation."""

    def __init__(self):
        # Tool references
        self.EDIT_TOOL = ToolVariable("edit_file")
        self.WRITE_TOOL = ToolVariable("write_file")
        self.READ_TOOL = ToolVariable("read_file")
        self.BASH_TOOL = ToolVariable("run_command")
        self.GLOB_TOOL = ToolVariable("list_files")
        self.GREP_TOOL = ToolVariable("search")
        self.PRESENT_PLAN_TOOL = ToolVariable("present_plan")
        self.ASK_USER_QUESTION_TOOL_NAME = "ask_user"

        # Tool name shortcuts (for reminder templates)
        self.GLOB_TOOL_NAME = "list_files"
        self.GREP_TOOL_NAME = "search"
        self.READ_TOOL_NAME = "read_file"

        # Agent configuration
        self.EXPLORE_AGENT_COUNT = 3
        self.PLAN_AGENT_COUNT = 1
        self.EXPLORE_AGENT_VARIANT = "enabled"  # or "disabled"

        # Subagent references
        self.EXPLORE_SUBAGENT = ToolVariable("Explore", "Explore agent")
        self.PLAN_SUBAGENT = ToolVariable("Plan", "Plan agent")

    def get_system_reminder(self, plan_path: str) -> SystemReminderVariable:
        """Get system reminder context.

        Args:
            plan_path: Path to plan file

        Returns:
            SystemReminderVariable with plan file context
        """
        import os

        plan_exists = os.path.exists(plan_path)
        return SystemReminderVariable(planFilePath=plan_path, planExists=plan_exists)

    def to_dict(self, **runtime_vars: Any) -> Dict[str, Any]:
        """Export variables for template interpolation.

        Args:
            **runtime_vars: Additional runtime variables to include

        Returns:
            Dictionary of all variables for template rendering
        """
        base = {
            "EDIT_TOOL": self.EDIT_TOOL,
            "WRITE_TOOL": self.WRITE_TOOL,
            "READ_TOOL": self.READ_TOOL,
            "BASH_TOOL": self.BASH_TOOL,
            "GLOB_TOOL": self.GLOB_TOOL,
            "GREP_TOOL": self.GREP_TOOL,
            "PRESENT_PLAN_TOOL": self.PRESENT_PLAN_TOOL,
            "ASK_USER_QUESTION_TOOL_NAME": self.ASK_USER_QUESTION_TOOL_NAME,
            "GLOB_TOOL_NAME": self.GLOB_TOOL_NAME,
            "GREP_TOOL_NAME": self.GREP_TOOL_NAME,
            "READ_TOOL_NAME": self.READ_TOOL_NAME,
            "EXPLORE_AGENT_COUNT": self.EXPLORE_AGENT_COUNT,
            "PLAN_AGENT_COUNT": self.PLAN_AGENT_COUNT,
            "EXPLORE_AGENT_VARIANT": self.EXPLORE_AGENT_VARIANT,
            "EXPLORE_SUBAGENT": self.EXPLORE_SUBAGENT,
            "PLAN_SUBAGENT": self.PLAN_SUBAGENT,
        }
        base.update(runtime_vars)
        return base
