"""LocalRuntime - executes commands locally (runs inside Docker container).

This is the actual execution engine that manages bash sessions and file operations.
It runs inside the Docker container and is controlled via the FastAPI server.
"""

from __future__ import annotations

import logging
from pathlib import Path

from .exceptions import SessionDoesNotExistError, SessionExistsError
from .models import (
    BashAction,
    BashObservation,
    CloseSessionRequest,
    CloseSessionResponse,
    CreateSessionRequest,
    CreateSessionResponse,
    IsAliveResponse,
    ReadFileRequest,
    ReadFileResponse,
    WriteFileRequest,
    WriteFileResponse,
)
from .session import BashSession

__all__ = ["LocalRuntime"]

logger = logging.getLogger(__name__)


class LocalRuntime:
    """Local runtime that manages bash sessions and file operations.

    This runs inside the Docker container and provides the actual
    execution capabilities.
    """

    def __init__(self):
        """Initialize the local runtime."""
        self._sessions: dict[str, BashSession] = {}
        self._closed = False

    async def is_alive(self) -> IsAliveResponse:
        """Health check endpoint."""
        if self._closed:
            return IsAliveResponse(status="error", message="Runtime is closed")
        return IsAliveResponse(status="ok")

    async def create_session(self, request: CreateSessionRequest) -> CreateSessionResponse:
        """Create a new bash session.

        Args:
            request: Session creation request

        Returns:
            CreateSessionResponse with session info

        Raises:
            SessionExistsError: If session already exists
        """
        session_name = request.session

        if session_name in self._sessions:
            raise SessionExistsError(session_name)

        session = BashSession(
            name=session_name,
            startup_timeout=request.startup_timeout,
        )

        response = await session.start()
        self._sessions[session_name] = session

        logger.info(f"Created session '{session_name}'")
        return response

    async def run_in_session(self, action: BashAction) -> BashObservation:
        """Execute a command in an existing session.

        Args:
            action: The bash action to execute

        Returns:
            BashObservation with output and exit code

        Raises:
            SessionDoesNotExistError: If session doesn't exist
        """
        session_name = action.session

        if session_name not in self._sessions:
            # Auto-create default session if it doesn't exist
            if session_name == "default":
                await self.create_session(CreateSessionRequest(session=session_name))
            else:
                raise SessionDoesNotExistError(session_name)

        session = self._sessions[session_name]
        return await session.run(action)

    async def close_session(self, request: CloseSessionRequest) -> CloseSessionResponse:
        """Close a bash session.

        Args:
            request: Session close request

        Returns:
            CloseSessionResponse
        """
        session_name = request.session

        if session_name not in self._sessions:
            return CloseSessionResponse(success=False, message=f"Session '{session_name}' not found")

        session = self._sessions.pop(session_name)
        await session.close()

        logger.info(f"Closed session '{session_name}'")
        return CloseSessionResponse(success=True)

    async def read_file(self, request: ReadFileRequest) -> ReadFileResponse:
        """Read a file from the filesystem.

        Args:
            request: File read request

        Returns:
            ReadFileResponse with file contents
        """
        try:
            path = Path(request.path)
            if not path.exists():
                return ReadFileResponse(
                    success=False,
                    error=f"File not found: {request.path}",
                )

            content = path.read_text(encoding="utf-8", errors="replace")
            return ReadFileResponse(success=True, content=content)

        except PermissionError:
            return ReadFileResponse(
                success=False,
                error=f"Permission denied: {request.path}",
            )
        except Exception as e:
            return ReadFileResponse(
                success=False,
                error=f"Error reading file: {e}",
            )

    async def write_file(self, request: WriteFileRequest) -> WriteFileResponse:
        """Write a file to the filesystem.

        Args:
            request: File write request

        Returns:
            WriteFileResponse
        """
        try:
            path = Path(request.path)

            # Create parent directories if needed
            path.parent.mkdir(parents=True, exist_ok=True)

            path.write_text(request.content, encoding="utf-8")
            return WriteFileResponse(success=True)

        except PermissionError:
            return WriteFileResponse(
                success=False,
                error=f"Permission denied: {request.path}",
            )
        except Exception as e:
            return WriteFileResponse(
                success=False,
                error=f"Error writing file: {e}",
            )

    async def close(self) -> None:
        """Close all sessions and shut down the runtime."""
        logger.info("Closing runtime...")

        # Close all sessions
        for session_name in list(self._sessions.keys()):
            try:
                await self.close_session(CloseSessionRequest(session=session_name))
            except Exception as e:
                logger.warning(f"Error closing session '{session_name}': {e}")

        self._closed = True
        logger.info("Runtime closed")
