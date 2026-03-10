"""Chat and query API endpoints."""

from __future__ import annotations

import asyncio
import queue as queue_mod
from typing import Dict, List

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from opendev.models.api import (
    MessageResponse,
    tool_call_to_response as tool_call_to_info,
)
from opendev.models.message import ChatMessage, Role
from opendev.models.user import User
from opendev.web.dependencies.auth import require_authenticated_user
from opendev.web.protocol import WSMessageType, ws_message
from opendev.web.state import get_state

router = APIRouter(
    prefix="/api/chat",
    tags=["chat"],
    dependencies=[Depends(require_authenticated_user)],
)


class QueryRequest(BaseModel):
    """Request model for sending a query."""

    message: str
    sessionId: str | None = None


@router.post("/query", status_code=202)
async def send_query(
    request: QueryRequest,
    user: User = Depends(require_authenticated_user),
) -> Dict[str, str]:
    """Send a query to the AI agent and execute it asynchronously.

    Results stream back via WebSocket. Returns 202 Accepted immediately.

    Args:
        request: Query request with message and optional session ID.
        user: Authenticated user (injected by dependency).

    Returns:
        Accepted status with the resolved session_id.

    Raises:
        HTTPException: 400 if message is empty or no session is active,
                       404 if the given session_id does not exist,
                       409 if the agent is running and the injection queue is full,
                       500 on unexpected errors.
    """
    # Inline imports follow the established pattern in websocket.py/_handle_query
    # to avoid module-load-order issues between sibling web modules.
    from opendev.web.agent_executor import AgentExecutor
    from opendev.web.websocket import ws_manager as global_ws_manager

    try:
        state = get_state()

        message = request.message.strip()
        if not message:
            raise HTTPException(status_code=400, detail="Message cannot be empty.")

        # Resolve session: prefer the request-supplied id, fall back to the
        # currently active session.
        session_id = request.sessionId or state.get_current_session_id()
        if not session_id:
            raise HTTPException(
                status_code=400,
                detail="No active session. Create a session first.",
            )

        # Bridge mode: the TUI owns execution — delegate to its injector.
        if state.is_bridge_mode:
            try:
                state.tui_message_injector(message, session_id)
            except Exception as exc:
                raise HTTPException(
                    status_code=500,
                    detail=f"Bridge mode injection failed: {exc}",
                ) from exc
            return {"status": "accepted", "session_id": session_id}

        # Running session: inject into the agent's live loop rather than starting
        # a second execution for the same session.
        if state.is_session_running(session_id):
            injection_queue = state.get_injection_queue(session_id)
            try:
                injection_queue.put_nowait(message)
            except queue_mod.Full:
                raise HTTPException(
                    status_code=409,
                    detail="Agent is busy; injection queue is full. Try again shortly.",
                )
            await global_ws_manager.broadcast(
                ws_message(
                    WSMessageType.USER_MESSAGE,
                    role="user",
                    content=message,
                    session_id=session_id,
                    injected=True,
                )
            )
            return {"status": "accepted", "session_id": session_id}

        # Load the session object without mutating session_manager.current_session,
        # mirroring the non-mutating read used by websocket._handle_query.
        try:
            session = state.session_manager.get_session_by_id(session_id)
        except FileNotFoundError:
            # Session may be newly created but not yet flushed to disk.
            current = state.session_manager.get_current_session()
            if current and current.id == session_id:
                session = current
            else:
                raise HTTPException(
                    status_code=404,
                    detail=f"Session {session_id!r} not found.",
                )

        # Persist user message before dispatching so the agent loop sees it in
        # the history even if it picks up state before our broadcast arrives.
        user_msg = ChatMessage(role=Role.USER, content=message)
        session.add_message(user_msg)
        state.session_manager.save_session(session)

        # Notify all connected WebSocket clients of the new user message.
        await global_ws_manager.broadcast(
            ws_message(
                WSMessageType.USER_MESSAGE,
                role="user",
                content=message,
                session_id=session_id,
            )
        )

        # Lazily initialise the shared executor (same singleton used by the
        # WebSocket path) and fire the agent loop as a background task.
        if not hasattr(state, "_agent_executor") or state._agent_executor is None:
            state._agent_executor = AgentExecutor(state)

        asyncio.create_task(
            state._agent_executor.execute_query(
                message,
                global_ws_manager,
                session_id=session_id,
                session=session,
            )
        )

        return {"status": "accepted", "session_id": session_id}

    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.get("/messages")
async def get_messages() -> List[MessageResponse]:
    """Get all messages in the current session.

    Returns:
        List of messages

    Raises:
        HTTPException: If retrieval fails
    """
    try:
        state = get_state()

        # Return empty list if no session exists
        session = state.session_manager.get_current_session()
        if not session:
            return []

        messages = state.get_messages()

        return [
            MessageResponse(
                role=msg.role.value,
                content=msg.content,
                timestamp=(
                    msg.timestamp.isoformat()
                    if hasattr(msg, "timestamp") and msg.timestamp
                    else None
                ),
                tool_calls=(
                    [tool_call_to_info(tc) for tc in msg.tool_calls] if msg.tool_calls else None
                ),
                thinking_trace=msg.thinking_trace,
                reasoning_content=msg.reasoning_content,
            )
            for msg in messages
        ]

    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


class ClearChatRequest(BaseModel):
    """Request model for clearing chat with optional workspace."""

    workspace: str | None = None


@router.delete("/clear")
async def clear_chat(request: ClearChatRequest | None = None) -> Dict[str, str]:
    """Clear the current chat session.

    Args:
        request: Optional request with workspace path

    Returns:
        Status response

    Raises:
        HTTPException: If clearing fails
    """
    try:
        state = get_state()
        # Create a new session (effectively clearing current one)
        if request and request.workspace:
            state.session_manager.create_session(working_directory=request.workspace)
        else:
            state.session_manager.create_session()

        return {"status": "success", "message": "Chat cleared"}

    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.post("/interrupt")
async def interrupt_task() -> Dict[str, str]:
    """Interrupt the currently running task.

    Returns:
        Status response

    Raises:
        HTTPException: If interrupt fails
    """
    try:
        state = get_state()
        # Signal interrupt via state flag (legacy fallback)
        state.request_interrupt()
        # Also signal via ReactExecutor's interrupt token (primary mechanism)
        agent_executor = getattr(state, "_agent_executor", None)
        if agent_executor:
            # Interrupt all running sessions
            for sid in list(agent_executor._current_react_executors.keys()):
                agent_executor.interrupt_session(sid)

        return {"status": "success", "message": "Interrupt requested"}

    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc
