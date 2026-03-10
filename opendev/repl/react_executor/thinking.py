"""Thinking trace, critique, and refinement methods for ReactExecutor."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


def _debug_log(message: str) -> None:
    """Write debug message to /tmp/swecli_react_debug.log."""
    from datetime import datetime

    log_file = "/tmp/swecli_react_debug.log"
    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
    with open(log_file, "a") as f:
        f.write(f"[{timestamp}] {message}\n")


class ThinkingMixin:
    """Mixin providing thinking trace, critique, and refinement methods.

    Expects the host class to provide:
        - self._active_interrupt_token
        - self._current_task_monitor
        - self._last_thinking_error
    """

    def _get_thinking_trace(
        self,
        messages: list,
        agent,
        ui_callback=None,
        tool_registry=None,
    ) -> Optional[str]:
        """Make a SEPARATE LLM call to get thinking trace.

        Uses the full conversation history with a swapped thinking system prompt.
        Both this and the main action phase operate on the same compacted messages.

        Args:
            messages: Current conversation messages (already compacted)
            agent: The agent to use for the thinking call
            ui_callback: Optional UI callback for displaying thinking

        Returns:
            Thinking trace string, or None on failure
        """
        from opendev.core.runtime.monitoring import TaskMonitor
        from opendev.core.agents.prompts import get_reminder

        try:
            # Build thinking-specific system prompt
            thinking_system_prompt = agent.build_system_prompt(thinking_visible=True)

            # Clone messages with swapped system prompt (exclude nudges)
            thinking_messages = self._build_messages_with_system_prompt(
                messages, thinking_system_prompt, exclude_nudges=True
            )

            # Build analysis prompt — todo-aware when todos exist
            todo_handler = getattr(tool_registry, "todo_handler", None) if tool_registry else None
            if todo_handler and todo_handler.has_todos():
                todos = list(todo_handler._todos.values())
                done = sum(1 for t in todos if t.status == "done")
                total = len(todos)
                status_lines = []
                for t in todos:
                    symbol = {"done": "done", "doing": "doing"}.get(t.status, "todo")
                    status_lines.append(f"  [{symbol}] {t.title}")
                analysis_content = get_reminder(
                    "thinking_analysis_prompt_with_todos",
                    done_count=str(done),
                    total_count=str(total),
                    todo_status="\n".join(status_lines),
                )
            else:
                analysis_content = get_reminder("thinking_analysis_prompt")

            # Append analysis prompt as final user message
            thinking_messages.append(
                {
                    "role": "user",
                    "content": analysis_content,
                },
            )

            # Call LLM WITHOUT tools - just get reasoning
            task_monitor = TaskMonitor()
            if self._active_interrupt_token:
                task_monitor.set_interrupt_token(self._active_interrupt_token)
            # Track task monitor for interrupt support
            self._current_task_monitor = task_monitor
            try:
                from opendev.ui_textual.debug_logger import debug_log
            except ImportError:
                debug_log = lambda *a, **kw: None  # noqa: E731

            debug_log("ReactExecutor", f"Thinking phase: SET _current_task_monitor={task_monitor}")
            try:
                response = agent.call_thinking_llm(thinking_messages, task_monitor)

                if response.get("success"):
                    thinking_trace = response.get("content", "")

                    # Display in UI
                    if thinking_trace and ui_callback and hasattr(ui_callback, "on_thinking"):
                        ui_callback.on_thinking(thinking_trace)

                    return thinking_trace
                else:
                    # Log the error for debugging
                    error = response.get("error", "Unknown error")
                    if ui_callback and hasattr(ui_callback, "on_debug"):
                        ui_callback.on_debug(f"Thinking phase error: {error}", "THINK")
                    # Store full response for interrupt checking (reused by _handle_llm_error)
                    self._last_thinking_error = response
            finally:
                # Clear task monitor after thinking phase
                self._current_task_monitor = None
                debug_log("ReactExecutor", "Thinking phase: CLEARED _current_task_monitor")

        except Exception as e:
            if isinstance(e, InterruptedError):
                raise
            # Log exceptions for debugging
            if ui_callback and hasattr(ui_callback, "on_debug"):
                ui_callback.on_debug(f"Thinking phase exception: {str(e)}", "THINK")
            import logging

            logging.getLogger(__name__).exception("Error in thinking phase")

        return None

    def _critique_and_refine_thinking(
        self,
        thinking_trace: str,
        messages: list,
        agent,
        ui_callback=None,
    ) -> str:
        """Critique thinking trace and optionally refine it.

        When High thinking level is active, this method:
        1. Calls the critique LLM to analyze the thinking trace
        2. Uses the critique to generate a refined thinking trace

        Args:
            thinking_trace: The original thinking trace to critique
            messages: Current conversation messages (for context in refinement)
            agent: The agent to use for critique/refinement calls
            ui_callback: Optional UI callback for displaying critique

        Returns:
            Refined thinking trace (or original if critique fails)
        """
        from opendev.core.runtime.monitoring import TaskMonitor

        try:
            # Step 1: Get critique of the thinking trace
            task_monitor = TaskMonitor()
            if self._active_interrupt_token:
                task_monitor.set_interrupt_token(self._active_interrupt_token)
            self._current_task_monitor = task_monitor

            try:
                critique_response = agent.call_critique_llm(thinking_trace, task_monitor)

                if not critique_response.get("success"):
                    error = critique_response.get("error", "Unknown error")
                    if ui_callback and hasattr(ui_callback, "on_debug"):
                        ui_callback.on_debug(f"Critique phase error: {error}", "CRITIQUE")
                    return thinking_trace  # Return original on failure

                critique = critique_response.get("content", "")

                if not critique or not critique.strip():
                    return thinking_trace  # No critique generated

                # Display critique in UI if callback available
                if ui_callback and hasattr(ui_callback, "on_critique"):
                    ui_callback.on_critique(critique)

                # Step 2: Refine thinking trace using the critique
                refined_trace = self._refine_thinking_with_critique(
                    thinking_trace, critique, messages, agent, ui_callback
                )

                return refined_trace if refined_trace else thinking_trace

            finally:
                self._current_task_monitor = None

        except Exception as e:
            if isinstance(e, InterruptedError):
                raise
            if ui_callback and hasattr(ui_callback, "on_debug"):
                ui_callback.on_debug(f"Critique phase exception: {str(e)}", "CRITIQUE")
            import logging

            logging.getLogger(__name__).exception("Error in critique phase")
            return thinking_trace  # Return original on exception

    def _refine_thinking_with_critique(
        self,
        thinking_trace: str,
        critique: str,
        messages: list,
        agent,
        ui_callback=None,
    ) -> Optional[str]:
        """Generate a refined thinking trace incorporating critique feedback.

        Args:
            thinking_trace: Original thinking trace
            critique: Critique feedback
            messages: Current conversation messages (already compacted)
            agent: Agent for LLM call
            ui_callback: Optional UI callback

        Returns:
            Refined thinking trace, or None on failure
        """
        from opendev.core.runtime.monitoring import TaskMonitor

        try:
            # Build refinement system prompt
            refinement_system = agent.build_system_prompt(thinking_visible=True)

            # Clone messages with swapped system prompt (exclude nudges)
            refinement_messages = self._build_messages_with_system_prompt(
                messages, refinement_system, exclude_nudges=True
            )

            # Append refinement user message with trace + critique
            refinement_messages.append(
                {
                    "role": "user",
                    "content": f"""Your previous reasoning was:

{thinking_trace}

A critique identified these issues:

{critique}

Please provide refined reasoning that addresses these concerns. Keep it concise (under 100 words).""",
                },
            )

            task_monitor = TaskMonitor()
            if self._active_interrupt_token:
                task_monitor.set_interrupt_token(self._active_interrupt_token)
            self._current_task_monitor = task_monitor

            try:
                response = agent.call_thinking_llm(refinement_messages, task_monitor)

                if response.get("success"):
                    refined = response.get("content", "")
                    if refined and refined.strip():
                        # Display refined thinking in UI
                        if ui_callback and hasattr(ui_callback, "on_thinking"):
                            ui_callback.on_thinking(refined)
                        return refined
            finally:
                self._current_task_monitor = None

        except Exception as e:
            if isinstance(e, InterruptedError):
                raise
            if ui_callback and hasattr(ui_callback, "on_debug"):
                ui_callback.on_debug(f"Refinement error: {str(e)}", "CRITIQUE")

        return None
