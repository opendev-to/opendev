"""Model Context Protocol integration for OpenDev."""

from swecli.core.context_engineering.mcp.manager import MCPManager
from swecli.core.context_engineering.mcp.models import MCPServerConfig, MCPConfig

__all__ = ["MCPManager", "MCPServerConfig", "MCPConfig"]
