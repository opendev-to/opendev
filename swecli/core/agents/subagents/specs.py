"""SubAgent specification types."""

from typing import Any, NotRequired, TypedDict, TYPE_CHECKING

if TYPE_CHECKING:
    from swecli.core.docker.deployment import DockerConfig


class SubAgentSpec(TypedDict):
    """Specification for defining a subagent.

    Subagents are ephemeral agents that handle isolated tasks.
    They receive a task description, execute with their own context,
    and return a single result.
    """

    name: str
    """Unique identifier for the subagent type."""

    description: str
    """Human-readable description of what this subagent does.
    Used in the task tool's enum description."""

    system_prompt: str
    """System prompt that defines the subagent's behavior and role."""

    tools: NotRequired[list[str]]
    """List of tool names this subagent has access to.
    If not specified, inherits all tools from the main agent."""

    model: NotRequired[str]
    """Override model for this subagent.
    If not specified, uses the same model as the main agent."""

    docker_config: NotRequired["DockerConfig"]
    """Optional Docker configuration for this subagent.
    If specified, the subagent will execute in a Docker container.
    Falls back to local execution if Docker is unavailable."""

    copy_back_recursive: NotRequired[bool]
    """If True, copy entire workspace tree from Docker to local after completion.
    Uses docker cp for recursive directory copy. Defaults to True for Docker subagents."""


class CompiledSubAgent(TypedDict):
    """A pre-compiled subagent ready for execution.

    Created from a SubAgentSpec with all dependencies resolved.
    """

    name: str
    """The subagent's identifier."""

    description: str
    """Description for UI/documentation."""

    agent: Any
    """The instantiated agent object (MainAgent or similar)."""

    tool_names: list[str]
    """List of tools this subagent can use."""
