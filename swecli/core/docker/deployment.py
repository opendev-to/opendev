"""DockerDeployment - manages Docker container lifecycle.

This handles starting, stopping, and managing Docker containers
that run the FastAPI runtime server.
"""

from __future__ import annotations

import asyncio
import logging
import shutil
import socket
import subprocess
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

from .exceptions import (
    DeploymentNotStartedError,
    DeploymentStartupError,
    DockerContainerError,
    DockerPullError,
)
from .remote_runtime import RemoteRuntime

__all__ = ["DockerDeployment", "DockerConfig"]

logger = logging.getLogger(__name__)


def _find_free_port() -> int:
    """Find a free port on the host."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("", 0))
        return s.getsockname()[1]


def _run_docker_command(
    args: list[str],
    timeout: float = 300.0,
    check: bool = True,
) -> subprocess.CompletedProcess:
    """Run a docker command.

    Args:
        args: Command arguments (without 'docker' prefix)
        timeout: Command timeout in seconds
        check: Whether to raise on non-zero exit code

    Returns:
        CompletedProcess result
    """
    cmd = ["docker"] + args
    logger.debug(f"Running: {' '.join(cmd)}")

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout,
    )

    if check and result.returncode != 0:
        logger.error(f"Docker command failed: {result.stderr}")
        raise subprocess.CalledProcessError(
            result.returncode, cmd, result.stdout, result.stderr
        )

    return result


@dataclass
class DockerConfig:
    """Configuration for Docker deployment."""

    # Container image
    image: str = "python:3.11"

    # Resource limits
    memory: str = "4g"
    cpus: str = "4"

    # Network
    network_mode: str = "bridge"

    # Startup
    startup_timeout: float = 120.0
    pull_policy: str = "if-not-present"  # "always", "never", "if-not-present"

    # Server inside container
    server_port: int = 8000

    # Extra environment variables
    environment: dict[str, str] = field(default_factory=dict)

    # Shell initialization command (prepended to all commands)
    # Empty for plain images (uv, python), conda activation for SWE-bench
    shell_init: str = ""


class DockerDeployment:
    """Manages Docker container lifecycle for runtime execution.

    This class handles:
    1. Pulling the Docker image (if needed)
    2. Starting a container with the FastAPI server
    3. Waiting for the server to become ready
    4. Providing access to the RemoteRuntime
    5. Stopping and cleaning up the container
    """

    def __init__(
        self,
        config: DockerConfig | None = None,
        on_status: Callable[[str], None] | None = None,
    ):
        """Initialize Docker deployment.

        Args:
            config: Docker configuration
            on_status: Optional callback for status updates
        """
        self.config = config or DockerConfig()
        self.on_status = on_status or (lambda s: None)

        # Generate unique identifiers
        self._auth_token = str(uuid.uuid4())
        self._container_name = f"opendev-runtime-{uuid.uuid4().hex[:8]}"
        self._host_port = _find_free_port()

        # State
        self._container_id: str | None = None
        self._runtime: RemoteRuntime | None = None
        self._started = False

    @property
    def runtime(self) -> RemoteRuntime:
        """Get the RemoteRuntime client.

        Raises:
            DeploymentNotStartedError: If deployment hasn't been started
        """
        if not self._started or self._runtime is None:
            raise DeploymentNotStartedError("Deployment not started - call start() first")
        return self._runtime

    @property
    def container_id(self) -> str | None:
        """Get the container ID."""
        return self._container_id

    @property
    def is_started(self) -> bool:
        """Check if deployment has been started."""
        return self._started

    async def start(self) -> None:
        """Start the Docker deployment.

        This will:
        1. Pull the image (if needed)
        2. Start the container
        3. Wait for the server to become ready
        """
        if self._started:
            logger.warning("Deployment already started")
            return

        # Check if Docker is available
        if not shutil.which("docker"):
            raise DeploymentStartupError("Docker is not installed or not in PATH")

        # Pull image if needed
        await self._pull_image()

        # Start container
        await self._start_container()

        # Create runtime client
        self._runtime = RemoteRuntime(
            host="localhost",
            port=self._host_port,
            auth_token=self._auth_token,
        )

        # Wait for server to be ready
        self.on_status("Waiting for container to be ready...")
        ready = await self._runtime.wait_for_ready(
            timeout=self.config.startup_timeout,
            poll_interval=0.5,
        )

        if not ready:
            await self.stop()
            raise DeploymentStartupError(
                f"Container failed to start within {self.config.startup_timeout}s"
            )

        self._started = True
        self.on_status(f"Container ready on port {self._host_port}")
        logger.info(f"Docker deployment started: {self._container_name}")

    async def _pull_image(self) -> None:
        """Pull the Docker image if needed."""
        policy = self.config.pull_policy

        if policy == "never":
            return

        # Check if image exists locally
        if policy == "if-not-present":
            result = _run_docker_command(
                ["image", "inspect", self.config.image],
                check=False,
            )
            if result.returncode == 0:
                logger.info(f"Image {self.config.image} already exists locally")
                return

        # Pull the image
        self.on_status(f"Pulling Docker image: {self.config.image}")
        logger.info(f"Pulling Docker image: {self.config.image}")

        try:
            _run_docker_command(
                ["pull", self.config.image],
                timeout=600.0,  # Images can be large
            )
        except subprocess.CalledProcessError as e:
            raise DockerPullError(self.config.image, e.stderr)

    async def _start_container(self) -> None:
        """Start the Docker container."""
        self.on_status(f"Starting container: {self._container_name}")

        # Build the docker run command
        cmd = [
            "run",
            "--detach",
            "--rm",  # Remove container when stopped
            f"--name={self._container_name}",
            f"--memory={self.config.memory}",
            f"--cpus={self.config.cpus}",
            f"--publish={self._host_port}:{self.config.server_port}",
        ]

        # Add environment variables
        env_vars = {
            "OPENDEV_AUTH_TOKEN": self._auth_token,
            "OPENDEV_PORT": str(self.config.server_port),
            **self.config.environment,
        }
        for key, value in env_vars.items():
            cmd.append(f"--env={key}={value}")

        # Add the image and command
        # The container runs a Python HTTP server
        cmd.extend([
            self.config.image,
            "python", "-c", self._get_server_script(),
        ])

        try:
            result = _run_docker_command(cmd)
            self._container_id = result.stdout.strip()
            logger.info(f"Container started: {self._container_id[:12]}")
        except subprocess.CalledProcessError as e:
            raise DeploymentStartupError(f"Failed to start container: {e.stderr}")

    def _get_server_script(self) -> str:
        """Get the Python script to run inside the container.

        This is an inline script that creates a minimal FastAPI server.
        For production, you'd copy the actual server module into the image.
        """
        return '''
import os
import sys

# Install required packages
import subprocess
subprocess.run([sys.executable, "-m", "pip", "install", "-q", "fastapi", "uvicorn", "pexpect", "httpx"], check=True)

# Now import and run
from fastapi import FastAPI, Header, HTTPException
from pydantic import BaseModel
import pexpect
import re
import time
import asyncio

app = FastAPI()

# Auth token from environment
AUTH_TOKEN = os.environ.get("OPENDEV_AUTH_TOKEN")
PORT = int(os.environ.get("OPENDEV_PORT", "8000"))

# Session storage
sessions = {}

class BashAction(BaseModel):
    command: str
    session: str = "default"
    timeout: float = 120.0
    check: str = "silent"

class BashObservation(BaseModel):
    output: str
    exit_code: int | None
    failure_reason: str | None = None

class CreateSessionRequest(BaseModel):
    session: str = "default"
    startup_timeout: float = 10.0

class CreateSessionResponse(BaseModel):
    success: bool
    session: str
    message: str = ""

class ReadFileRequest(BaseModel):
    path: str

class WriteFileRequest(BaseModel):
    path: str
    content: str

def verify_auth(x_api_key: str = Header(None)):
    if AUTH_TOKEN and x_api_key != AUTH_TOKEN:
        raise HTTPException(status_code=401, detail="Invalid API key")

def strip_ansi(s: str) -> str:
    return re.sub(r"\\x1B[@-_][0-?]*[ -/]*[@-~]", "", s).replace("\\r\\n", "\\n")

PS1 = "OPENDEV_PS1> "

@app.get("/is_alive")
async def is_alive(x_api_key: str = Header(None)):
    verify_auth(x_api_key)
    return {"status": "ok"}

@app.post("/create_session")
async def create_session(req: CreateSessionRequest, x_api_key: str = Header(None)):
    verify_auth(x_api_key)
    if req.session in sessions:
        return CreateSessionResponse(success=False, session=req.session, message="Session exists")

    shell = pexpect.spawn("/bin/bash --norc --noprofile", encoding="utf-8", echo=False)
    time.sleep(0.2)
    shell.sendline(f"export PS1=\\"{PS1}\\" PS2=\\"\\" PS0=\\"\\"; export PAGER=cat")
    shell.expect(PS1, timeout=req.startup_timeout)
    # Start in /workspace where files are written
    shell.sendline("cd /workspace 2>/dev/null || cd /testbed 2>/dev/null || true")
    shell.expect(PS1, timeout=req.startup_timeout)
    sessions[req.session] = shell
    return CreateSessionResponse(success=True, session=req.session)

@app.post("/run_in_session")
async def run_in_session(action: BashAction, x_api_key: str = Header(None)):
    verify_auth(x_api_key)

    if action.session not in sessions:
        # Auto-create default session
        if action.session == "default":
            await create_session(CreateSessionRequest(session="default"), x_api_key)
        else:
            return BashObservation(output="", exit_code=None, failure_reason=f"Session {action.session} not found")

    shell = sessions[action.session]
    shell.sendline(action.command)

    try:
        shell.expect(PS1, timeout=action.timeout)
    except pexpect.TIMEOUT:
        return BashObservation(output="", exit_code=None, failure_reason=f"Timeout after {action.timeout}s")

    output = strip_ansi(shell.before or "").strip()

    # Get exit code
    exit_code = None
    if action.check != "ignore":
        shell.sendline("echo EXIT_CODE_$?_END")
        try:
            shell.expect("EXIT_CODE_", timeout=2)
            shell.expect("_END", timeout=2)
            match = re.search(r"(\\d+)", shell.before or "")
            if match:
                exit_code = int(match.group(1))
            shell.expect(PS1, timeout=1)
        except:
            pass

    return BashObservation(output=output, exit_code=exit_code)

@app.post("/close_session")
async def close_session(req: dict, x_api_key: str = Header(None)):
    verify_auth(x_api_key)
    session = req.get("session", "default")
    if session in sessions:
        try:
            sessions[session].close(force=True)
        except:
            pass
        del sessions[session]
    return {"success": True}

@app.post("/read_file")
async def read_file(req: ReadFileRequest, x_api_key: str = Header(None)):
    verify_auth(x_api_key)
    try:
        with open(req.path, "r") as f:
            return {"success": True, "content": f.read()}
    except Exception as e:
        return {"success": False, "content": "", "error": str(e)}

@app.post("/write_file")
async def write_file(req: WriteFileRequest, x_api_key: str = Header(None)):
    verify_auth(x_api_key)
    try:
        import os
        os.makedirs(os.path.dirname(req.path) or ".", exist_ok=True)
        with open(req.path, "w") as f:
            f.write(req.content)
        return {"success": True}
    except Exception as e:
        return {"success": False, "error": str(e)}

@app.post("/close")
async def close(x_api_key: str = Header(None)):
    verify_auth(x_api_key)
    for s in list(sessions.values()):
        try:
            s.close(force=True)
        except:
            pass
    sessions.clear()
    return {"status": "ok"}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT, log_level="warning")
'''

    async def stop(self) -> None:
        """Stop and remove the Docker container."""
        if not self._started and self._container_id is None:
            return

        self.on_status("Stopping container...")
        logger.info(f"Stopping container: {self._container_name}")

        # Close runtime client
        if self._runtime:
            try:
                await self._runtime.close()
            except Exception:
                pass
            self._runtime = None

        # Stop container
        if self._container_id:
            try:
                _run_docker_command(
                    ["stop", "-t", "5", self._container_id],
                    timeout=30.0,
                    check=False,
                )
            except Exception as e:
                logger.warning(f"Error stopping container: {e}")

            # Force remove if still exists
            try:
                _run_docker_command(
                    ["rm", "-f", self._container_id],
                    check=False,
                )
            except Exception:
                pass

            self._container_id = None

        self._started = False
        logger.info("Container stopped")

    async def __aenter__(self) -> "DockerDeployment":
        """Context manager entry."""
        await self.start()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Context manager exit."""
        await self.stop()
