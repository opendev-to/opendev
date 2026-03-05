"""BashSession for interactive shell management using pexpect.

Based on SWE-ReX BashSession implementation.
"""

from __future__ import annotations

import logging
import os
import re
import subprocess
import time
from typing import TYPE_CHECKING

import pexpect

from .exceptions import (
    BashSyntaxError,
    CommandTimeoutError,
    NoExitCodeError,
    NonZeroExitCodeError,
    SessionNotInitializedError,
)
from .models import BashAction, BashObservation, CreateSessionRequest, CreateSessionResponse

if TYPE_CHECKING:
    pass

__all__ = ["BashSession"]

logger = logging.getLogger(__name__)


def _strip_control_chars(s: str) -> str:
    """Remove ANSI control characters from string."""
    ansi_escape = re.compile(r"\x1B[@-_][0-?]*[ -/]*[@-~]")
    return ansi_escape.sub("", s).replace("\r\n", "\n")


def _check_bash_syntax(command: str) -> None:
    """Check if a bash command has valid syntax.

    Raises:
        BashSyntaxError: If the command has syntax errors.
    """
    unique_eof = "OPENDEV_SYNTAX_CHECK_EOF"
    cmd = f"/usr/bin/env bash -n << '{unique_eof}'\n{command}\n{unique_eof}"
    result = subprocess.run(cmd, shell=True, capture_output=True)
    if result.returncode == 0:
        return
    stderr = result.stderr.decode(errors="backslashreplace")
    raise BashSyntaxError(command, stderr)


class BashSession:
    """Interactive bash session using pexpect.

    This manages a single REPL-like bash shell that we can send commands to
    and receive output from.
    """

    _UNIQUE_STRING = "OPENDEV_UNIQUE_29234"
    _EXIT_CODE_PREFIX = "EXITCODE_START_"
    _EXIT_CODE_SUFFIX = "_EXITCODE_END"

    def __init__(
        self,
        name: str = "default",
        startup_timeout: float = 10.0,
    ):
        """Initialize a bash session.

        Args:
            name: Session identifier
            startup_timeout: Timeout for session startup in seconds
        """
        self.name = name
        self.startup_timeout = startup_timeout
        self._ps1 = "OPENDEV_PS1_PROMPT> "
        self._shell: pexpect.spawn | None = None

    @property
    def shell(self) -> pexpect.spawn:
        """Get the pexpect shell, raising if not initialized."""
        if self._shell is None:
            raise SessionNotInitializedError(self.name)
        return self._shell

    @property
    def is_alive(self) -> bool:
        """Check if the shell is alive."""
        return self._shell is not None and self._shell.isalive()

    async def start(self) -> CreateSessionResponse:
        """Start the bash session.

        Spawns a bash shell, sets up custom PS1, and waits for prompt.
        """
        # Set up environment with custom PS1
        env = os.environ.copy()
        env.update({
            "PS1": self._ps1,
            "PS2": "",
            "PS0": "",
            "TERM": "dumb",  # Disable fancy terminal features
        })

        # Spawn bash
        self._shell = pexpect.spawn(
            "/usr/bin/env bash --norc --noprofile",
            encoding="utf-8",
            codec_errors="backslashreplace",
            echo=False,
            env=env,
        )

        # Small delay for shell startup
        time.sleep(0.2)

        # Reset PS1/PS2/PS0 inside shell (in case env vars didn't take)
        reset_cmds = [
            f"export PS1='{self._ps1}'",
            "export PS2=''",
            "export PS0=''",
            "export PAGER=cat",  # Disable paging
            "export GIT_PAGER=cat",
        ]
        self.shell.sendline(" ; ".join(reset_cmds))

        try:
            self.shell.expect(self._ps1, timeout=self.startup_timeout)
        except pexpect.TIMEOUT as e:
            raise CommandTimeoutError(self.startup_timeout, "session startup") from e

        output = _strip_control_chars(self.shell.before or "")

        return CreateSessionResponse(
            success=True,
            session=self.name,
            message=output,
        )

    async def run(self, action: BashAction) -> BashObservation:
        """Execute a command in the session.

        Args:
            action: The bash action to execute

        Returns:
            BashObservation with output and exit code

        Raises:
            SessionNotInitializedError: If session not started
            CommandTimeoutError: If command times out
            NonZeroExitCodeError: If check="raise" and exit code != 0
        """
        if self._shell is None:
            raise SessionNotInitializedError(self.name)

        # Optionally check syntax first (can be slow, so skip for simple commands)
        if "\n" in action.command or ";" in action.command:
            try:
                _check_bash_syntax(action.command)
            except BashSyntaxError:
                # Log but don't fail - let bash report the error
                logger.debug(f"Bash syntax check failed for: {action.command[:50]}...")

        # Send the command
        self.shell.sendline(action.command)

        # Wait for PS1 prompt
        try:
            self.shell.expect(self._ps1, timeout=action.timeout)
        except pexpect.TIMEOUT as e:
            raise CommandTimeoutError(action.timeout, action.command) from e

        output = _strip_control_chars(self.shell.before or "")

        # Get exit code if check is not "ignore"
        exit_code: int | None = None
        if action.check != "ignore":
            exit_code = await self._get_exit_code()

        # Clean up output
        output = output.strip()

        # Check for errors if requested
        if action.check == "raise" and exit_code is not None and exit_code != 0:
            raise NonZeroExitCodeError(exit_code, action.command, output)

        return BashObservation(
            output=output,
            exit_code=exit_code,
            failure_reason=None if exit_code == 0 else f"Exit code {exit_code}",
        )

    async def _get_exit_code(self) -> int | None:
        """Extract the exit code from the last command."""
        try:
            self.shell.sendline(f"echo {self._EXIT_CODE_PREFIX}$?{self._EXIT_CODE_SUFFIX}")
            self.shell.expect(self._EXIT_CODE_SUFFIX, timeout=2.0)

            output = _strip_control_chars(self.shell.before or "")
            match = re.search(f"{self._EXIT_CODE_PREFIX}(\\d+)", output)

            if not match:
                logger.warning(f"Could not extract exit code from: {output[:100]}")
                return None

            exit_code = int(match.group(1))

            # Consume the following PS1
            try:
                self.shell.expect(self._ps1, timeout=0.5)
            except pexpect.TIMEOUT:
                pass  # OK if no PS1

            return exit_code

        except pexpect.TIMEOUT:
            logger.warning("Timeout while getting exit code")
            return None
        except Exception as e:
            logger.warning(f"Error getting exit code: {e}")
            return None

    async def close(self) -> None:
        """Close the bash session."""
        if self._shell is not None:
            try:
                self._shell.sendline("exit")
                self._shell.expect(pexpect.EOF, timeout=2.0)
            except Exception:
                pass  # Ignore errors during cleanup
            finally:
                try:
                    self._shell.close(force=True)
                except Exception:
                    pass
                self._shell = None

    def interrupt(self) -> str:
        """Send interrupt signal (Ctrl+C) to the session.

        Returns:
            Any output captured after the interrupt
        """
        if self._shell is None:
            return ""

        self._shell.sendintr()
        time.sleep(0.2)

        try:
            self._shell.expect(self._ps1, timeout=2.0)
            return _strip_control_chars(self.shell.before or "")
        except pexpect.TIMEOUT:
            return ""
