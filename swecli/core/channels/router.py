"""Message router for multi-channel agent.

Routes inbound messages from channels to sessions and dispatches responses
back to the correct channel/user.
"""

import asyncio
import logging
from typing import Any, Callable, Optional

from swecli.core.channels.base import ChannelAdapter, InboundMessage, OutboundMessage
from swecli.core.channels.reset_policies import should_reset_session
from swecli.core.channels.workspace_selector import WorkspaceSelector
from swecli.core.context_engineering.history.session_manager import SessionManager
from swecli.models.message import ChatMessage, InputProvenance, Role
from swecli.models.session import Session

logger = logging.getLogger(__name__)


class MessageRouter:
    """Routes inbound messages from channels to agent sessions.

    The router is the central coordinator for multi-channel messaging:
    1. Receives InboundMessage from channel adapters
    2. Resolves or creates appropriate session (by channel+user)
    3. Handles workspace selection for new channel users
    4. Dispatches messages to the agent for processing
    5. Routes agent responses back to the correct channel

    Example:
        router = MessageRouter(
            session_manager=session_manager,
            agent_executor=agent.run,
        )

        # Register channel adapters
        router.register_adapter(telegram_adapter)
        router.register_adapter(whatsapp_adapter)

        # Handle incoming message
        await router.handle_inbound(message)
    """

    def __init__(
        self,
        session_manager: SessionManager,
        agent_executor: Optional[Callable[[Session, str], Any]] = None,
    ):
        """Initialize message router.

        Args:
            session_manager: Session manager for session CRUD
            agent_executor: Optional callable to execute agent
                           (session, message_text) -> response_text
        """
        self._session_manager = session_manager
        self._agent_executor = agent_executor
        self._workspace_selector = WorkspaceSelector(session_manager)

        # Track adapters by channel name
        self._adapters: dict[str, ChannelAdapter] = {}

        # Track sessions awaiting workspace selection
        # (channel, user_id, thread_id) -> (Session, InboundMessage, available_workspaces)
        self._pending_workspace_selection: dict[
            tuple[str, str, Optional[str]], tuple[Session, InboundMessage, list[str]]
        ] = {}

    def register_adapter(self, adapter: ChannelAdapter) -> None:
        """Register a channel adapter for routing.

        Args:
            adapter: Channel adapter to register
        """
        self._adapters[adapter.channel_name] = adapter
        logger.info(f"Registered channel adapter: {adapter.channel_name}")

    def get_adapter(self, channel_name: str) -> Optional[ChannelAdapter]:
        """Get registered adapter by channel name.

        Args:
            channel_name: Channel name

        Returns:
            Adapter if found, None otherwise
        """
        return self._adapters.get(channel_name)

    async def handle_inbound(self, message: InboundMessage) -> None:
        """Route inbound message to correct session and agent.

        This is the main entry point for processing messages from channels.

        Args:
            message: Inbound message from a channel
        """
        logger.info(
            f"Routing message from {message.channel}:{message.user_id} "
            f"(thread={message.thread_id})"
        )

        # Get channel adapter
        adapter = self.get_adapter(message.channel)
        if not adapter:
            logger.error(f"No adapter registered for channel: {message.channel}")
            return

        # Resolve or create session
        session = self._resolve_session(message)

        # Check if session should be reset based on channel policy
        if should_reset_session(session):
            logger.info(
                f"Session {session.id} for {message.channel}:{message.user_id} "
                f"expired per channel policy - creating new session"
            )
            # Create new session (will trigger workspace selection)
            delivery_context = {
                "channel": message.channel,
                "user_id": message.user_id,
                "thread_id": message.thread_id,
                **message.metadata,
            }
            session = self._session_manager.create_session(
                working_directory=None,
                channel=message.channel,
                channel_user_id=message.user_id,
                chat_type=message.chat_type,
                thread_id=message.thread_id,
                delivery_context=delivery_context,
                workspace_confirmed=False,
            )

        # Check if workspace selection is pending
        if not session.workspace_confirmed:
            await self._handle_workspace_selection(message, session, adapter)
            return

        # Convert to ChatMessage with provenance tracking
        chat_message = ChatMessage(
            role=Role.USER,
            content=message.text,
            metadata={
                "channel": message.channel,
                "user_id": message.user_id,
                "thread_id": message.thread_id,
                "timestamp": message.timestamp.isoformat(),
            },
            provenance=InputProvenance(
                kind="external_user",
                source_channel=message.channel,
                source_session_id=None,  # Not forwarded from another session
            ),
        )

        # Add to session
        session.add_message(chat_message)
        session.last_activity = message.timestamp

        # Dispatch to agent (if executor provided)
        if self._agent_executor:
            try:
                response = await self._dispatch_to_agent(session, message.text)

                # Send response back via channel
                delivery_context = session.delivery_context
                outbound = OutboundMessage(
                    text=response,
                    thread_id=message.thread_id,
                    reply_to_message_id=message.reply_to_message_id,
                )
                await adapter.send(delivery_context, outbound)

            except Exception as e:
                logger.error(f"Error dispatching to agent: {e}", exc_info=True)
                # Send error message to user
                error_msg = OutboundMessage(
                    text=f"⚠️ Sorry, I encountered an error: {str(e)[:200]}",
                    thread_id=message.thread_id,
                )
                await adapter.send(session.delivery_context, error_msg)

        # Save session
        self._session_manager.save_session(session)

    async def _handle_workspace_selection(
        self, message: InboundMessage, session: Session, adapter: ChannelAdapter
    ) -> None:
        """Handle workspace selection flow.

        Args:
            message: Inbound message
            session: Session awaiting workspace selection
            adapter: Channel adapter to send prompts
        """
        # Key for tracking pending workspace selections
        pending_key = (message.channel, message.user_id, message.thread_id)

        # Check if this is a response to workspace prompt
        if pending_key in self._pending_workspace_selection:
            pending_session, original_msg, available_workspaces = (
                self._pending_workspace_selection[pending_key]
            )

            # Use the pending session (has consistent ID)
            session = pending_session

            # Parse user's choice
            workspace = self._workspace_selector.parse_workspace_choice(
                message.text, available_workspaces
            )

            if workspace:
                # Valid workspace selected
                session.working_directory = workspace
                session.workspace_confirmed = True
                self._session_manager.save_session(session)

                # Remove from pending
                del self._pending_workspace_selection[pending_key]

                # Send confirmation
                confirm_msg = OutboundMessage(
                    text=f"✅ Workspace set to: `{workspace}`\n\nHow can I help you today?",
                    parse_mode="markdown",
                )
                await adapter.send(session.delivery_context, confirm_msg)

                logger.info(
                    f"Workspace confirmed for {message.channel}:{message.user_id} -> {workspace}"
                )

                # Process original message if it wasn't just a workspace selection
                if original_msg.text and original_msg.text != message.text:
                    await self.handle_inbound(original_msg)

            else:
                # Invalid choice - reprompt
                error_msg = OutboundMessage(
                    text=(
                        "❌ Invalid workspace selection.\n\n"
                        "Please enter a number from the list or provide a valid project path."
                    )
                )
                await adapter.send(session.delivery_context, error_msg)

                # Reprompt
                workspaces = self._workspace_selector.get_available_workspaces()
                await self._workspace_selector.prompt_workspace_selection(
                    adapter, session.delivery_context, workspaces
                )

        else:
            # First message - prompt for workspace
            workspaces = self._workspace_selector.get_available_workspaces()
            self._pending_workspace_selection[pending_key] = (session, message, workspaces)

            await self._workspace_selector.prompt_workspace_selection(
                adapter, session.delivery_context, workspaces
            )

            logger.info(
                f"Prompting workspace selection for {message.channel}:{message.user_id}"
            )

    def _resolve_session(self, message: InboundMessage) -> Session:
        """Find or create session for a message.

        Args:
            message: Inbound message

        Returns:
            Session (existing or newly created)
        """
        # Try to find existing session
        metadata = self._session_manager.find_session_by_channel_user(
            channel=message.channel,
            user_id=message.user_id,
            thread_id=message.thread_id,
        )

        if metadata:
            # Load existing session
            session = self._session_manager.load_session(metadata.id)
            logger.debug(f"Found existing session: {metadata.id}")
            return session

        # Create new session
        delivery_context = {
            "channel": message.channel,
            "user_id": message.user_id,
            "thread_id": message.thread_id,
            **message.metadata,
        }

        session = self._session_manager.create_session(
            working_directory=None,  # Will be set after workspace selection
            channel=message.channel,
            channel_user_id=message.user_id,
            chat_type=message.chat_type,
            thread_id=message.thread_id,
            delivery_context=delivery_context,
            workspace_confirmed=False,  # Requires workspace selection
        )

        logger.info(
            f"Created new session {session.id} for {message.channel}:{message.user_id}"
        )
        return session

    async def _dispatch_to_agent(self, session: Session, message_text: str) -> str:
        """Dispatch message to agent for processing.

        Args:
            session: Session context
            message_text: User message text

        Returns:
            Agent response text
        """
        if not self._agent_executor:
            return "Agent executor not configured"

        # Execute agent (sync or async)
        if asyncio.iscoroutinefunction(self._agent_executor):
            response = await self._agent_executor(session, message_text)
        else:
            # Run sync executor in thread pool
            loop = asyncio.get_event_loop()
            response = await loop.run_in_executor(
                None, self._agent_executor, session, message_text
            )

        return str(response)
