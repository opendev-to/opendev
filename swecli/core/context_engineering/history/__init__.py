"""History management for OpenDev.

Manages session state and undo/redo functionality.
"""

from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.core.context_engineering.history.topic_detector import TopicDetector
from swecli.core.context_engineering.history.undo_manager import UndoManager

__all__ = [
    "SessionManager",
    "TopicDetector",
    "UndoManager",
]
