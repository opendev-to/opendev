"""Protocols and interface abstractions for OpenDev core components."""

from .agent_interface import AgentInterface
from .approval_interface import ApprovalManagerInterface
from .config_interface import ConfigManagerInterface
from .session_interface import SessionManagerInterface
from .tool_interface import ToolInterface, ToolRegistryInterface

__all__ = [
    "AgentInterface",
    "ApprovalManagerInterface",
    "ConfigManagerInterface",
    "SessionManagerInterface",
    "ToolInterface",
    "ToolRegistryInterface",
]
