"""Web-based ask-user manager for WebSocket clients."""

from __future__ import annotations

import asyncio
import threading
import uuid
from typing import Any, Dict, List, Optional

from swecli.web.state import get_state
from swecli.web.logging_config import logger


class WebAskUserManager:
    """Ask-user manager for web UI that uses WebSocket for question prompts."""

    def __init__(
        self,
        ws_manager: Any,
        loop: asyncio.AbstractEventLoop,
        session_id: Optional[str] = None,
    ):
        """Initialize web ask-user manager.

        Args:
            ws_manager: WebSocket manager for broadcasting
            loop: Event loop for async operations
            session_id: Session ID for scoping broadcasts
        """
        self.ws_manager = ws_manager
        self.loop = loop
        self.session_id = session_id
        self.state = get_state()

    def prompt_user(self, questions: List[Any]) -> Optional[Dict[str, Any]]:
        """Prompt user with questions via WebSocket.

        This is called from a sync context (agent thread), so we need to
        schedule the async broadcast and wait for response.

        Args:
            questions: List of Question dataclass objects

        Returns:
            Dictionary mapping question index to selected answer(s),
            or None if cancelled/timeout
        """
        request_id = str(uuid.uuid4())

        # Serialize questions for JSON transport
        serialized_questions = []
        for q in questions:
            serialized_options = []
            for opt in q.options:
                serialized_options.append({
                    "label": opt.label,
                    "description": opt.description,
                })
            serialized_questions.append({
                "question": q.question,
                "header": q.header,
                "options": serialized_options,
                "multi_select": q.multi_select,
            })

        ask_user_request = {
            "request_id": request_id,
            "questions": serialized_questions,
            "session_id": self.session_id,
        }

        done_event = threading.Event()

        # Store pending request in shared state (with event for wake-up)
        self.state.add_pending_ask_user(
            request_id, ask_user_request, session_id=self.session_id, event=done_event
        )

        # Broadcast ask-user request via WebSocket
        logger.info(f"Requesting ask-user: {request_id} ({len(questions)} questions)")
        future = asyncio.run_coroutine_threadsafe(
            self.ws_manager.broadcast({
                "type": "ask_user_required",
                "data": ask_user_request,
            }),
            self.loop,
        )

        try:
            future.result(timeout=5)
            logger.info(f"Ask-user request broadcasted: {request_id}")
        except Exception as e:
            logger.error(f"Failed to broadcast ask-user request: {e}")
            self.state.clear_ask_user(request_id)
            return None

        # Wait for response using Event (no polling)
        wait_timeout = 300
        logger.info(f"Waiting for ask-user response (timeout: {wait_timeout}s)...")

        if not done_event.wait(timeout=wait_timeout):
            # Timeout
            logger.warning(f"Ask-user request {request_id} timed out after {wait_timeout}s")
            self.state.clear_ask_user(request_id)
            return None

        # Event was set — read result
        pending = self.state.get_pending_ask_user(request_id)
        answers = pending["answers"]
        cancelled = pending["cancelled"]
        self.state.clear_ask_user(request_id)
        if cancelled:
            logger.info(f"Ask-user {request_id} cancelled by user")
            return None
        logger.info(f"Ask-user {request_id} resolved with answers: {answers}")
        return answers
