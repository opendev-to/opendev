"""Tool for spawning subagents."""

from __future__ import annotations

from typing import Any, TYPE_CHECKING

if TYPE_CHECKING:
    from .manager import SubAgentManager

TASK_TOOL_NAME = "spawn_subagent"

TASK_TOOL_DESCRIPTION = """Spawn an ephemeral subagent to handle complex, multi-step tasks with isolated context.

## When to Use
- Complex tasks requiring multiple steps that can be delegated
- Tasks that benefit from isolated context (research, analysis, code review)
- Independent tasks that can run in parallel
- Tasks requiring focused reasoning or heavy token usage

## When NOT to Use
- Simple tasks that can be completed with a few tool calls
- Tasks requiring intermediate feedback or clarification
- Tasks where you need to see the reasoning process

## Available Subagent Types
{subagent_descriptions}

## Usage Notes
1. Provide a short `description` (3-5 words) summarizing what the agent will do
2. Include all context in `prompt` - subagents have no access to conversation history
3. The subagent returns a single result - you won't see intermediate steps
4. **Parallel execution**: To run subagents concurrently, make multiple spawn_subagent calls in the SAME response. The system detects this and executes them in parallel automatically. Always prefer parallel spawning for independent tasks — it maximizes performance and is what users expect when they ask for multiple agents.
5. Use `run_in_background` for long-running tasks you want to check on later
6. Use `model` to select a specific model (haiku for quick tasks, opus for complex ones)
7. Use `resume` with an agent_id to continue a previous subagent session"""


def create_task_tool_schema(manager: "SubAgentManager") -> dict[str, Any]:
    """Create the task tool schema with available subagent types.

    Args:
        manager: The SubAgentManager with registered subagents

    Returns:
        OpenAI-compatible tool schema dict
    """
    # Use get_agent_configs() which reads from ALL_SUBAGENTS directly
    # instead of get_available_types() which requires register_defaults() to be called first
    agent_configs = manager.get_agent_configs()

    available_types = [c.name for c in agent_configs]

    # Build subagent descriptions for tool description
    subagent_lines = []
    for config in agent_configs:
        subagent_lines.append(f"- **{config.name}**: {config.description}")

    subagent_descriptions = "\n".join(subagent_lines)

    return {
        "type": "function",
        "function": {
            "name": TASK_TOOL_NAME,
            "description": TASK_TOOL_DESCRIPTION.format(
                subagent_descriptions=subagent_descriptions
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "description": {
                        "type": "string",
                        "description": "A short (3-5 word) description of the task",
                    },
                    "prompt": {
                        "type": "string",
                        "description": (
                            "The task for the agent to perform. Include all context needed "
                            "since the subagent has no access to the conversation history."
                        ),
                    },
                    "subagent_type": {
                        "type": "string",
                        "description": "Type of subagent to use for this task",
                        "enum": available_types,
                    },
                    "model": {
                        "type": "string",
                        "enum": ["sonnet", "opus", "haiku"],
                        "description": (
                            "Optional model to use for this subagent. If not specified, "
                            "inherits from parent. Use 'haiku' for quick, straightforward "
                            "tasks to minimize cost and latency."
                        ),
                    },
                    "run_in_background": {
                        "type": "boolean",
                        "default": False,
                        "description": (
                            "Set to true to run this subagent in the background. The tool "
                            "result will include a task_id - use get_subagent_output to "
                            "check on output later."
                        ),
                    },
                    "resume": {
                        "type": "string",
                        "description": (
                            "Optional agent ID to resume from. If provided, the subagent "
                            "will continue from the previous execution with full context preserved."
                        ),
                    },
                },
                "required": ["description", "prompt", "subagent_type"],
            },
        },
    }


def format_task_result(result: dict[str, Any], subagent_type: str) -> str:
    """Format the task result for display.

    Args:
        result: The result from subagent execution
        subagent_type: The type of subagent that was used

    Returns:
        Formatted result string
    """
    if not result.get("success"):
        error = result.get("error", "Unknown error")
        return f"[{subagent_type}] Task failed: {error}"

    content = result.get("content", "")
    if not content:
        return f"[{subagent_type}] Task completed (no output)"

    return f"[{subagent_type}] {content}"
