"""Subprocess runner for hook commands."""

from __future__ import annotations

import json
import logging
import subprocess
from dataclasses import dataclass, field
from typing import Any, Optional

from swecli.core.hooks.models import HookCommand

logger = logging.getLogger(__name__)


@dataclass
class HookResult:
    """Result from executing a single hook command."""

    exit_code: int = 0
    stdout: str = ""
    stderr: str = ""
    timed_out: bool = False
    error: Optional[str] = None

    @property
    def success(self) -> bool:
        """Hook succeeded (exit code 0)."""
        return self.exit_code == 0 and not self.timed_out and self.error is None

    @property
    def should_block(self) -> bool:
        """Hook requests blocking the operation (exit code 2)."""
        return self.exit_code == 2

    def parse_json_output(self) -> dict[str, Any]:
        """Parse stdout as JSON.

        Returns:
            Parsed JSON dict, or empty dict if stdout is not valid JSON.
        """
        if not self.stdout.strip():
            return {}
        try:
            return json.loads(self.stdout)
        except (json.JSONDecodeError, ValueError):
            return {}


class HookCommandExecutor:
    """Executes hook commands as subprocesses."""

    def execute(self, command: HookCommand, stdin_data: dict[str, Any]) -> HookResult:
        """Execute a hook command.

        The command receives JSON on stdin and communicates via exit codes
        and optional JSON on stdout.

        Exit codes:
            0: Success (operation proceeds)
            2: Block (operation is denied)
            Other: Error (logged, operation proceeds)

        Args:
            command: The hook command to execute.
            stdin_data: JSON data to pass on stdin.

        Returns:
            HookResult with exit code, stdout, stderr, and status.
        """
        stdin_json = json.dumps(stdin_data)

        try:
            result = subprocess.run(
                command.command,
                shell=True,
                input=stdin_json,
                capture_output=True,
                text=True,
                timeout=command.timeout,
            )
            return HookResult(
                exit_code=result.returncode,
                stdout=result.stdout,
                stderr=result.stderr,
            )
        except subprocess.TimeoutExpired:
            logger.warning(
                "Hook command timed out after %ds: %s", command.timeout, command.command
            )
            return HookResult(
                exit_code=1,
                timed_out=True,
                error=f"Hook timed out after {command.timeout}s",
            )
        except OSError as e:
            logger.error("Hook command failed to execute: %s — %s", command.command, e)
            return HookResult(
                exit_code=1,
                error=f"Failed to execute hook: {e}",
            )
