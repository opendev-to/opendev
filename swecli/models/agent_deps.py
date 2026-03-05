"""Agent dependencies for Pydantic AI tools."""

from pathlib import Path
from typing import Any

from pydantic import BaseModel, ConfigDict


class AgentDependencies(BaseModel):
    """Dependencies passed to agent tools via RunContext.

    These dependencies provide access to OpenDev managers and state
    that tools need to execute operations properly.
    """

    model_config = ConfigDict(arbitrary_types_allowed=True)

    # Core managers
    mode_manager: Any  # ModeManager
    approval_manager: Any  # ApprovalManager
    undo_manager: Any  # UndoManager
    session_manager: Any  # SessionManager

    # Environment
    working_dir: Path
    console: Any  # Rich Console

    # Config
    config: Any  # AppConfig
