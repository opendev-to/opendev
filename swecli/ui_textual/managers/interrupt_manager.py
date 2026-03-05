"""Centralized interrupt/escape handling for the Textual UI.

This module provides a unified InterruptManager that:
1. Tracks what is currently active (prompt, tool, panel, thinking)
2. Provides consistent cancel behavior based on active state
3. Ensures proper cleanup (spinners, UI state)
"""

from __future__ import annotations

import threading
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import TYPE_CHECKING, Any, List, Optional

if TYPE_CHECKING:
    from textual.app import App


class InterruptState(Enum):
    """States that affect how ESC/interrupt is handled."""

    IDLE = auto()
    EXIT_CONFIRMATION = auto()
    APPROVAL_PROMPT = auto()
    ASK_USER_PROMPT = auto()
    PLAN_APPROVAL = auto()
    MODEL_PICKER = auto()
    AGENT_WIZARD = auto()
    SKILL_WIZARD = auto()
    AUTOCOMPLETE = auto()
    PROCESSING_THINKING = auto()
    PROCESSING_TOOL = auto()
    PROCESSING_PARALLEL_TOOLS = auto()


# States that represent active modal controllers
_CONTROLLER_STATES = {
    InterruptState.APPROVAL_PROMPT,
    InterruptState.ASK_USER_PROMPT,
    InterruptState.PLAN_APPROVAL,
    InterruptState.MODEL_PICKER,
    InterruptState.AGENT_WIZARD,
    InterruptState.SKILL_WIZARD,
}


@dataclass
class InterruptContext:
    """Context information for the current interrupt state."""

    state: InterruptState
    tool_name: Optional[str] = None
    tool_names: List[str] = field(default_factory=list)
    spinner_ids: List[str] = field(default_factory=list)
    controller_ref: Optional[Any] = None


class InterruptManager:
    """Centralized manager for interrupt/escape key handling.

    This manager tracks the current UI state and ensures ESC key
    presses are handled consistently across different contexts:

    - Autocomplete visible: dismiss autocomplete only
    - Modal/wizard active: cancel the modal
    - Processing: interrupt the running operation
    - Exit confirmation: clear confirmation mode

    Thread Safety:
    - All public methods are thread-safe via RLock
    - State stack supports nested contexts (e.g., approval during subagent)
    """

    def __init__(self, app: "App") -> None:
        """Initialize the InterruptManager.

        Args:
            app: The Textual App instance
        """
        self.app = app
        self._lock = threading.RLock()
        self._current_state = InterruptState.IDLE
        self._context: Optional[InterruptContext] = None
        self._state_stack: List[InterruptContext] = []
        self._controller_registry: List[Any] = []
        self._active_interrupt_token: Optional[Any] = None

    @property
    def current_state(self) -> InterruptState:
        """Get the current interrupt state."""
        with self._lock:
            return self._current_state

    @property
    def context(self) -> Optional[InterruptContext]:
        """Get the current context."""
        with self._lock:
            return self._context

    def enter_state(
        self,
        state: InterruptState,
        tool_name: Optional[str] = None,
        tool_names: Optional[List[str]] = None,
        spinner_ids: Optional[List[str]] = None,
        controller_ref: Optional[Any] = None,
    ) -> None:
        """Push current state and enter new state.

        Args:
            state: The new state to enter
            tool_name: Optional single tool name (for PROCESSING_TOOL)
            tool_names: Optional list of tool names (for PROCESSING_PARALLEL_TOOLS)
            spinner_ids: Optional list of active spinner IDs
            controller_ref: Optional reference to the active controller
        """
        with self._lock:
            # Push current context to stack if we have one
            if self._context is not None:
                self._state_stack.append(self._context)

            self._current_state = state
            self._context = InterruptContext(
                state=state,
                tool_name=tool_name,
                tool_names=tool_names or [],
                spinner_ids=spinner_ids or [],
                controller_ref=controller_ref,
            )

    def exit_state(self) -> None:
        """Pop and restore previous state."""
        with self._lock:
            if self._state_stack:
                self._context = self._state_stack.pop()
                self._current_state = self._context.state
            else:
                self._current_state = InterruptState.IDLE
                self._context = None

    def is_in_state(self, *states: InterruptState) -> bool:
        """Check if currently in any of the given states.

        Args:
            *states: States to check against

        Returns:
            True if current state matches any of the given states
        """
        with self._lock:
            return self._current_state in states

    def handle_interrupt(self) -> bool:
        """Handle ESC key press based on current state.

        Returns:
            True if the interrupt was consumed (no further handling needed),
            False if the caller should handle the interrupt
        """
        # First, check for autocomplete (highest priority)
        if self._has_autocomplete():
            return self._dismiss_autocomplete()

        # Check for active controllers (state-tracked or direct query)
        if self._cancel_active_controller():
            return True

        with self._lock:
            state = self._current_state

            # Check for exit confirmation mode
            if state == InterruptState.EXIT_CONFIRMATION:
                return self._clear_exit_confirmation()

            # Processing states - handled by caller (action_interrupt)
            if state in (
                InterruptState.PROCESSING_THINKING,
                InterruptState.PROCESSING_TOOL,
                InterruptState.PROCESSING_PARALLEL_TOOLS,
            ):
                # Cleanup spinners but let caller handle the actual interrupt
                self.cleanup_spinners()
                return False

            # IDLE state - nothing to handle
            return False

    def handle_cancel(self) -> bool:
        """Handle Ctrl+C based on current state.

        This is similar to handle_interrupt but may have different
        behavior for some states.

        Returns:
            True if the cancel was consumed, False otherwise
        """
        return self.handle_interrupt()

    def cleanup_spinners(self) -> None:
        """Stop all active spinners tracked in current context."""
        spinner_ids = []
        with self._lock:
            if self._context:
                spinner_ids = list(self._context.spinner_ids)

        # Stop spinners outside lock to avoid deadlock
        if hasattr(self.app, "spinner_service"):
            spinner_service = self.app.spinner_service
            for spinner_id in spinner_ids:
                if spinner_service.is_active(spinner_id):
                    spinner_service.stop(spinner_id, success=False)

    def stop_all_spinners(self, success: bool = False) -> None:
        """Stop all active spinners via SpinnerService.

        Args:
            success: Whether to mark spinners as successful
        """
        if hasattr(self.app, "spinner_service"):
            self.app.spinner_service.stop_all(immediate=True)

    def add_spinner_id(self, spinner_id: str) -> None:
        """Track a spinner ID in the current context.

        Args:
            spinner_id: The spinner ID to track
        """
        with self._lock:
            if self._context:
                self._context.spinner_ids.append(spinner_id)

    def remove_spinner_id(self, spinner_id: str) -> None:
        """Remove a spinner ID from tracking.

        Args:
            spinner_id: The spinner ID to remove
        """
        with self._lock:
            if self._context and spinner_id in self._context.spinner_ids:
                self._context.spinner_ids.remove(spinner_id)

    # -------------------------------------------------------------------------
    # Controller registry
    # -------------------------------------------------------------------------

    def register_controller(self, controller: Any) -> None:
        """Register a modal controller for ESC cancellation.

        Controllers must have:
        - active: bool property
        - cancel() method
        """
        with self._lock:
            if controller not in self._controller_registry:
                self._controller_registry.append(controller)

    def unregister_controller(self, controller: Any) -> None:
        """Unregister a modal controller."""
        with self._lock:
            if controller in self._controller_registry:
                self._controller_registry.remove(controller)

    # -------------------------------------------------------------------------
    # Interrupt token management
    # -------------------------------------------------------------------------

    def set_interrupt_token(self, token: Any) -> None:
        """Set the active interrupt token for the current run."""
        with self._lock:
            self._active_interrupt_token = token

    def clear_interrupt_token(self) -> None:
        """Clear the active interrupt token."""
        with self._lock:
            self._active_interrupt_token = None

    def request_run_interrupt(self) -> bool:
        """Request interrupt of the active run via the interrupt token.

        Returns:
            True if an active token was found and signaled.
        """
        with self._lock:
            token = self._active_interrupt_token
        if token is not None:
            token.request()
            return True
        return False

    # -------------------------------------------------------------------------
    # Internal handlers
    # -------------------------------------------------------------------------

    def _has_autocomplete(self) -> bool:
        """Check if autocomplete is currently visible."""
        if hasattr(self.app, "input_field"):
            input_field = self.app.input_field
            completions = getattr(input_field, "_completions", None)
            return bool(completions)
        return False

    def _dismiss_autocomplete(self) -> bool:
        """Dismiss autocomplete popup.

        Returns:
            True if autocomplete was dismissed
        """
        if hasattr(self.app, "input_field"):
            input_field = self.app.input_field
            if hasattr(input_field, "_dismiss_autocomplete"):
                input_field._dismiss_autocomplete()
                return True
        return False

    def _cancel_active_controller(self) -> bool:
        """Cancel any active modal controller.

        Priority order:
        1. Registered controllers (via register_controller)
        2. State-tracked controller_ref (registered via enter_state)
        3. Hardcoded fallback list (deprecated, for backward compat during migration)

        Returns:
            True if a controller was cancelled
        """
        # Priority 1: Check registered controllers
        with self._lock:
            registry = list(self._controller_registry)
        for controller in registry:
            if getattr(controller, "active", False) and hasattr(controller, "cancel"):
                controller.cancel()
                return True

        # Priority 2: Try using the state-tracked controller_ref
        with self._lock:
            context = self._context
            state = self._current_state

        if context and state in _CONTROLLER_STATES:
            controller = context.controller_ref
            if controller and hasattr(controller, "cancel"):
                controller.cancel()
                return True

        # Priority 3 (DEPRECATED): query controllers directly on the app.
        # This hardcoded fallback is kept for backward compatibility during
        # migration to the registry pattern. Remove once all controllers
        # use register_controller().
        _controllers = [
            ("_approval_controller", "_approval_cancel"),
            ("_ask_user_controller", "_ask_user_cancel"),
            ("_plan_approval_controller", "_plan_approval_cancel"),
            ("_model_picker", "_model_picker_cancel"),
            ("_agent_creator", "_agent_wizard_cancel"),
            ("_skill_creator", "_skill_wizard_cancel"),
        ]
        for attr_name, cancel_method in _controllers:
            controller = getattr(self.app, attr_name, None)
            if controller and getattr(controller, "active", False):
                method = getattr(self.app, cancel_method, None)
                if method:
                    method()
                    return True

        return False

    def _clear_exit_confirmation(self) -> bool:
        """Clear exit confirmation mode.

        Returns:
            True if exit confirmation was cleared
        """
        if hasattr(self.app, "_cancel_exit_confirmation"):
            self.app._cancel_exit_confirmation()
            return True
        return False


__all__ = [
    "InterruptManager",
    "InterruptState",
    "InterruptContext",
]
