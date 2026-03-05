"""RemoteRuntime - HTTP client for communicating with container runtime.

This runs on the host and sends HTTP requests to the FastAPI server
running inside the Docker container.
"""

from __future__ import annotations

import logging
import time
from typing import Any

import httpx

from .exceptions import (
    CommandTimeoutError,
    ConnectionError,
    DockerException,
    NonZeroExitCodeError,
    SessionDoesNotExistError,
    SessionExistsError,
    SessionNotInitializedError,
)
from .models import (
    BashAction,
    BashObservation,
    CloseSessionRequest,
    CloseSessionResponse,
    CreateSessionRequest,
    CreateSessionResponse,
    ExceptionTransfer,
    IsAliveResponse,
    ReadFileRequest,
    ReadFileResponse,
    WriteFileRequest,
    WriteFileResponse,
)

__all__ = ["RemoteRuntime"]

logger = logging.getLogger(__name__)


# Map exception class names to exception classes for deserialization
EXCEPTION_MAP: dict[str, type[DockerException]] = {
    "SessionNotInitializedError": SessionNotInitializedError,
    "SessionExistsError": SessionExistsError,
    "SessionDoesNotExistError": SessionDoesNotExistError,
    "NonZeroExitCodeError": NonZeroExitCodeError,
    "CommandTimeoutError": CommandTimeoutError,
}


class RemoteRuntime:
    """HTTP client for communicating with Docker container runtime.

    This class mirrors the LocalRuntime interface but sends HTTP requests
    to the FastAPI server running inside the container.
    """

    def __init__(
        self,
        host: str = "localhost",
        port: int = 8000,
        auth_token: str | None = None,
        timeout: float = 300.0,
    ):
        """Initialize the remote runtime client.

        Args:
            host: Host where the runtime server is running
            port: Port the server is listening on
            auth_token: Authentication token for API access
            timeout: Default timeout for HTTP requests in seconds
        """
        self.host = host
        self.port = port
        self.auth_token = auth_token
        self.timeout = timeout
        self._base_url = f"http://{host}:{port}"
        self._client: httpx.AsyncClient | None = None

    async def _ensure_client(self) -> httpx.AsyncClient:
        """Get or create the HTTP client."""
        if self._client is None or self._client.is_closed:
            headers = {}
            if self.auth_token:
                headers["X-API-Key"] = self.auth_token

            self._client = httpx.AsyncClient(
                base_url=self._base_url,
                headers=headers,
                timeout=httpx.Timeout(self.timeout),
            )
        return self._client

    def _handle_error_response(self, response: httpx.Response) -> None:
        """Handle error responses from the server.

        Args:
            response: The HTTP response

        Raises:
            DockerException: Appropriate exception based on response
        """
        if response.status_code == 511:
            # Application-level exception transfer
            try:
                transfer = ExceptionTransfer.model_validate(response.json())
                exc_class = EXCEPTION_MAP.get(transfer.class_name, DockerException)

                # Try to reconstruct the exception
                if exc_class == NonZeroExitCodeError:
                    raise NonZeroExitCodeError(
                        exit_code=transfer.extra.get("exit_code", -1),
                        command=transfer.extra.get("command", ""),
                        output=transfer.extra.get("output", ""),
                    )
                elif exc_class == CommandTimeoutError:
                    raise CommandTimeoutError(
                        timeout=transfer.extra.get("timeout", 0),
                        command=transfer.extra.get("command", ""),
                    )
                elif exc_class == SessionNotInitializedError:
                    raise SessionNotInitializedError(transfer.extra.get("session", "default"))
                elif exc_class == SessionExistsError:
                    raise SessionExistsError(transfer.extra.get("session", "default"))
                elif exc_class == SessionDoesNotExistError:
                    raise SessionDoesNotExistError(transfer.extra.get("session", "default"))
                else:
                    raise DockerException(transfer.message)
            except DockerException:
                raise
            except Exception:
                raise DockerException(f"Error from server: {response.text}")

        elif response.status_code == 401:
            raise DockerException("Authentication failed - invalid API key")
        elif response.status_code >= 400:
            raise DockerException(f"HTTP error {response.status_code}: {response.text}")

    async def _post(self, endpoint: str, data: dict[str, Any] | None = None) -> dict[str, Any]:
        """Make a POST request to the server.

        Args:
            endpoint: API endpoint (without leading slash)
            data: Request body as dict

        Returns:
            Response JSON as dict

        Raises:
            ConnectionError: If connection fails
            DockerException: If server returns error
        """
        client = await self._ensure_client()

        try:
            response = await client.post(f"/{endpoint}", json=data or {})
            self._handle_error_response(response)
            return response.json()
        except httpx.ConnectError as e:
            raise ConnectionError(self.host, self.port, str(e))
        except httpx.TimeoutException as e:
            raise CommandTimeoutError(self.timeout, endpoint)

    async def _get(self, endpoint: str) -> dict[str, Any]:
        """Make a GET request to the server."""
        client = await self._ensure_client()

        try:
            response = await client.get(f"/{endpoint}")
            self._handle_error_response(response)
            return response.json()
        except httpx.ConnectError as e:
            raise ConnectionError(self.host, self.port, str(e))
        except httpx.TimeoutException:
            raise CommandTimeoutError(self.timeout, endpoint)

    async def is_alive(self) -> IsAliveResponse:
        """Check if the runtime server is alive."""
        try:
            data = await self._get("is_alive")
            return IsAliveResponse.model_validate(data)
        except Exception:
            return IsAliveResponse(status="error", message="Connection failed")

    async def wait_for_ready(
        self,
        timeout: float = 60.0,
        poll_interval: float = 0.5,
    ) -> bool:
        """Wait for the runtime server to become ready.

        Args:
            timeout: Maximum time to wait in seconds
            poll_interval: Time between health checks

        Returns:
            True if server became ready, False if timeout
        """
        start_time = time.time()
        while time.time() - start_time < timeout:
            try:
                response = await self.is_alive()
                if response.status == "ok":
                    return True
            except Exception:
                pass
            await self._sleep(poll_interval)
        return False

    async def _sleep(self, seconds: float) -> None:
        """Async sleep helper."""
        import asyncio
        await asyncio.sleep(seconds)

    async def create_session(self, request: CreateSessionRequest) -> CreateSessionResponse:
        """Create a new bash session in the container."""
        data = await self._post("create_session", request.model_dump())
        return CreateSessionResponse.model_validate(data)

    async def run_in_session(self, action: BashAction) -> BashObservation:
        """Execute a command in an existing session."""
        # Use longer timeout for command execution
        client = await self._ensure_client()
        old_timeout = client.timeout
        try:
            client.timeout = httpx.Timeout(action.timeout + 30)  # Add buffer
            data = await self._post("run_in_session", action.model_dump())
            return BashObservation.model_validate(data)
        finally:
            client.timeout = old_timeout

    async def close_session(self, request: CloseSessionRequest) -> CloseSessionResponse:
        """Close a bash session."""
        data = await self._post("close_session", request.model_dump())
        return CloseSessionResponse.model_validate(data)

    async def read_file(self, path: str) -> str:
        """Read a file from the container.

        Args:
            path: Absolute path to the file

        Returns:
            File contents as string

        Raises:
            DockerException: If file read fails
        """
        request = ReadFileRequest(path=path)
        data = await self._post("read_file", request.model_dump())
        response = ReadFileResponse.model_validate(data)

        if not response.success:
            raise DockerException(response.error or f"Failed to read file: {path}")

        return response.content

    async def write_file(self, path: str, content: str) -> None:
        """Write a file to the container.

        Args:
            path: Absolute path to the file
            content: File contents to write

        Raises:
            DockerException: If file write fails
        """
        request = WriteFileRequest(path=path, content=content)
        data = await self._post("write_file", request.model_dump())
        response = WriteFileResponse.model_validate(data)

        if not response.success:
            raise DockerException(response.error or f"Failed to write file: {path}")

    async def close(self) -> None:
        """Close the runtime and HTTP client."""
        try:
            await self._post("close")
        except Exception:
            pass  # Ignore errors during cleanup

        if self._client:
            await self._client.aclose()
            self._client = None

    # Convenience methods for common operations

    async def run(self, command: str, timeout: float = 120.0, check: str = "silent") -> BashObservation:
        """Convenience method to run a command.

        Args:
            command: Bash command to execute
            timeout: Command timeout in seconds
            check: How to handle non-zero exit codes

        Returns:
            BashObservation with output and exit code
        """
        action = BashAction(command=command, timeout=timeout, check=check)
        return await self.run_in_session(action)
