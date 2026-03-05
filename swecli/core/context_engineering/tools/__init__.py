"""Tool subsystem for OpenDev core.

This package contains:
- implementations/: Low-level tool implementations (BashTool, EditTool, etc.)
- handlers/: High-level handlers that wrap implementations and add orchestration logic
- registry.py: ToolRegistry that dispatches tool calls to handlers
- context.py: ToolExecutionContext for passing dependencies to handlers
"""

from .context import ToolExecutionContext
from .registry import ToolRegistry

# Re-export implementations for convenience
from .implementations import (
    BaseTool,
    BashTool,
    Diff,
    DiffPreview,
    EditTool,
    FileOperations,
    OpenBrowserTool,
    VLMTool,
    WebFetchTool,
    WebScreenshotTool,
    WriteTool,
)

# Re-export handlers for convenience
from .handlers import (
    FileToolHandler,
    ProcessToolHandler,
    ScreenshotToolHandler,
    TodoHandler,
    TodoItem,
    WebToolHandler,
)

__all__ = [
    # Core
    "ToolExecutionContext",
    "ToolRegistry",
    # Implementations
    "BaseTool",
    "BashTool",
    "Diff",
    "DiffPreview",
    "EditTool",
    "FileOperations",
    "OpenBrowserTool",
    "VLMTool",
    "WebFetchTool",
    "WebScreenshotTool",
    "WriteTool",
    # Handlers
    "FileToolHandler",
    "ProcessToolHandler",
    "ScreenshotToolHandler",
    "TodoHandler",
    "TodoItem",
    "WebToolHandler",
]
