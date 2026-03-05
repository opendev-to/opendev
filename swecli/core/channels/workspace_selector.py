"""Workspace selection for channel sessions.

When a user first messages the agent via a channel (Telegram, WhatsApp, etc.),
they need to select which workspace/project the agent should work in.

This module handles the workspace selection flow.
"""

import logging
from pathlib import Path
from typing import Optional

from swecli.core.channels.base import ChannelAdapter, OutboundMessage
from swecli.core.context_engineering.history.session_manager import SessionManager

logger = logging.getLogger(__name__)


class WorkspaceSelector:
    """Handles workspace selection for channel sessions.

    When a user first contacts the agent via a channel, they need to choose
    which workspace (project directory) the agent should operate in.

    This class manages the selection flow:
    1. List available workspaces (from existing sessions)
    2. Present options to the user
    3. Parse user's choice (number or path)
    4. Validate and set the workspace

    Example:
        selector = WorkspaceSelector(session_manager)

        # Prompt user for workspace
        await selector.prompt_workspace_selection(adapter, delivery_context)

        # Later, when user responds
        workspace = selector.parse_workspace_choice(user_input, available_workspaces)
        if workspace:
            session.working_directory = workspace
            session.workspace_confirmed = True
    """

    def __init__(self, session_manager: SessionManager):
        """Initialize workspace selector.

        Args:
            session_manager: Session manager to query available workspaces
        """
        self._session_manager = session_manager

    async def prompt_workspace_selection(
        self,
        adapter: ChannelAdapter,
        delivery_context: dict,
        workspaces: Optional[list[str]] = None,
    ) -> None:
        """Prompt user to select a workspace.

        Args:
            adapter: Channel adapter to send prompt through
            delivery_context: Where to send the prompt
            workspaces: Optional list of workspaces (if None, will query from session manager)
        """
        if workspaces is None:
            workspaces = self._session_manager.list_user_workspaces()

        if not workspaces:
            # No existing workspaces - ask for path
            message = OutboundMessage(
                text=(
                    "👋 Welcome! I'm your AI coding assistant.\n\n"
                    "To get started, please tell me the full path to your project directory.\n\n"
                    "Example: `/Users/you/projects/myapp` or `/home/user/code/webapp`"
                ),
                parse_mode="markdown",
            )
        else:
            # Present workspace options
            options = "\n".join(f"{i+1}. `{ws}`" for i, ws in enumerate(workspaces))
            message = OutboundMessage(
                text=(
                    "👋 Welcome! I'm your AI coding assistant.\n\n"
                    f"I found {len(workspaces)} workspace(s) with existing sessions. "
                    "Which would you like to work in?\n\n"
                    f"{options}\n\n"
                    f"Reply with a number (1-{len(workspaces)}) or provide a new project path."
                ),
                parse_mode="markdown",
            )

        await adapter.send(delivery_context, message)

    def parse_workspace_choice(
        self, user_input: str, available_workspaces: list[str]
    ) -> Optional[str]:
        """Parse user's workspace selection.

        Args:
            user_input: User's response (number or path)
            available_workspaces: List of available workspace paths

        Returns:
            Selected workspace path if valid, None if invalid
        """
        user_input = user_input.strip()

        # Try parsing as number selection
        try:
            choice_num = int(user_input)
            if 1 <= choice_num <= len(available_workspaces):
                return available_workspaces[choice_num - 1]
        except ValueError:
            pass

        # Try parsing as direct path
        path = Path(user_input).expanduser().resolve()
        if path.exists() and path.is_dir():
            return str(path)

        # Invalid choice
        return None

    def get_available_workspaces(self) -> list[str]:
        """Get list of available workspaces.

        Returns:
            List of workspace paths
        """
        return self._session_manager.list_user_workspaces()
