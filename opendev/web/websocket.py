"""WebSocket handler for real-time communication."""

from __future__ import annotations

from typing import Any, Dict

from fastapi import WebSocket, WebSocketDisconnect, status

from opendev.web.state import get_state
from opendev.web.logging_config import logger
from opendev.models.message import ChatMessage, Role
from opendev.web.routes.auth import TOKEN_COOKIE, verify_token
from opendev.web.routes.auth import TOKEN_COOKIE, verify_token


class WebSocketManager:
    """Manages WebSocket connections and message broadcasting."""

    def __init__(self):
        self.active_connections: list[WebSocket] = []

    async def connect(self, websocket: WebSocket):
        """Accept a new WebSocket connection."""
        await websocket.accept()
        self.active_connections.append(websocket)
        state = get_state()
        state.add_ws_client(websocket)

    def disconnect(self, websocket: WebSocket):
        """Remove a WebSocket connection."""
        if websocket in self.active_connections:
            self.active_connections.remove(websocket)
        state = get_state()
        state.remove_ws_client(websocket)

    async def send_message(self, websocket: WebSocket, message: Dict[str, Any]):
        """Send a message to a specific client."""
        try:
            await websocket.send_json(message)
        except Exception as e:
            logger.error(f"Failed to send WebSocket message: {e}")
            logger.error(f"Message type: {message.get('type')}")
            self.disconnect(websocket)

    async def broadcast(self, message: Dict[str, Any]):
        """Broadcast a message to all connected clients."""
        # Validate message is JSON-serializable before broadcasting
        try:
            import json
            json.dumps(message)
            logger.debug(f"Broadcasting: {message.get('type')}")
        except (TypeError, ValueError) as e:
            logger.error(f"❌ Message is not JSON-serializable: {e}")
            logger.error(f"Message type: {message.get('type')}")
            logger.error(f"Message keys: {list(message.keys())}")
            # Try to send error message instead
            error_message = {
                "type": "error",
                "data": {"message": f"Internal serialization error: {str(e)}"}
            }
            message = error_message

        disconnected = []
        for connection in self.active_connections:
            try:
                await connection.send_json(message)
            except Exception as e:
                logger.error(f"Failed to broadcast to connection: {e}")
                logger.error(f"Message type: {message.get('type')}")
                disconnected.append(connection)

        # Clean up disconnected clients
        for conn in disconnected:
            self.disconnect(conn)

    async def handle_message(self, websocket: WebSocket, data: Dict[str, Any]):
        """Handle incoming WebSocket message."""
        msg_type = data.get("type")
        if msg_type != "ping":
            logger.debug(f"Received WebSocket message: type={msg_type}")

        if msg_type == "query":
            await self._handle_query(websocket, data)
        elif msg_type == "approve":
            await self._handle_approval(websocket, data)
        elif msg_type == "ask_user_response":
            await self._handle_ask_user_response(websocket, data)
        elif msg_type == "plan_approval_response":
            await self._handle_plan_approval_response(websocket, data)
        elif msg_type == "ping":
            await self.send_message(websocket, {"type": "pong"})
        else:
            logger.warning(f"Unknown message type: {msg_type}")
            await self.send_message(
                websocket,
                {"type": "error", "data": {"message": f"Unknown message type: {msg_type}"}}
            )

    async def _handle_query(self, websocket: WebSocket, data: Dict[str, Any]):
        """Handle a query message."""
        import asyncio

        message = data.get("data", {}).get("message")
        session_id = data.get("data", {}).get("session_id")

        if not message:
            await self.send_message(
                websocket,
                {"type": "error", "data": {"message": "Missing message field"}}
            )
            return

        state = get_state()

        # Resolve session_id: use provided, fall back to current
        if not session_id:
            session_id = state.get_current_session_id()
        if not session_id:
            await self.send_message(websocket, {
                "type": "error", "data": {"message": "No active session"}
            })
            return

        # If session is already running, inject message into the agent loop
        if state.is_session_running(session_id):
            injection_queue = state.get_injection_queue(session_id)
            import queue as queue_mod
            try:
                injection_queue.put_nowait(message)
            except queue_mod.Full:
                await self.send_message(websocket, {
                    "type": "error",
                    "data": {
                        "message": "Injection queue full, message dropped",
                        "session_id": session_id,
                    },
                })
                return
            # Broadcast injected user message (EC5: no session persistence here)
            await self.broadcast({
                "type": "user_message",
                "data": {
                    "role": "user",
                    "content": message,
                    "session_id": session_id,
                    "injected": True,
                },
            })
            return

        # Load session without mutating current_session
        try:
            session = state.session_manager.get_session_by_id(session_id)
        except FileNotFoundError:
            # Fallback: session may be newly created but not yet on disk
            current = state.session_manager.get_current_session()
            if current and current.id == session_id:
                session = current
            else:
                await self.send_message(websocket, {
                    "type": "error",
                    "data": {"message": f"Session {session_id} not found"},
                })
                return

        # Add user message directly to the session object
        user_msg = ChatMessage(role=Role.USER, content=message)
        session.add_message(user_msg)
        state.session_manager.save_session(session)

        # Broadcast user message with session_id
        await self.broadcast({
            "type": "user_message",
            "data": {
                "role": "user",
                "content": message,
                "session_id": session_id,
            }
        })

        # Execute query with agent in background task
        from opendev.web.agent_executor import AgentExecutor

        executor = AgentExecutor(state)
        asyncio.create_task(
            executor.execute_query(message, self, session_id=session_id, session=session)
        )

    async def _handle_approval(self, websocket: WebSocket, data: Dict[str, Any]):
        """Handle an approval response from the web UI."""
        logger.info(f"Received approval response: {data}")
        approval_data = data.get("data", {})
        approval_id = approval_data.get("approvalId")
        approved = approval_data.get("approved")
        auto_approve = approval_data.get("autoApprove", False)

        logger.info(f"Approval: id={approval_id}, approved={approved}, auto={auto_approve}")

        if approval_id is None or approved is None:
            logger.error(f"Invalid approval data: {approval_data}")
            await self.send_message(
                websocket,
                {"type": "error", "data": {"message": "Invalid approval data"}}
            )
            return

        # Resolve the approval in shared state
        state = get_state()
        success = state.resolve_approval(approval_id, approved, auto_approve)

        if not success:
            logger.error(f"Approval {approval_id} not found in state")
            await self.send_message(
                websocket,
                {"type": "error", "data": {"message": f"Approval {approval_id} not found"}}
            )
            return

        logger.info(f"✓ Approval {approval_id} resolved successfully")
        # Retrieve session_id from the pending approval
        pending = state.get_pending_approval(approval_id)
        resolved_session_id = pending.get("session_id") if pending else None
        # Broadcast the resolution to all clients
        await self.broadcast({
            "type": "approval_resolved",
            "data": {
                "approvalId": approval_id,
                "approved": approved,
                "session_id": resolved_session_id,
            }
        })

    async def _handle_ask_user_response(self, websocket: WebSocket, data: Dict[str, Any]):
        """Handle an ask-user response from the web UI."""
        logger.info(f"Received ask-user response: {data}")
        response_data = data.get("data", {})
        request_id = response_data.get("requestId")
        answers = response_data.get("answers")
        cancelled = response_data.get("cancelled", False)

        if not request_id:
            logger.error(f"Invalid ask-user response data: {response_data}")
            await self.send_message(
                websocket,
                {"type": "error", "data": {"message": "Invalid ask-user response data"}}
            )
            return

        state = get_state()
        success = state.resolve_ask_user(request_id, answers, cancelled)

        if not success:
            logger.error(f"Ask-user request {request_id} not found in state")
            await self.send_message(
                websocket,
                {"type": "error", "data": {"message": f"Ask-user request {request_id} not found"}}
            )
            return

        logger.info(f"✓ Ask-user {request_id} resolved")
        # Retrieve session_id from the pending ask-user request
        pending = state.get_pending_ask_user(request_id)
        resolved_session_id = pending.get("session_id") if pending else None
        await self.broadcast({
            "type": "ask_user_resolved",
            "data": {"requestId": request_id, "session_id": resolved_session_id},
        })

    async def _handle_plan_approval_response(
        self, websocket: WebSocket, data: Dict[str, Any]
    ):
        """Handle a plan approval response from the web UI."""
        logger.info(f"Received plan approval response: {data}")
        response_data = data.get("data", {})
        request_id = response_data.get("requestId")
        action = response_data.get("action", "reject")
        feedback = response_data.get("feedback", "")

        if not request_id:
            logger.error(f"Invalid plan approval response data: {response_data}")
            await self.send_message(
                websocket,
                {"type": "error", "data": {"message": "Invalid plan approval response data"}},
            )
            return

        state = get_state()
        success = state.resolve_plan_approval(request_id, action, feedback)

        if not success:
            logger.error(f"Plan approval request {request_id} not found in state")
            await self.send_message(
                websocket,
                {
                    "type": "error",
                    "data": {"message": f"Plan approval request {request_id} not found"},
                },
            )
            return

        logger.info(f"✓ Plan approval {request_id} resolved: action={action}")
        pending = state.get_pending_plan_approval(request_id)
        resolved_session_id = pending.get("session_id") if pending else None
        await self.broadcast({
            "type": "plan_approval_resolved",
            "data": {
                "requestId": request_id,
                "action": action,
                "session_id": resolved_session_id,
            },
        })


# Global WebSocket manager instance
ws_manager = WebSocketManager()


async def websocket_endpoint(websocket: WebSocket):
    """WebSocket endpoint handler."""
    token = websocket.cookies.get(TOKEN_COOKIE)
    if token:
        try:
            user_id = verify_token(token)
            state = get_state()
            user = state.user_store.get_by_id(user_id)
            if user:
                websocket.scope["user"] = user
        except Exception:
            pass  # Fall through to unauthenticated connection

    logger.info("New WebSocket connection established")
    await ws_manager.connect(websocket)

    try:
        while True:
            data = await websocket.receive_json()
            if data.get("type") != "ping":
                logger.debug(f"Raw message received: {data}")
            await ws_manager.handle_message(websocket, data)

    except WebSocketDisconnect:
        logger.info("WebSocket disconnected normally")
        ws_manager.disconnect(websocket)
    except Exception as e:
        logger.error(f"❌ WebSocket error: {e}")
        import traceback
        logger.error(traceback.format_exc())
        ws_manager.disconnect(websocket)
