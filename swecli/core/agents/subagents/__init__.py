"""SubAgent infrastructure for task delegation."""

from .specs import SubAgentSpec, CompiledSubAgent
from .manager import SubAgentManager
from .task_tool import create_task_tool_schema, TASK_TOOL_NAME
from .agents import ALL_SUBAGENTS

__all__ = [
    "SubAgentSpec",
    "CompiledSubAgent",
    "SubAgentManager",
    "create_task_tool_schema",
    "TASK_TOOL_NAME",
    "ALL_SUBAGENTS",
]
