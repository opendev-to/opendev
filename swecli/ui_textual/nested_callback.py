"""Nested UI callback wrapper for subagent tool call display."""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any, Callable, Dict, Optional

from swecli.ui_textual.callback_interface import ForwardingUICallback, UICallbackProtocol

# Re-export for backwards compatibility
UICallback = UICallbackProtocol


@dataclass
class DockerContext:
    """Docker execution context for path sanitization.

    Used by create_subagent_nested_callback to configure Docker-aware
    path sanitization in subagent tool call displays.
    """

    workspace_dir: str
    """Docker workspace path (e.g., '/workspace' or '/testbed')."""

    image_name: str
    """Full Docker image name (e.g., 'ghcr.io/astral-sh/uv:python3.11')."""

    container_id: str
    """Short container ID (last 8 chars of container name)."""

    local_dir: Optional[str] = None
    """Local directory for path remapping (optional)."""


class NestedUICallback(ForwardingUICallback):
    """Wraps a UI callback to add nesting/indentation context for subagent tool calls.

    This wrapper intercepts tool events from subagents and forwards them to the parent
    callback with additional nesting information (depth and parent context).

    Extends ForwardingUICallback to automatically forward methods like on_bash_output_line,
    on_progress_*, on_interrupt, etc. Only methods that need special nesting behavior
    are overridden.

    Optionally sanitizes paths before display (useful for Docker mode where LLM may
    output local filesystem paths that should be shown as relative paths).
    """

    def __init__(
        self,
        parent_callback: Any,
        parent_context: str,
        depth: int = 1,
        path_sanitizer: Optional[Callable[[str], str]] = None,
    ) -> None:
        """Initialize the nested callback wrapper.

        Args:
            parent_callback: The parent UI callback to forward events to
            parent_context: Name/identifier of the parent subagent (e.g., "researcher")
            depth: Nesting depth level (1 = direct child of main agent)
            path_sanitizer: Optional function to sanitize paths before display.
                           Used in Docker mode to convert /Users/.../file.py → file.py
        """
        super().__init__(parent_callback)  # Initialize ForwardingUICallback with parent
        self._context = parent_context
        self._depth = depth
        self._path_sanitizer = path_sanitizer

    def _sanitize_tool_args(
        self, tool_name: str, tool_args: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Sanitize path arguments for display.

        Args:
            tool_name: Name of the tool being called
            tool_args: Original tool arguments from LLM

        Returns:
            Copy of tool_args with paths sanitized if path_sanitizer is set
        """
        if self._path_sanitizer is None:
            return tool_args

        sanitized = dict(tool_args)
        # Sanitize common path argument names
        for key in ("path", "file_path", "working_dir"):
            if key in sanitized and isinstance(sanitized[key], str):
                sanitized[key] = self._path_sanitizer(sanitized[key])

        # For bash commands, inject working_dir with Docker prefix
        # This allows the UI to show where the command is being executed
        if tool_name in ("bash_execute", "run_command"):
            if "working_dir" not in sanitized:
                sanitized["working_dir"] = self._path_sanitizer(".")

        return sanitized

    # Override methods that should NOT forward from subagents:

    def on_thinking_start(self) -> None:
        """Don't forward thinking events from subagents."""
        pass

    def on_thinking_complete(self) -> None:
        """Don't forward thinking events from subagents."""
        pass

    def on_assistant_message(self, content: str) -> None:
        """Don't forward assistant messages from subagents (they appear as final result)."""
        pass

    # Override methods that need special nesting behavior:

    def on_tool_call(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        tool_call_id: Optional[str] = None,
    ) -> None:
        """Called when a tool call is about to be executed.

        Forwards to parent as a nested tool call with depth information.
        Sanitizes paths before display if a path_sanitizer is configured.

        Args:
            tool_name: Name of the tool being called
            tool_args: Arguments for the tool call
            tool_call_id: Optional unique ID for this tool call (for parallel tracking)
        """
        if self._parent is None:
            return

        # Sanitize paths for display (e.g., /Users/.../file.py → [uv:id]:/workspace/file.py)
        display_args = self._sanitize_tool_args(tool_name, tool_args)

        # Generate tool_id if not provided
        tool_id = tool_call_id or f"{self._context}_{tool_name}_{id(tool_args)}"

        # Check if parent supports nested tool calls
        if hasattr(self._parent, "on_nested_tool_call"):
            self._parent.on_nested_tool_call(
                tool_name,
                display_args,
                depth=self._depth,
                parent=self._context,
                tool_id=tool_id,
            )
        elif hasattr(self._parent, "on_tool_call"):
            # Fallback: use regular tool call (loses nesting info)
            self._parent.on_tool_call(tool_name, display_args, tool_call_id)

    def on_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Dict[str, Any],
        tool_call_id: Optional[str] = None,
    ) -> None:
        """Called when a tool execution completes.

        Forwards to parent as a nested tool result with depth information.
        Sanitizes paths before display if a path_sanitizer is configured.

        Args:
            tool_name: Name of the tool that was executed
            tool_args: Arguments that were used
            result: Result of the tool execution
            tool_call_id: Optional unique ID for this tool call (for parallel tracking)
        """
        if self._parent is None:
            return

        # Sanitize paths for display (e.g., /Users/.../file.py → [uv:id]:/workspace/file.py)
        display_args = self._sanitize_tool_args(tool_name, tool_args)

        # Generate tool_id if not provided
        tool_id = tool_call_id or f"{self._context}_{tool_name}_{id(tool_args)}"

        # Check if parent supports nested tool results
        if hasattr(self._parent, "on_nested_tool_result"):
            self._parent.on_nested_tool_result(
                tool_name,
                display_args,
                result,
                depth=self._depth,
                parent=self._context,
                tool_id=tool_id,
            )
        elif hasattr(self._parent, "on_tool_result"):
            # Fallback: use regular tool result (loses nesting info)
            self._parent.on_tool_result(tool_name, display_args, result, tool_call_id)

    # on_interrupt is automatically forwarded by ForwardingUICallback

    def on_debug(self, message: str, prefix: str = "DEBUG") -> None:
        """Called to display debug information.

        Args:
            message: The debug message
            prefix: Optional prefix for categorizing
        """
        # Forward debug with context prefix
        if self._parent and hasattr(self._parent, "on_debug"):
            self._parent.on_debug(f"[{self._context}] {message}", prefix)

    def create_nested(self, child_context: str) -> "NestedUICallback":
        """Create a further nested callback for child subagents.

        Args:
            child_context: Name/identifier of the child subagent

        Returns:
            A new NestedUICallback with incremented depth
        """
        return NestedUICallback(
            parent_callback=self._parent,
            parent_context=child_context,
            depth=self._depth + 1,
            path_sanitizer=self._path_sanitizer,  # Propagate to children
        )

    # The following methods are automatically forwarded by ForwardingUICallback:
    # - on_message()
    # - on_progress_start(), on_progress_update(), on_progress_complete()
    # - on_interrupt()
    # - on_bash_output_line()  <- This was missing before, now auto-forwarded!
    # - on_nested_tool_call(), on_nested_tool_result()
    # - on_tool_complete()


def _create_path_sanitizer(docker_context: DockerContext) -> Callable[[str], str]:
    """Create a path sanitizer for Docker mode UI display.

    Converts local paths to Docker workspace paths with container prefix:
    - /Users/.../test_opencli/src/file.py → [uv:a1b2c3d4]:/workspace/src/file.py
    - README.md → [uv:a1b2c3d4]:/workspace/README.md

    Args:
        docker_context: Docker context with workspace, image, and container info

    Returns:
        A callable that sanitizes paths for display
    """
    # Extract short image name: "ghcr.io/astral-sh/uv:python3.11-bookworm" → "uv"
    short_image = docker_context.image_name.split("/")[-1].split(":")[0]
    prefix = f"[{short_image}:{docker_context.container_id}]:"
    workspace_dir = docker_context.workspace_dir
    local_dir = docker_context.local_dir or ""

    def sanitize(path: str) -> str:
        # If path starts with local_dir, replace with workspace_dir
        if local_dir and path.startswith(local_dir):
            relative = path[len(local_dir) :].lstrip("/")
            docker_path = f"{workspace_dir}/{relative}" if relative else workspace_dir
            return f"{prefix}{docker_path}"

        # Handle Docker-internal absolute paths (e.g., /workspace/..., /testbed/...)
        if path.startswith(workspace_dir):
            return f"{prefix}{path}"

        # Fallback: extract filename from other absolute paths
        match = re.match(r"^(/Users/|/home/|/var/|/tmp/).+/([^/]+)$", path)
        if match:
            filename = match.group(2)
            return f"{prefix}{workspace_dir}/{filename}"

        # Clean relative paths that might have leading ./
        clean_path = path.lstrip("./")
        if clean_path and not clean_path.startswith("/"):
            return f"{prefix}{workspace_dir}/{clean_path}"

        return path

    return sanitize


def create_subagent_nested_callback(
    ui_callback: Any,
    subagent_name: str,
    docker_context: Optional[DockerContext] = None,
) -> Optional[NestedUICallback]:
    """Standard factory for creating subagent UI callbacks.

    This factory works for both Docker and non-Docker subagents. When docker_context
    is provided, paths in tool arguments are sanitized to show container prefixes.

    Args:
        ui_callback: Parent UI callback to wrap (can be None)
        subagent_name: Name of the subagent for context (e.g., "Code-Explorer")
        docker_context: Optional Docker context for path sanitization

    Returns:
        NestedUICallback properly configured for the subagent, or None if ui_callback is None

    Examples:
        # Non-Docker subagent (e.g., Code-Explorer)
        nested = create_subagent_nested_callback(
            ui_callback=self.ui_callback,
            subagent_name="Code-Explorer",
        )

        # Docker subagent (e.g., Web-clone)
        nested = create_subagent_nested_callback(
            ui_callback=self.ui_callback,
            subagent_name="Web-clone",
            docker_context=DockerContext(
                workspace_dir="/workspace",
                image_name="ghcr.io/astral-sh/uv:python3.11",
                container_id="a1b2c3d4",
                local_dir="/Users/.../project",
            ),
        )
    """
    if ui_callback is None:
        return None

    path_sanitizer = None
    if docker_context:
        path_sanitizer = _create_path_sanitizer(docker_context)

    return NestedUICallback(
        parent_callback=ui_callback,
        parent_context=subagent_name,
        depth=1,
        path_sanitizer=path_sanitizer,
    )
