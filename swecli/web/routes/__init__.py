"""API routes for web UI."""

from swecli.web.routes.chat import router as chat_router
from swecli.web.routes.sessions import router as sessions_router
from swecli.web.routes.config import router as config_router
from swecli.web.routes.commands import router as commands_router
from swecli.web.routes.mcp import router as mcp_router

__all__ = ["chat_router", "sessions_router", "config_router", "commands_router", "mcp_router"]
