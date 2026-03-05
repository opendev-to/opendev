"""Process-oriented tool handlers (run command & process management)."""

from __future__ import annotations

import asyncio
import re
from datetime import datetime
from typing import Any

from swecli.core.context_engineering.tools.context import ToolExecutionContext
from swecli.core.context_engineering.tools.implementations.bash_tool import truncate_output
from swecli.models.operation import Operation, OperationType


class ProcessToolHandler:
    """Encapsulates bash execution and process monitoring tools."""

    _SERVER_PATTERNS = (
        r"flask\s+run",
        r"python.*app\.py",
        r"python.*manage\.py\s+runserver",
        r"django.*runserver",
        r"uvicorn",
        r"gunicorn",
        r"python.*-m\s+http\.server",
        r"npm\s+(run\s+)?(start|dev|serve)",
        r"yarn\s+(run\s+)?(start|dev|serve)",
        r"node.*server",
        r"nodemon",
        r"next\s+(dev|start)",
        r"rails\s+server",
        r"php.*artisan\s+serve",
        r"hugo\s+server",
        r"jekyll\s+serve",
    )

    def __init__(self, bash_tool: Any) -> None:
        self._bash_tool = bash_tool

    def run_command(self, args: dict[str, Any], context: ToolExecutionContext) -> dict[str, Any]:
        if not self._bash_tool:
            return {"success": False, "error": "BashTool not available"}

        command = args["command"]
        background = args.get("background", False)

        if not background and self._is_server_command(command):
            background = True

        operation = Operation(
            id=str(hash(f"{command}{datetime.now()}")),
            type=OperationType.BASH_EXECUTE,
            target=command,
            parameters={"command": command, "background": background},
            created_at=datetime.now(),
        )

        if not self._ensure_command_approval(command, background, operation, context):
            return {
                "success": False,
                "interrupted": True,
                "denied": True,
                "output": None,
            }

        # Create output callback for streaming bash output to UI
        output_callback = None
        if context.ui_callback and hasattr(context.ui_callback, 'on_bash_output_line'):
            def _output_callback(line: str, is_stderr: bool = False) -> None:
                context.ui_callback.on_bash_output_line(line, is_stderr)
            output_callback = _output_callback

        result = self._bash_tool.execute(
            command,
            background=background,
            operation=operation,
            task_monitor=context.task_monitor,
            auto_confirm=getattr(context, "is_subagent", False),
            output_callback=output_callback,
        )

        if result.success and context.undo_manager:
            context.undo_manager.record_operation(operation)

        output_parts = [part for part in (result.stdout, result.stderr) if part]
        combined_output = truncate_output("\n".join(output_parts))

        if result.success:
            return {
                "success": True,
                "output": combined_output or "Command executed",
                "stdout": result.stdout,
                "stderr": result.stderr,
                "exit_code": result.exit_code,
                "error": None,
            }

        error_parts = [p for p in (result.error, combined_output) if p]
        error_message = "\n".join(error_parts) if error_parts else "Command execution failed"
        # Detect if the command was interrupted by user (Fix 5)
        interrupted = "interrupted" in (result.error or "").lower()
        return {
            "success": False,
            "output": combined_output or None,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "error": error_message,
            "interrupted": interrupted,
        }

    def list_processes(self) -> dict[str, Any]:
        if not self._bash_tool:
            return {"success": False, "error": "BashTool not available"}

        try:
            processes = self._bash_tool.list_processes()
            if not processes:
                output = "No background processes running"
            else:
                lines = []
                for proc in processes:
                    line = (
                        f"PID {proc['pid']}: {proc['command'][:60]} "
                        f"({proc['status']}, {proc['runtime']:.1f}s)"
                    )
                    if proc["exit_code"] is not None:
                        line += f" [exit code: {proc['exit_code']}]"
                    lines.append(line)
                output = "\n".join(lines)

            return {"success": True, "output": output, "error": None}
        except Exception as exc:  # noqa: BLE001
            return {"success": False, "error": str(exc), "output": None}

    def get_process_output(self, args: dict[str, Any]) -> dict[str, Any]:
        if not self._bash_tool:
            return {"success": False, "error": "BashTool not available"}

        pid = args["pid"]
        try:
            result = self._bash_tool.get_process_output(pid)
            if not result["success"]:
                return {"success": False, "error": result["error"], "output": None}

            lines = [
                f"Process {pid}: {result['command'][:60]}",
                f"Status: {result['status']}",
                f"Runtime: {result['runtime']:.1f}s",
            ]
            if result["exit_code"] is not None:
                lines.append(f"Exit code: {result['exit_code']}")
            if result["stdout"]:
                lines.append(f"\nStdout:\n{result['stdout']}")
            if result["stderr"]:
                lines.append(f"\nStderr:\n{result['stderr']}")

            return {"success": True, "output": "\n".join(lines), "error": None}
        except Exception as exc:  # noqa: BLE001
            return {"success": False, "error": str(exc), "output": None}

    def kill_process(self, args: dict[str, Any]) -> dict[str, Any]:
        if not self._bash_tool:
            return {"success": False, "error": "BashTool not available"}

        pid = args["pid"]
        signal = args.get("signal", 15)

        try:
            result = self._bash_tool.kill_process(pid, signal)
            if not result["success"]:
                return {"success": False, "error": result["error"], "output": None}
            return {"success": True, "output": result["message"], "error": None}
        except Exception as exc:  # noqa: BLE001
            return {"success": False, "error": str(exc), "output": None}

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------
    def _ensure_command_approval(
        self,
        command: str,
        background: bool,
        operation: Operation,
        context: ToolExecutionContext,
    ) -> bool:
        mode_manager = context.mode_manager
        if mode_manager and not mode_manager.needs_approval(operation):
            operation.approved = True
            return True

        approval_manager = context.approval_manager
        if not approval_manager:
            operation.approved = True
            return True

        # Early exit if already interrupted - don't show approval modal
        if context.task_monitor and context.task_monitor.should_interrupt():
            return False

        if hasattr(approval_manager, "pre_approved_commands") and command in approval_manager.pre_approved_commands:
            approval_manager.pre_approved_commands.discard(command)
            operation.approved = True
            return True

        preview = f"Execute{' (background)' if background else ''}: {command}"
        working_dir = str(self._bash_tool.working_dir) if getattr(self._bash_tool, "working_dir", None) else "."

        try:
            asyncio.get_running_loop()
        except RuntimeError:
            # Check if request_approval already returned a result (WebApprovalManager) or needs to be awaited
            approval_result = approval_manager.request_approval(
                operation=operation,
                preview=preview,
                command=command,
                working_dir=working_dir,
                force_prompt=True,
            )

            # If it's already a result object, use it directly
            if hasattr(approval_result, 'approved'):
                result = approval_result
            else:
                # If it's a coroutine, run it
                result = asyncio.run(approval_result)

            if not result.approved:
                return False
            operation.approved = True
            return True

        operation.approved = True
        return True

    @classmethod
    def _is_server_command(cls, command: str) -> bool:
        return any(re.search(pattern, command, re.IGNORECASE) for pattern in cls._SERVER_PATTERNS)
