"""Base UI callback interface and protocol definitions.

This module provides the abstract interface for UI callbacks used throughout
the application. The Protocol defines the expected methods, while BaseUICallback
provides a no-op implementation suitable for testing or when no UI is attached.
"""

from __future__ import annotations

from typing import Any, Dict, Optional, Protocol, runtime_checkable


@runtime_checkable
class UICallbackProtocol(Protocol):
    """Protocol defining the UI callback interface.

    Implementations should provide methods to handle various UI events
    such as thinking indicators, tool calls, progress updates, etc.
    """

    def on_thinking_start(self) -> None:
        """Called when the agent starts thinking."""
        ...

    def on_thinking_complete(self) -> None:
        """Called when the agent completes thinking."""
        ...

    def on_assistant_message(self, content: str) -> None:
        """Called when assistant provides a message."""
        ...

    def on_message(self, message: str) -> None:
        """Called to display a general message."""
        ...

    def on_progress_start(self, message: str) -> None:
        """Called when a progress indicator should start."""
        ...

    def on_progress_update(self, message: str) -> None:
        """Called to update the progress message."""
        ...

    def on_progress_complete(self, message: str = "", success: bool = True) -> None:
        """Called when a progress indicator should stop."""
        ...

    def on_interrupt(self) -> None:
        """Called when execution is interrupted."""
        ...

    def on_bash_output_line(self, line: str, is_stderr: bool = False) -> None:
        """Called for each line of bash output."""
        ...

    def on_tool_call(self, tool_name: str, tool_args: Dict[str, Any]) -> None:
        """Called when a tool call is about to be executed."""
        ...

    def on_tool_result(
        self, tool_name: str, tool_args: Dict[str, Any], result: Any
    ) -> None:
        """Called when a tool execution completes."""
        ...

    def on_nested_tool_call(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        depth: int = 1,
        parent: str = "",
    ) -> None:
        """Called when a nested/subagent tool call starts."""
        ...

    def on_nested_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Any,
        depth: int = 1,
        parent: str = "",
    ) -> None:
        """Called when a nested/subagent tool call completes."""
        ...

    def on_debug(self, message: str, prefix: str = "DEBUG") -> None:
        """Called to display debug information."""
        ...

    def on_tool_complete(
        self,
        tool_name: str,
        success: bool,
        message: str,
        details: Optional[str] = None,
    ) -> None:
        """Called when ANY tool completes to display result.

        REQUIRED: Every tool must show a result line using this method.

        Args:
            tool_name: Name of the tool that completed
            success: Whether the tool succeeded
            message: Result message to display
            details: Optional additional details (shown dimmed)
        """
        ...

    def on_single_agent_start(
        self, agent_type: str, description: str, tool_call_id: str
    ) -> None:
        """Called when a single subagent starts executing."""
        ...

    def on_single_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Called when a single subagent completes."""
        ...

    def on_parallel_agents_start(self, agent_infos: list) -> None:
        """Called when parallel agents start executing."""
        ...

    def on_parallel_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Called when a parallel agent completes."""
        ...

    def on_parallel_agents_done(self) -> None:
        """Called when all parallel agents have completed."""
        ...

    def on_context_usage(self, usage_pct: float) -> None:
        """Called to update context window usage percentage (0-100)."""
        ...


class BaseUICallback:
    """Base implementation of UI callback with no-op methods.

    This class provides empty implementations of all callback methods,
    making it suitable for:
    - Testing scenarios where no UI feedback is needed
    - Fallback when the real UI is not available
    - Base class for partial implementations

    See also ForwardingUICallback for a base class that forwards to a parent.
    """

    def on_thinking_start(self) -> None:
        """Called when the agent starts thinking."""
        pass

    def on_thinking_complete(self) -> None:
        """Called when the agent completes thinking."""
        pass

    def on_thinking(self, content: str) -> None:
        """Called when the model produces thinking content."""
        pass

    def on_assistant_message(self, content: str) -> None:
        """Called when assistant provides a message."""
        pass

    def on_message(self, message: str) -> None:
        """Called to display a general message."""
        pass

    def on_progress_start(self, message: str) -> None:
        """Called when a progress indicator should start."""
        pass

    def on_progress_update(self, message: str) -> None:
        """Called to update the progress message."""
        pass

    def on_progress_complete(self, message: str = "", success: bool = True) -> None:
        """Called when a progress indicator should stop."""
        pass

    def on_interrupt(self) -> None:
        """Called when execution is interrupted."""
        pass

    def on_bash_output_line(self, line: str, is_stderr: bool = False) -> None:
        """Called for each line of bash output."""
        pass

    def on_tool_call(self, tool_name: str, tool_args: Dict[str, Any]) -> None:
        """Called when a tool call is about to be executed."""
        pass

    def on_tool_result(
        self, tool_name: str, tool_args: Dict[str, Any], result: Any
    ) -> None:
        """Called when a tool execution completes."""
        pass

    def on_nested_tool_call(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        depth: int = 1,
        parent: str = "",
    ) -> None:
        """Called when a nested/subagent tool call starts."""
        pass

    def on_nested_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Any,
        depth: int = 1,
        parent: str = "",
    ) -> None:
        """Called when a nested/subagent tool call completes."""
        pass

    def on_debug(self, message: str, prefix: str = "DEBUG") -> None:
        """Called to display debug information."""
        pass

    def on_tool_complete(
        self,
        tool_name: str,
        success: bool,
        message: str,
        details: Optional[str] = None,
    ) -> None:
        """Called when ANY tool completes to display result."""
        pass

    def on_single_agent_start(
        self, agent_type: str, description: str, tool_call_id: str
    ) -> None:
        """Called when a single subagent starts executing."""
        pass

    def on_single_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Called when a single subagent completes."""
        pass

    def on_parallel_agents_start(self, agent_infos: list) -> None:
        """Called when parallel agents start executing."""
        pass

    def on_parallel_agent_complete(self, tool_call_id: str, success: bool) -> None:
        """Called when a parallel agent completes."""
        pass

    def on_parallel_agents_done(self) -> None:
        """Called when all parallel agents have completed."""
        pass

    def on_context_usage(self, usage_pct: float) -> None:
        """Called to update context window usage percentage (0-100)."""
        pass


class ForwardingUICallback(BaseUICallback):
    """Base class that forwards all callback methods to a parent.

    Extend this class when you need a wrapper that:
    - Forwards most callbacks to a parent unchanged
    - Only overrides specific methods for custom behavior

    This ensures new protocol methods are automatically forwarded,
    avoiding silent failures when the protocol is extended.

    Example:
        class MyWrapper(ForwardingUICallback):
            def on_thinking_start(self) -> None:
                pass  # Don't forward this one

            # All other methods automatically forward to parent
    """

    def __init__(self, parent: Any) -> None:
        """Initialize with parent callback to forward to.

        Args:
            parent: The parent callback to forward events to
        """
        self._parent = parent

    def _forward(self, method_name: str, *args: Any, **kwargs: Any) -> Any:
        """Forward a method call to parent if it has the method.

        Args:
            method_name: Name of the method to call on parent
            *args: Positional arguments to pass
            **kwargs: Keyword arguments to pass

        Returns:
            Result from parent method, or None if parent doesn't have the method
        """
        if self._parent and hasattr(self._parent, method_name):
            return getattr(self._parent, method_name)(*args, **kwargs)
        return None

    def on_thinking_start(self) -> None:
        self._forward('on_thinking_start')

    def on_thinking_complete(self) -> None:
        self._forward('on_thinking_complete')

    def on_thinking(self, content: str) -> None:
        self._forward('on_thinking', content)

    def on_assistant_message(self, content: str) -> None:
        self._forward('on_assistant_message', content)

    def on_message(self, message: str) -> None:
        self._forward('on_message', message)

    def on_progress_start(self, message: str) -> None:
        self._forward('on_progress_start', message)

    def on_progress_update(self, message: str) -> None:
        self._forward('on_progress_update', message)

    def on_progress_complete(self, message: str = "", success: bool = True) -> None:
        self._forward('on_progress_complete', message, success=success)

    def on_interrupt(self) -> None:
        self._forward('on_interrupt')

    def on_bash_output_line(self, line: str, is_stderr: bool = False) -> None:
        self._forward('on_bash_output_line', line, is_stderr=is_stderr)

    def on_tool_call(self, tool_name: str, tool_args: Dict[str, Any]) -> None:
        self._forward('on_tool_call', tool_name, tool_args)

    def on_tool_result(
        self, tool_name: str, tool_args: Dict[str, Any], result: Any
    ) -> None:
        self._forward('on_tool_result', tool_name, tool_args, result)

    def on_nested_tool_call(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        depth: int = 1,
        parent: str = "",
    ) -> None:
        self._forward('on_nested_tool_call', tool_name, tool_args, depth=depth, parent=parent)

    def on_nested_tool_result(
        self,
        tool_name: str,
        tool_args: Dict[str, Any],
        result: Any,
        depth: int = 1,
        parent: str = "",
    ) -> None:
        self._forward('on_nested_tool_result', tool_name, tool_args, result, depth=depth, parent=parent)

    def on_debug(self, message: str, prefix: str = "DEBUG") -> None:
        self._forward('on_debug', message, prefix=prefix)

    def on_tool_complete(
        self,
        tool_name: str,
        success: bool,
        message: str,
        details: Optional[str] = None,
    ) -> None:
        self._forward('on_tool_complete', tool_name, success, message, details=details)

    def on_single_agent_start(
        self, agent_type: str, description: str, tool_call_id: str
    ) -> None:
        self._forward('on_single_agent_start', agent_type, description, tool_call_id)

    def on_single_agent_complete(self, tool_call_id: str, success: bool) -> None:
        self._forward('on_single_agent_complete', tool_call_id, success)

    def on_parallel_agents_start(self, agent_infos: list) -> None:
        self._forward('on_parallel_agents_start', agent_infos)

    def on_parallel_agent_complete(self, tool_call_id: str, success: bool) -> None:
        self._forward('on_parallel_agent_complete', tool_call_id, success)

    def on_parallel_agents_done(self) -> None:
        self._forward('on_parallel_agents_done')

    def on_context_usage(self, usage_pct: float) -> None:
        self._forward('on_context_usage', usage_pct)


__all__ = ["UICallbackProtocol", "BaseUICallback", "ForwardingUICallback"]
