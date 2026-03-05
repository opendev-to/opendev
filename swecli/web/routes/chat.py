"""Chat and query API endpoints."""

from typing import Dict, List

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from swecli.web.state import get_state
from swecli.models.message import ChatMessage, Role

router = APIRouter(prefix="/api/chat", tags=["chat"])


class QueryRequest(BaseModel):
    """Request model for sending a query."""
    message: str
    sessionId: str | None = None


class ToolCallInfo(BaseModel):
    """Tool call information."""
    id: str
    name: str
    parameters: Dict
    result: str | None = None
    error: str | None = None
    result_summary: str | None = None
    approved: bool | None = None


class MessageResponse(BaseModel):
    """Response model for a chat message."""
    role: str
    content: str
    timestamp: str | None = None
    tool_calls: List[ToolCallInfo] | None = None


@router.post("/query")
async def send_query(request: QueryRequest) -> Dict[str, str]:
    """Send a query to the AI agent.

    Args:
        request: Query request with message and optional session ID

    Returns:
        Status response

    Raises:
        HTTPException: If query fails
    """
    try:
        state = get_state()

        # Add user message to session
        user_msg = ChatMessage(role=Role.USER, content=request.message)
        state.add_message(user_msg)

        # TODO: Trigger agent processing in background
        # For now, just acknowledge receipt

        return {
            "status": "received",
            "message": "Query processing will be implemented in next phase"
        }

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


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
            print("[DEBUG] No current session found")
            return []

        messages = state.get_messages()
        print(f"[DEBUG] Loaded {len(messages)} messages from session {session.id}")

        return [
            MessageResponse(
                role=msg.role.value,
                content=msg.content,
                timestamp=msg.timestamp.isoformat() if hasattr(msg, 'timestamp') and msg.timestamp else None,
                tool_calls=[
                    ToolCallInfo(
                        id=tc.id,
                        name=tc.name,
                        parameters=tc.parameters,
                        result=tc.result,
                        error=tc.error,
                        result_summary=tc.result_summary,
                        approved=tc.approved
                    )
                    for tc in msg.tool_calls
                ] if msg.tool_calls else None
            )
            for msg in messages
        ]

    except Exception as e:
        print(f"[ERROR] Failed to get messages: {e}")
        import traceback
        traceback.print_exc()
        raise HTTPException(status_code=500, detail=str(e))


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

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))


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
        state.request_interrupt()

        return {"status": "success", "message": "Interrupt requested"}

    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
