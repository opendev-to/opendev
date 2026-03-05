"""DockerToolHandler - Routes tool calls to Docker runtime.

This handler executes tools inside the Docker container instead of locally.
It translates swecli tool calls into RemoteRuntime operations.
"""

from __future__ import annotations

import asyncio
import logging
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import TYPE_CHECKING, Any, Coroutine, TypeVar, Union

if TYPE_CHECKING:
    from .remote_runtime import RemoteRuntime

__all__ = ["DockerToolHandler", "DockerToolRegistry"]

logger = logging.getLogger(__name__)

T = TypeVar("T")


def _run_async(coro: Coroutine[Any, Any, T]) -> T:
    """Run an async coroutine, handling both nested and standalone event loops.

    When called from within a running event loop (e.g., Textual UI), we can't use
    asyncio.run() directly. This helper detects that case and runs the coroutine
    in a separate thread with its own event loop.

    Args:
        coro: The coroutine to execute

    Returns:
        The result of the coroutine
    """
    try:
        # Check if there's already a running event loop
        asyncio.get_running_loop()
    except RuntimeError:
        # No running loop - we can use asyncio.run() safely
        return asyncio.run(coro)

    # There's a running loop - run in a separate thread
    with ThreadPoolExecutor(max_workers=1) as executor:
        future = executor.submit(asyncio.run, coro)
        return future.result()


class DockerToolHandler:
    """Execute tools via Docker runtime instead of local subprocess.

    This handler wraps a RemoteRuntime and provides methods that match
    the swecli tool interface, translating calls to HTTP operations.
    """

    def __init__(
        self,
        runtime: "RemoteRuntime",
        workspace_dir: str = "/testbed",
        shell_init: str = "",
    ):
        """Initialize the Docker tool handler.

        Args:
            runtime: RemoteRuntime instance for communicating with container
            workspace_dir: Directory inside container where repo is located
                          (default: /testbed for SWE-bench images)
            shell_init: Shell initialization command to prepend to all commands
                       (e.g., conda activation for SWE-bench, empty for uv images)
        """
        self.runtime = runtime
        self.workspace_dir = workspace_dir
        self.shell_init = shell_init

    async def run_command(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Execute a command inside the Docker container.

        Args:
            arguments: Tool arguments with 'command', 'timeout', 'working_dir'
            context: Tool execution context (unused in Docker mode)

        Returns:
            Result dict with success, output, exit_code
        """
        from .models import BashAction

        command = arguments.get("command", "")
        timeout = arguments.get("timeout", 120.0)
        working_dir = arguments.get("working_dir")

        if not command:
            return {
                "success": False,
                "error": "command is required",
                "output": None,
            }

        # Prepend cd if working_dir specified
        if working_dir:
            # Translate host path to container path if needed
            container_path = self._translate_path(working_dir)
            command = f"cd {container_path} && {command}"

        # Prepend shell initialization if configured
        # (e.g., conda activation for SWE-bench, empty for uv/plain images)
        if self.shell_init:
            command = f"{self.shell_init} && {command}"

        try:
            action = BashAction(
                command=command,
                timeout=timeout,
                check="silent",  # Don't raise on non-zero exit
            )
            obs = await self.runtime.run_in_session(action)

            return {
                "success": obs.exit_code == 0 or obs.exit_code is None,
                "output": obs.output,
                "exit_code": obs.exit_code,
                "error": obs.failure_reason if obs.exit_code != 0 else None,
            }
        except Exception as e:
            logger.error(f"Docker run_command failed: {e}")
            return {
                "success": False,
                "error": str(e),
                "output": None,
            }

    async def read_file(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Read a file from inside the Docker container.

        Args:
            arguments: Tool arguments with 'path'

        Returns:
            Result dict with success, content
        """
        # Accept both "file_path" (standard) and "path" (legacy) argument names
        path = arguments.get("file_path") or arguments.get("path", "")
        if not path:
            return {
                "success": False,
                "error": "file_path or path is required",
                "output": None,
            }

        # Translate path to container path
        container_path = self._translate_path(path)

        try:
            content = await self.runtime.read_file(container_path)
            return {
                "success": True,
                "output": content,
                "content": content,
            }
        except Exception as e:
            return {
                "success": False,
                "error": str(e),
                "output": None,
            }

    async def write_file(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Write a file inside the Docker container.

        Args:
            arguments: Tool arguments with 'path', 'content'
            context: Tool execution context (unused in Docker mode)

        Returns:
            Result dict with success status
        """
        # Accept both "file_path" (standard) and "path" (legacy) argument names
        path = arguments.get("file_path") or arguments.get("path", "")
        content = arguments.get("content", "")

        # Debug: Confirm Docker write is being used
        logger.info(f"DockerToolHandler.write_file called with path: {path}")

        if not path:
            return {
                "success": False,
                "error": "file_path or path is required",
                "output": None,
            }

        # Translate path to container path
        container_path = self._translate_path(path)
        logger.info(f"  → Translated to Docker path: {container_path}")

        try:
            await self.runtime.write_file(container_path, content)
            return {
                "success": True,
                "output": f"Wrote {len(content)} bytes to {container_path}",
            }
        except Exception as e:
            return {
                "success": False,
                "error": str(e),
                "output": None,
            }

    async def edit_file(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Edit a file inside the Docker container using sed-like replacement.

        Args:
            arguments: Tool arguments with 'path', 'old_text', 'new_text'
            context: Tool execution context (unused in Docker mode)

        Returns:
            Result dict with success status, diff, lines_added, lines_removed
        """
        # Accept both standard and legacy argument names
        path = arguments.get("file_path") or arguments.get("path", "")
        old_text = arguments.get("old_content") or arguments.get("old_text", "")
        new_text = arguments.get("new_content") or arguments.get("new_text", "")

        if not path:
            return {
                "success": False,
                "error": "file_path or path is required",
                "output": None,
            }

        if not old_text:
            return {
                "success": False,
                "error": "old_content or old_text is required for editing",
                "output": None,
            }

        container_path = self._translate_path(path)

        try:
            # Read current content
            content = await self.runtime.read_file(container_path)

            # Check if old_text exists (with fuzzy matching fallback)
            found, actual_old_text = self._find_content(content, old_text)
            if not found:
                return {
                    "success": False,
                    "error": f"old_text not found in {container_path}",
                    "output": None,
                }

            # Perform replacement using actual matched content
            new_content = content.replace(actual_old_text, new_text, 1)

            # Calculate diff statistics before writing
            from swecli.core.context_engineering.tools.implementations.diff_preview import Diff
            diff = Diff(container_path, content, new_content)
            stats = diff.get_stats()
            diff_text = diff.generate_unified_diff(context_lines=3)

            # Write back
            await self.runtime.write_file(container_path, new_content)

            return {
                "success": True,
                "output": f"Edited {container_path}",
                "file_path": container_path,
                "lines_added": stats["lines_added"],
                "lines_removed": stats["lines_removed"],
                "diff": diff_text,
            }
        except Exception as e:
            return {
                "success": False,
                "error": str(e),
                "output": None,
            }

    async def list_files(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """List files in a directory inside the Docker container.

        Args:
            arguments: Tool arguments with 'path', 'pattern', 'recursive'

        Returns:
            Result dict with file listing
        """
        # Accept multiple naming conventions for directory path
        path = (
            arguments.get("directory")
            or arguments.get("dir_path")
            or arguments.get("path", ".")
        )
        pattern = arguments.get("pattern", "*")
        recursive = arguments.get("recursive", False)

        container_path = self._translate_path(path)

        try:
            if recursive:
                cmd = f"find {container_path} -name '{pattern}' -type f 2>/dev/null | head -100"
            else:
                cmd = f"ls -la {container_path} 2>/dev/null"

            obs = await self.runtime.run(cmd, timeout=30.0)

            if obs.exit_code != 0:
                # Provide informative error message
                error_msg = obs.failure_reason or obs.output or f"Directory not found: {container_path}"
                return {
                    "success": False,
                    "output": None,
                    "error": error_msg,
                }

            return {
                "success": True,
                "output": obs.output or "(empty directory)",
                "error": None,
            }
        except Exception as e:
            return {
                "success": False,
                "error": f"Failed to list files in {container_path}: {str(e)}",
                "output": None,
            }

    async def search(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Search for text in files inside the Docker container.

        Args:
            arguments: Tool arguments with 'query', 'path', 'type'

        Returns:
            Result dict with search results
        """
        # Accept both "pattern" (standard) and "query" (legacy) argument names
        query = arguments.get("pattern") or arguments.get("query", "")
        path = arguments.get("file_path") or arguments.get("path", ".")
        search_type = arguments.get("type", "text")

        if not query:
            return {
                "success": False,
                "error": "pattern or query is required for search",
                "output": None,
            }

        container_path = self._translate_path(path)

        try:
            if search_type == "text":
                # Use grep for text search
                cmd = f"grep -rn --include='*.py' --include='*.js' --include='*.ts' '{query}' {container_path} 2>/dev/null | head -50"
            else:
                # For AST search, fall back to grep (ast-grep may not be in container)
                cmd = f"grep -rn '{query}' {container_path} 2>/dev/null | head -50"

            obs = await self.runtime.run(cmd, timeout=60.0)

            return {
                "success": True,  # grep returns 1 if no matches, but that's OK
                "output": obs.output or "No matches found",
            }
        except Exception as e:
            return {
                "success": False,
                "error": str(e),
                "output": None,
            }

    def _translate_path(self, path: str) -> str:
        """Translate a host path to a container path.

        If the path is already absolute and starts with /workspace, use as-is.
        Otherwise, assume it's relative to the workspace.

        Args:
            path: Host path or relative path

        Returns:
            Container path
        """
        if not path:
            return self.workspace_dir

        # If it's already a container path, use as-is
        if path.startswith("/testbed") or path.startswith("/workspace"):
            return path

        # Relative path - prepend workspace (strip leading ./)
        if not path.startswith("/"):
            clean_path = path.lstrip("./")
            return f"{self.workspace_dir}/{clean_path}"

        # Absolute host path (e.g., /Users/.../file.py)
        # Extract just the filename - safest for Docker since we can't know
        # the original repo structure
        try:
            p = Path(path)
            return f"{self.workspace_dir}/{p.name}"
        except Exception:
            pass

        # Fallback: just use the path as-is under workspace
        return f"{self.workspace_dir}/{path}"

    def _find_content(self, original: str, old_content: str) -> tuple[bool, str]:
        """Find content in file, with fallback to normalized matching.

        When exact match fails, tries to find content by normalizing whitespace
        (stripping each line, normalizing line endings) and then locating the
        actual content in the original file.

        Args:
            original: The original file content
            old_content: The content to find

        Returns:
            (found, actual_content) - actual_content is what should be replaced
        """
        # Try exact match first (fast path)
        if old_content in original:
            return (True, old_content)

        # Normalize: strip each line, normalize line endings
        def normalize(s: str) -> str:
            lines = s.replace("\r\n", "\n").replace("\r", "\n").split("\n")
            return "\n".join(line.strip() for line in lines)

        norm_old = normalize(old_content)
        norm_original = normalize(original)

        # If normalized content not found, give up
        if norm_old not in norm_original:
            return (False, old_content)

        # Find actual content in original by line matching
        old_lines = [ln.strip() for ln in old_content.split("\n") if ln.strip()]
        if not old_lines:
            return (False, old_content)

        original_lines = original.split("\n")

        # Find start line that matches first stripped line
        for i, line in enumerate(original_lines):
            if line.strip() == old_lines[0]:
                # Try to match all subsequent lines
                matched_lines = []
                j = 0  # Index into old_lines
                for k in range(i, min(i + len(old_lines) * 2, len(original_lines))):
                    if j >= len(old_lines):
                        break
                    if original_lines[k].strip() == old_lines[j]:
                        matched_lines.append(original_lines[k])
                        j += 1

                if j == len(old_lines):
                    # Found all lines - reconstruct actual content
                    actual = "\n".join(matched_lines)
                    # Check if we need trailing newline
                    if actual in original:
                        return (True, actual)
                    if actual + "\n" in original:
                        return (True, actual + "\n")

        return (False, old_content)

    # Synchronous wrappers for use with MainAgent (which expects sync handlers)

    def _create_fresh_handler(self) -> "DockerToolHandler":
        """Create a fresh handler with a new runtime for thread-safe execution."""
        from .remote_runtime import RemoteRuntime

        fresh_runtime = RemoteRuntime(
            host=self.runtime.host,
            port=self.runtime.port,
            auth_token=self.runtime.auth_token,
            timeout=self.runtime.timeout,
        )
        return DockerToolHandler(fresh_runtime, self.workspace_dir, self.shell_init)

    def run_command_sync(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Synchronous wrapper for run_command.

        Always creates a fresh handler to avoid event loop issues with cached
        HTTP sessions. Each call gets a fresh RemoteRuntime/aiohttp session.
        """
        fresh = self._create_fresh_handler()
        return _run_async(fresh.run_command(arguments, context))

    def read_file_sync(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Synchronous wrapper for read_file."""
        fresh = self._create_fresh_handler()
        return _run_async(fresh.read_file(arguments))

    def write_file_sync(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Synchronous wrapper for write_file."""
        fresh = self._create_fresh_handler()
        return _run_async(fresh.write_file(arguments, context))

    def edit_file_sync(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Synchronous wrapper for edit_file."""
        fresh = self._create_fresh_handler()
        return _run_async(fresh.edit_file(arguments, context))

    def list_files_sync(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Synchronous wrapper for list_files."""
        fresh = self._create_fresh_handler()
        return _run_async(fresh.list_files(arguments))

    def search_sync(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Synchronous wrapper for search."""
        fresh = self._create_fresh_handler()
        return _run_async(fresh.search(arguments))


class DockerToolRegistry:
    """A tool registry that routes tools through Docker.

    This wraps the Docker tool handler to provide a compatible interface
    with the standard ToolRegistry. Uses synchronous wrappers for compatibility
    with MainAgent.

    For tools not supported in Docker (like read_pdf), falls back to the
    local tool registry if provided.
    """

    def __init__(
        self,
        docker_handler: DockerToolHandler,
        local_registry: Any = None,
        path_mapping: dict[str, str] | None = None,
    ):
        """Initialize with a Docker tool handler and optional local fallback.

        Args:
            docker_handler: DockerToolHandler instance
            local_registry: Optional local ToolRegistry for fallback on unsupported tools
            path_mapping: Mapping of Docker paths to local paths for local-only tools
        """
        self.handler = docker_handler
        self._local_registry = local_registry
        self._path_mapping = path_mapping or {}
        # Use sync handlers for compatibility with MainAgent
        self._sync_handlers = {
            "run_command": self.handler.run_command_sync,
            "read_file": self.handler.read_file_sync,
            "write_file": self.handler.write_file_sync,
            "edit_file": self.handler.edit_file_sync,
            "list_files": self.handler.list_files_sync,
            "search": self.handler.search_sync,
        }
        # Tools that should always run locally (not in Docker)
        self._local_only_tools = {"read_pdf", "analyze_image", "capture_screenshot"}
        # Track last run_command result for todo verification (Layer 1 & 2)
        self._last_run_command_result: dict[str, Any] | None = None

    def _remap_paths_to_local(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Remap Docker paths in arguments to local paths.

        Uses the path_mapping to convert Docker paths (e.g., /workspace/paper.pdf)
        back to their original local paths for local-only tool execution.

        Args:
            arguments: Tool arguments that may contain Docker paths

        Returns:
            Arguments with Docker paths replaced by local paths
        """
        if not self._path_mapping:
            return arguments

        remapped = dict(arguments)
        for key, value in remapped.items():
            if isinstance(value, str):
                # Check if this value matches a Docker path in our mapping
                for docker_path, local_path in self._path_mapping.items():
                    # Match exact Docker path or path ending with Docker path
                    if value == docker_path or value.endswith(docker_path):
                        remapped[key] = local_path
                        logger.info(f"  Remapped {key}: {value} → {local_path}")
                        break
                    # Also match by filename (for when LLM outputs just the filename)
                    docker_filename = Path(docker_path).name
                    if value == docker_filename or value.endswith(f"/{docker_filename}"):
                        remapped[key] = local_path
                        logger.info(f"  Remapped {key} (by filename): {value} → {local_path}")
                        break
        return remapped

    def _sanitize_local_paths(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Sanitize local paths in arguments to relative paths.

        This is a safety net that catches any local paths the LLM might output
        and converts them to just filenames for Docker execution.

        Args:
            arguments: Tool arguments that may contain local paths

        Returns:
            Arguments with local paths replaced by filenames
        """
        import re
        sanitized = dict(arguments)
        for key, value in sanitized.items():
            if isinstance(value, str):
                # Match absolute paths starting with /Users/, /home/, /var/, etc.
                match = re.match(r'^(/Users/|/home/|/var/|/tmp/).+/([^/]+)$', value)
                if match:
                    filename = match.group(2)
                    sanitized[key] = filename
                    logger.warning(f"Sanitized local path: {value} → {filename}")
        return sanitized

    def execute_tool(
        self,
        tool_name: str,
        arguments: dict[str, Any],
        *,
        mode_manager: Union[Any, None] = None,
        approval_manager: Union[Any, None] = None,
        undo_manager: Union[Any, None] = None,
        task_monitor: Union[Any, None] = None,
        session_manager: Union[Any, None] = None,
        ui_callback: Union[Any, None] = None,
        is_subagent: bool = False,
    ) -> dict[str, Any]:
        """Execute a tool synchronously via Docker.

        This method matches the ToolRegistry.execute_tool interface so it can
        be used as a drop-in replacement when running in Docker mode.

        Args:
            tool_name: Name of the tool to execute
            arguments: Tool arguments
            mode_manager: Mode manager (unused in Docker)
            approval_manager: Approval manager (unused in Docker)
            undo_manager: Undo manager (unused in Docker)
            task_monitor: Task monitor (unused in Docker)
            session_manager: Session manager (unused in Docker)
            ui_callback: UI callback (unused in Docker)
            is_subagent: Whether running as subagent (unused in Docker)

        Returns:
            Tool execution result
        """
        # Sanitize any local paths in arguments (safety net for LLM outputs)
        arguments = self._sanitize_local_paths(arguments)

        # Logging to trace tool routing (INFO for visibility during testing)
        logger.info(f"DockerToolRegistry.execute_tool: {tool_name}")
        logger.debug(f"  sync_handlers: {list(self._sync_handlers.keys())}")
        logger.debug(f"  local_only_tools: {self._local_only_tools}")

        # Check if tool should run locally (not in Docker)
        if tool_name in self._local_only_tools:
            logger.info(f"  → Routing to LOCAL (local-only tool: {tool_name})")
            if self._local_registry is not None:
                # Remap Docker paths to local paths for local execution
                local_arguments = self._remap_paths_to_local(arguments)
                # Fall back to local registry for this tool
                return self._local_registry.execute_tool(
                    tool_name,
                    local_arguments,
                    mode_manager=mode_manager,
                    approval_manager=approval_manager,
                    undo_manager=undo_manager,
                    task_monitor=task_monitor,
                    session_manager=session_manager,
                    ui_callback=ui_callback,
                    is_subagent=is_subagent,
                )
            else:
                return {
                    "success": False,
                    "error": f"Tool '{tool_name}' requires local execution but no local registry available",
                    "output": None,
                }

        if tool_name not in self._sync_handlers:
            # LAYER 2: Block complete_todo if last run_command failed
            if tool_name == "complete_todo" and self._last_run_command_result:
                last = self._last_run_command_result
                exit_code = last.get("exit_code", 0)
                output = last.get("output", "")

                if self._check_command_has_error(exit_code, output):
                    # Truncate error output for readability
                    error_preview = output[:500] if output else "No output"
                    logger.warning(
                        f"  → Blocked complete_todo: last run_command failed (exit_code={exit_code})"
                    )
                    return {
                        "success": False,
                        "error": (
                            f"Cannot complete todo: last run_command failed.\n\n"
                            f"Exit code: {exit_code}\n"
                            f"Output:\n{error_preview}\n\n"
                            f"Fix the error and run the command successfully before completing this todo."
                        ),
                        "output": None,
                        "blocked_by": "command_verification",
                    }

                # Clear state after successful verification (command succeeded)
                logger.info("  → Cleared _last_run_command_result (command succeeded)")
                self._last_run_command_result = None

            # Try local fallback for unknown tools
            logger.info(f"  → Routing to LOCAL (unknown tool: {tool_name}, not in sync_handlers)")
            if self._local_registry is not None:
                return self._local_registry.execute_tool(
                    tool_name,
                    arguments,
                    mode_manager=mode_manager,
                    approval_manager=approval_manager,
                    undo_manager=undo_manager,
                    task_monitor=task_monitor,
                    session_manager=session_manager,
                    ui_callback=ui_callback,
                    is_subagent=is_subagent,
                )
            return {
                "success": False,
                "error": f"Tool '{tool_name}' not supported in Docker mode",
                "output": None,
            }

        logger.info(f"  → Routing to DOCKER handler: {tool_name}")

        # For run_command, inject default working_dir if not specified
        # This ensures commands run from /workspace where files are written
        if tool_name == "run_command" and "working_dir" not in arguments:
            arguments = dict(arguments)
            arguments["working_dir"] = self.handler.workspace_dir
            logger.info(f"  → Injected default working_dir: {self.handler.workspace_dir}")

        handler = self._sync_handlers[tool_name]
        result = handler(arguments)

        # LAYER 1: Track run_command results and inject retry prompt on failure
        if tool_name == "run_command":
            self._last_run_command_result = result

            # Check for failure indicators
            exit_code = result.get("exit_code", 0)
            output = result.get("output", "")
            has_error = self._check_command_has_error(exit_code, output)

            if has_error:
                # Inject retry prompt to force LLM to fix before proceeding
                # Store in _llm_suffix so UI doesn't display it, only LLM sees it
                from swecli.core.agents.prompts.reminders import get_reminder

                retry_prompt = get_reminder(
                    "docker_command_failed_nudge", exit_code=str(exit_code)
                )
                result = dict(result)
                result["_llm_suffix"] = retry_prompt  # Hidden from UI, visible to LLM
                logger.info("  → Injected retry prompt (command failed)")

        logger.info(f"  → Docker result: success={result.get('success')}")
        return result

    def _check_command_has_error(self, exit_code: int, output: str) -> bool:
        """Check if command output indicates an error.

        Args:
            exit_code: Command exit code
            output: Command output string

        Returns:
            True if the command appears to have failed
        """
        if exit_code != 0:
            return True

        # Check for common error patterns in output
        error_patterns = [
            "Error:",
            "error:",
            "ERROR:",
            "ModuleNotFoundError",
            "ImportError",
            "No such file or directory",
            "SyntaxError",
            "TypeError",
            "ValueError",
            "Traceback (most recent call last)",
            "FileNotFoundError",
            "NameError",
            "AttributeError",
        ]
        for pattern in error_patterns:
            if pattern in output:
                return True

        return False

    def get_tool_specs(self) -> list[dict[str, Any]]:
        """Return tool specifications for the agent.

        Returns the same tool specs as the standard registry so the agent
        knows what tools are available.
        """
        return [
            {
                "name": "run_command",
                "description": "Execute a shell command in the Docker container",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute",
                        },
                        "timeout": {
                            "type": "number",
                            "description": "Timeout in seconds (default: 120)",
                        },
                    },
                    "required": ["command"],
                },
            },
            {
                "name": "read_file",
                "description": "Read a file from the Docker container",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file (relative to /workspace/repo)",
                        },
                    },
                    "required": ["path"],
                },
            },
            {
                "name": "write_file",
                "description": "Write content to a file in the Docker container",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file",
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write",
                        },
                    },
                    "required": ["path", "content"],
                },
            },
            {
                "name": "edit_file",
                "description": "Edit a file by replacing text in the Docker container",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file",
                        },
                        "old_text": {
                            "type": "string",
                            "description": "Text to find and replace",
                        },
                        "new_text": {
                            "type": "string",
                            "description": "Replacement text",
                        },
                    },
                    "required": ["path", "old_text", "new_text"],
                },
            },
            {
                "name": "list_files",
                "description": "List files in a directory in the Docker container",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Directory path",
                        },
                        "pattern": {
                            "type": "string",
                            "description": "File pattern to match",
                        },
                        "recursive": {
                            "type": "boolean",
                            "description": "Search recursively",
                        },
                    },
                },
            },
            {
                "name": "search",
                "description": "Search for text in files in the Docker container",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query",
                        },
                        "path": {
                            "type": "string",
                            "description": "Path to search in",
                        },
                        "type": {
                            "type": "string",
                            "enum": ["text", "ast"],
                            "description": "Search type",
                        },
                    },
                    "required": ["query"],
                },
            },
        ]

    async def execute_tool_async(
        self,
        tool_name: str,
        arguments: dict[str, Any],
        context: Any = None,
    ) -> dict[str, Any]:
        """Execute a tool asynchronously via Docker.

        Args:
            tool_name: Name of the tool to execute
            arguments: Tool arguments
            context: Execution context

        Returns:
            Tool execution result
        """
        # Map to async handlers
        async_handlers = {
            "run_command": self.handler.run_command,
            "read_file": self.handler.read_file,
            "write_file": self.handler.write_file,
            "edit_file": self.handler.edit_file,
            "list_files": self.handler.list_files,
            "search": self.handler.search,
        }

        if tool_name not in async_handlers:
            return {
                "success": False,
                "error": f"Tool '{tool_name}' not supported in Docker mode",
                "output": None,
            }

        handler = async_handlers[tool_name]

        # Check if handler accepts context
        if tool_name in {"run_command", "write_file", "edit_file"}:
            return await handler(arguments, context)
        return await handler(arguments)
