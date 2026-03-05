"""Context Engineering subsystem for OpenDev.

This package manages all context-related systems:
- tools/: Tool execution (implementations, handlers, LSP, symbol tools)
- history/: Session and undo management
- retrieval/: Codebase indexing and information retrieval
- memory/: Long-term learning (ACE playbook, strategies)
- mcp/: Model Context Protocol integration
"""

from swecli.core.context_engineering.tools import ToolRegistry, ToolExecutionContext
from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.core.context_engineering.history.undo_manager import UndoManager
from swecli.core.context_engineering.memory.playbook import Playbook

__all__ = [
    "ToolRegistry",
    "ToolExecutionContext",
    "SessionManager",
    "UndoManager",
    "Playbook",
]
