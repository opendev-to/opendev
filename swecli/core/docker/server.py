"""FastAPI server for Docker runtime - runs inside the container.

This server exposes the LocalRuntime via HTTP endpoints, allowing
the host to communicate with the container using the SWE-ReX protocol.

Usage (inside container):
    python -m swecli.core.docker.server --host 0.0.0.0 --port 8000 --auth-token <UUID>
"""

from __future__ import annotations

import argparse
import logging
import sys
import traceback
from typing import Any

from fastapi import Depends, FastAPI, Header, HTTPException, Request
from fastapi.responses import JSONResponse

from .exceptions import DockerException
from .local_runtime import LocalRuntime
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

__all__ = ["create_app", "run_server"]

logger = logging.getLogger(__name__)


def create_app(auth_token: str | None = None) -> FastAPI:
    """Create the FastAPI application.

    Args:
        auth_token: Optional authentication token for API access

    Returns:
        Configured FastAPI app
    """
    app = FastAPI(
        title="OpenDev Docker Runtime",
        description="HTTP API for Docker container runtime",
        version="1.0.0",
    )

    # Store runtime and auth token in app state
    app.state.runtime = LocalRuntime()
    app.state.auth_token = auth_token

    # Authentication dependency
    async def verify_auth(x_api_key: str = Header(None)) -> None:
        if app.state.auth_token and x_api_key != app.state.auth_token:
            raise HTTPException(status_code=401, detail="Invalid API key")

    # Exception handler for DockerExceptions
    @app.exception_handler(DockerException)
    async def docker_exception_handler(request: Request, exc: DockerException) -> JSONResponse:
        """Convert DockerException to JSON response with status 511."""
        transfer = ExceptionTransfer(
            message=str(exc),
            class_name=exc.__class__.__name__,
            module=exc.__class__.__module__,
            traceback=traceback.format_exc(),
        )
        return JSONResponse(
            status_code=511,  # Network Authentication Required (used for app-level errors)
            content=transfer.model_dump(),
        )

    # General exception handler
    @app.exception_handler(Exception)
    async def general_exception_handler(request: Request, exc: Exception) -> JSONResponse:
        """Convert general exceptions to JSON response."""
        transfer = ExceptionTransfer(
            message=str(exc),
            class_name=exc.__class__.__name__,
            module=exc.__class__.__module__,
            traceback=traceback.format_exc(),
        )
        return JSONResponse(
            status_code=500,
            content=transfer.model_dump(),
        )

    # Helper to get runtime
    def get_runtime() -> LocalRuntime:
        return app.state.runtime

    # ==========================================================================
    # Endpoints
    # ==========================================================================

    @app.get("/is_alive", response_model=IsAliveResponse)
    async def is_alive(
        _: None = Depends(verify_auth),
        runtime: LocalRuntime = Depends(get_runtime),
    ) -> IsAliveResponse:
        """Health check endpoint."""
        return await runtime.is_alive()

    @app.post("/create_session", response_model=CreateSessionResponse)
    async def create_session(
        request: CreateSessionRequest,
        _: None = Depends(verify_auth),
        runtime: LocalRuntime = Depends(get_runtime),
    ) -> CreateSessionResponse:
        """Create a new bash session."""
        return await runtime.create_session(request)

    @app.post("/run_in_session", response_model=BashObservation)
    async def run_in_session(
        action: BashAction,
        _: None = Depends(verify_auth),
        runtime: LocalRuntime = Depends(get_runtime),
    ) -> BashObservation:
        """Execute a command in an existing session."""
        return await runtime.run_in_session(action)

    @app.post("/close_session", response_model=CloseSessionResponse)
    async def close_session(
        request: CloseSessionRequest,
        _: None = Depends(verify_auth),
        runtime: LocalRuntime = Depends(get_runtime),
    ) -> CloseSessionResponse:
        """Close a bash session."""
        return await runtime.close_session(request)

    @app.post("/read_file", response_model=ReadFileResponse)
    async def read_file(
        request: ReadFileRequest,
        _: None = Depends(verify_auth),
        runtime: LocalRuntime = Depends(get_runtime),
    ) -> ReadFileResponse:
        """Read a file from the container filesystem."""
        return await runtime.read_file(request)

    @app.post("/write_file", response_model=WriteFileResponse)
    async def write_file(
        request: WriteFileRequest,
        _: None = Depends(verify_auth),
        runtime: LocalRuntime = Depends(get_runtime),
    ) -> WriteFileResponse:
        """Write a file to the container filesystem."""
        return await runtime.write_file(request)

    @app.post("/close")
    async def close(
        _: None = Depends(verify_auth),
        runtime: LocalRuntime = Depends(get_runtime),
    ) -> dict[str, Any]:
        """Shutdown the runtime and server."""
        await runtime.close()
        return {"status": "ok", "message": "Runtime closed"}

    return app


def run_server(host: str = "0.0.0.0", port: int = 8000, auth_token: str | None = None) -> None:
    """Run the server.

    Args:
        host: Host to bind to
        port: Port to listen on
        auth_token: Optional authentication token
    """
    import uvicorn

    app = create_app(auth_token=auth_token)
    uvicorn.run(app, host=host, port=port, log_level="info")


def main() -> None:
    """CLI entry point for running the server."""
    parser = argparse.ArgumentParser(description="OpenDev Docker Runtime Server")
    parser.add_argument("--host", default="0.0.0.0", help="Host to bind to")
    parser.add_argument("--port", type=int, default=8000, help="Port to listen on")
    parser.add_argument("--auth-token", help="Authentication token for API access")

    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )

    logger.info(f"Starting server on {args.host}:{args.port}")
    run_server(host=args.host, port=args.port, auth_token=args.auth_token)


if __name__ == "__main__":
    main()
