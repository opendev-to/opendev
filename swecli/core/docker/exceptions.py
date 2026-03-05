"""Exception classes for Docker runtime.

Based on SWE-ReX exception hierarchy.
"""

from __future__ import annotations


class DockerException(Exception):
    """Base exception for Docker runtime errors."""

    pass


class SessionNotInitializedError(DockerException):
    """Raised when trying to use a session that hasn't been created."""

    def __init__(self, session: str = "default"):
        self.session = session
        super().__init__(f"Session '{session}' has not been initialized")


class SessionExistsError(DockerException):
    """Raised when trying to create a session that already exists."""

    def __init__(self, session: str = "default"):
        self.session = session
        super().__init__(f"Session '{session}' already exists")


class SessionDoesNotExistError(DockerException):
    """Raised when trying to access a session that doesn't exist."""

    def __init__(self, session: str = "default"):
        self.session = session
        super().__init__(f"Session '{session}' does not exist")


class NonZeroExitCodeError(DockerException):
    """Raised when a command returns a non-zero exit code."""

    def __init__(self, exit_code: int, command: str = "", output: str = ""):
        self.exit_code = exit_code
        self.command = command
        self.output = output
        super().__init__(f"Command exited with code {exit_code}: {command[:100]}")


class CommandTimeoutError(DockerException):
    """Raised when a command times out."""

    def __init__(self, timeout: float, command: str = ""):
        self.timeout = timeout
        self.command = command
        super().__init__(f"Command timed out after {timeout}s: {command[:100]}")


class BashSyntaxError(DockerException):
    """Raised when bash command has syntax errors."""

    def __init__(self, command: str, error: str = ""):
        self.command = command
        self.error = error
        super().__init__(f"Bash syntax error: {error}")


class NoExitCodeError(DockerException):
    """Raised when exit code couldn't be extracted from command output."""

    def __init__(self, command: str = ""):
        self.command = command
        super().__init__(f"Could not extract exit code from command: {command[:100]}")


class DeploymentNotStartedError(DockerException):
    """Raised when trying to use deployment that hasn't been started."""

    pass


class DeploymentStartupError(DockerException):
    """Raised when deployment fails to start."""

    def __init__(self, message: str, details: str = ""):
        self.details = details
        super().__init__(message)


class DockerPullError(DeploymentStartupError):
    """Raised when Docker image pull fails."""

    def __init__(self, image: str, error: str = ""):
        self.image = image
        super().__init__(f"Failed to pull Docker image '{image}': {error}")


class DockerContainerError(DockerException):
    """Raised when Docker container operation fails."""

    def __init__(self, container_id: str, operation: str, error: str = ""):
        self.container_id = container_id
        self.operation = operation
        super().__init__(f"Docker container {operation} failed for '{container_id}': {error}")


class ConnectionError(DockerException):
    """Raised when connection to runtime server fails."""

    def __init__(self, host: str, port: int, error: str = ""):
        self.host = host
        self.port = port
        super().__init__(f"Failed to connect to {host}:{port}: {error}")
