"""Docker-based execution runtime for isolated issue resolution.

This module provides Docker container management and remote execution
capabilities based on the SWE-ReX protocol.
"""

from .models import (
    BashAction,
    BashObservation,
    CreateSessionRequest,
    CreateSessionResponse,
    CloseSessionRequest,
    CloseSessionResponse,
    ReadFileRequest,
    ReadFileResponse,
    WriteFileRequest,
    WriteFileResponse,
)
from .exceptions import (
    DockerException,
    SessionNotInitializedError,
    NonZeroExitCodeError,
    CommandTimeoutError,
    DeploymentNotStartedError,
    DeploymentStartupError,
)
from .deployment import DockerDeployment, DockerConfig
from .remote_runtime import RemoteRuntime
from .tool_handler import DockerToolHandler, DockerToolRegistry

__all__ = [
    # Models
    "BashAction",
    "BashObservation",
    "CreateSessionRequest",
    "CreateSessionResponse",
    "CloseSessionRequest",
    "CloseSessionResponse",
    "ReadFileRequest",
    "ReadFileResponse",
    "WriteFileRequest",
    "WriteFileResponse",
    # Exceptions
    "DockerException",
    "SessionNotInitializedError",
    "NonZeroExitCodeError",
    "CommandTimeoutError",
    "DeploymentNotStartedError",
    "DeploymentStartupError",
    # Runtime
    "DockerDeployment",
    "DockerConfig",
    "RemoteRuntime",
    "DockerToolHandler",
    "DockerToolRegistry",
]
