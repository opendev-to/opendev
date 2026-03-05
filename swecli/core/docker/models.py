"""Pydantic models for Docker runtime protocol.

Based on SWE-ReX protocol for communication between host and container.
"""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, Field


# =============================================================================
# Session Management
# =============================================================================


class CreateSessionRequest(BaseModel):
    """Request to create a new bash session."""

    session: str = Field(default="default", description="Session name/identifier")
    startup_timeout: float = Field(default=10.0, description="Timeout for session startup")


class CreateSessionResponse(BaseModel):
    """Response after creating a bash session."""

    success: bool
    session: str
    message: str = ""


class CloseSessionRequest(BaseModel):
    """Request to close a bash session."""

    session: str = Field(default="default", description="Session to close")


class CloseSessionResponse(BaseModel):
    """Response after closing a bash session."""

    success: bool
    message: str = ""


# =============================================================================
# Command Execution
# =============================================================================


class BashAction(BaseModel):
    """Action to execute in a bash session."""

    command: str = Field(..., description="Bash command to execute")
    session: str = Field(default="default", description="Session to execute in")
    timeout: float = Field(default=120.0, description="Command timeout in seconds")
    check: Literal["raise", "silent", "ignore"] = Field(
        default="silent",
        description="How to handle non-zero exit codes: raise=error, silent=return, ignore=skip check",
    )


class BashObservation(BaseModel):
    """Observation/result from executing a bash action."""

    output: str = Field(default="", description="Command output (stdout + stderr)")
    exit_code: int | None = Field(default=None, description="Command exit code")
    failure_reason: str | None = Field(default=None, description="Error description if failed")


# =============================================================================
# File Operations
# =============================================================================


class ReadFileRequest(BaseModel):
    """Request to read a file from the container."""

    path: str = Field(..., description="Absolute path to file")


class ReadFileResponse(BaseModel):
    """Response with file contents."""

    success: bool
    content: str = ""
    error: str | None = None


class WriteFileRequest(BaseModel):
    """Request to write a file in the container."""

    path: str = Field(..., description="Absolute path to file")
    content: str = Field(..., description="File content to write")


class WriteFileResponse(BaseModel):
    """Response after writing a file."""

    success: bool
    error: str | None = None


# =============================================================================
# Health Check
# =============================================================================


class IsAliveResponse(BaseModel):
    """Health check response."""

    status: Literal["ok", "error"] = "ok"
    message: str = ""


# =============================================================================
# Exception Transfer (for remote error handling)
# =============================================================================


class ExceptionTransfer(BaseModel):
    """Serialized exception for transfer over HTTP."""

    message: str
    class_name: str
    module: str
    traceback: str = ""
    extra: dict = Field(default_factory=dict)
