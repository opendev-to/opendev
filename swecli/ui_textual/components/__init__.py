"""Shared components used across the Textual UI."""

from .box_styles import BoxStyles
from .console_animations import Spinner, FlashingSymbol, ProgressIndicator
from .tips import TipsManager
from .welcome import WelcomeMessage
from .status_line import StatusLine
from .notifications import NotificationCenter, Notification
from .task_progress import TaskProgressDisplay
from .category_selector_message import create_category_selector_message, get_category_items
from .model_selector_message import create_model_selector_message, get_model_items
from .mcp_viewer_message import create_mcp_viewer_message

__all__ = [
    "BoxStyles",
    "TipsManager",
    "WelcomeMessage",
    "Spinner",
    "FlashingSymbol",
    "ProgressIndicator",
    "StatusLine",
    "NotificationCenter",
    "Notification",
    "TaskProgressDisplay",
    "create_category_selector_message",
    "get_category_items",
    "create_model_selector_message",
    "get_model_items",
    "create_mcp_viewer_message",
]
