"""Single iteration logic, LLM error handling, response parsing, and nudging for ReactExecutor."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    pass

from opendev.models.message import ChatMessage, Role
from opendev.core.context_engineering.memory import AgentResponse
from opendev.core.agents.prompts import get_reminder
from opendev.core.agents.prompts.reminders import append_nudge

logger = logging.getLogger(__name__)

_ctx_logger = logging.getLogger("swecli.context_debug")


def _debug_log(message: str) -> None:
    """Write debug message to /tmp/swecli_react_debug.log."""
    from datetime import datetime

    log_file = "/tmp/swecli_react_debug.log"
    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
    with open(log_file, "a") as f:
        f.write(f"[{timestamp}] {message}\n")


def _session_debug():
    """Get the current session debug logger."""
    from opendev.core.debug import get_debug_logger

    return get_debug_logger()


class IterationMixin:
    """Mixin providing single iteration logic, LLM error handling, response parsing, and nudging.

    Expects the host class to provide:
        - self._active_interrupt_token
        - self._current_task_monitor
        - self._current_thinking_trace
        - self._current_reasoning_content
        - self._current_token_usage
        - self._last_thinking_error
        - self._last_latency_ms
        - self._last_error
        - self._last_operation_summary
        - self._llm_caller
        - self._tool_executor
        - self._cost_tracker
        - self._compactor
        - self._injection_queue
        - self.session_manager
        - self.config
        - self.console
        - self.MAX_NUDGE_ATTEMPTS
        - self.MAX_TODO_NUDGES
    """

    def _check_subagent_completion(self, messages: list) -> bool:
        """Check if the last tool result was from a completed subagent.

        Returns True if the last tool result indicates subagent completion.
        Used to skip thinking phase and inject continuation signal.
        """
        for msg in reversed(messages):
            if msg.get("role") == "tool":
                content = msg.get("content", "")
                is_subagent_complete = (
                    "[completion_status=success]" in content
                    or "[SYNC COMPLETE]" in content
                    or "[completion_status=failed]" in content
                    or content.startswith("Error: [")
                )
                _debug_log(f"[SUBAGENT_CHECK] is_subagent={is_subagent_complete}")
                return is_subagent_complete
            # Stop searching if we hit a user message (new turn)
            if msg.get("role") == "user" and "<thinking_trace>" not in msg.get("content", ""):
                return False
        return False

    def _run_iteration(self, ctx) -> "LoopAction":
        """Run a single ReAct iteration."""
        from opendev.repl.react_executor.executor import LoopAction

        try:
            return self._run_iteration_inner(ctx)
        except InterruptedError:
            _debug_log("[INTERRUPT] Caught InterruptedError in _run_iteration")
            if ctx.ui_callback and hasattr(ctx.ui_callback, "on_interrupt"):
                ctx.ui_callback.on_interrupt()
            return LoopAction.BREAK

    def _call_action_llm(self, agent, messages, task_monitor, thinking_visible):
        """Call LLM for action phase. Uses llm_caller if available (TUI spinner), otherwise direct.

        Returns:
            Tuple of (response_dict, latency_ms)
        """
        if self._llm_caller:
            return self._llm_caller.call_llm_with_progress(
                agent, messages, task_monitor, thinking_visible=thinking_visible
            )
        import time

        start = time.monotonic()
        response = agent.call_llm(messages, task_monitor=task_monitor, thinking_visible=thinking_visible)
        latency = int((time.monotonic() - start) * 1000)
        return response, latency

    def _run_iteration_inner(self, ctx) -> "LoopAction":
        """Inner implementation of _run_iteration (wrapped by interrupt handler)."""
        from opendev.core.runtime.monitoring import TaskMonitor

        # Notify UI that a new iteration is starting (thinking phase)
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_thinking_start"):
            ctx.ui_callback.on_thinking_start()

        # Debug logging
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_debug"):
            ctx.ui_callback.on_debug(f"Calling LLM with {len(ctx.messages)} messages", "LLM")

        # Get thinking visibility from tool registry
        thinking_visible = False
        if ctx.tool_registry and hasattr(ctx.tool_registry, "thinking_handler"):
            thinking_visible = ctx.tool_registry.thinking_handler.is_visible

        # Check if last tool was subagent completion
        subagent_just_completed = self._check_subagent_completion(ctx.messages)

        # Log decision point to file
        _debug_log(
            f"[ITERATION] thinking_visible={thinking_visible}, "
            f"subagent_completed={subagent_just_completed}, "
            f"msg_count={len(ctx.messages)}"
        )

        # AUTO-COMPACTION: Compact messages if approaching context limit
        # Must happen BEFORE thinking phase so both thinking and action phases
        # operate on the same compacted message base.
        self._maybe_compact(ctx)

        # Phase boundary: catch ESC pressed during compaction or between iterations
        self._check_interrupt("pre-thinking")

        # THINKING PHASE: Get thinking trace BEFORE action (when thinking mode is ON)
        # Skip thinking phase after subagent completion - main agent decides directly
        # Skip thinking if previous iteration set skip_next_thinking (e.g., explore-first block)
        should_skip_thinking = ctx.skip_next_thinking
        ctx.skip_next_thinking = False

        if thinking_visible and not subagent_just_completed and not should_skip_thinking:
            thinking_trace = self._get_thinking_trace(
                ctx.messages, ctx.agent, ctx.ui_callback, tool_registry=ctx.tool_registry
            )

            # Check for interrupt from thinking phase (reuse existing _handle_llm_error)
            if self._last_thinking_error is not None:
                error_response = self._last_thinking_error
                self._last_thinking_error = None  # Clear the stored error
                error_text = error_response.get("error", "")
                if "interrupted" in error_text.lower():
                    # Use existing error handler - it calls on_interrupt() and returns BREAK
                    return self._handle_llm_error(error_response, ctx)

            # Phase boundary: catch ESC pressed during thinking when LLM returned before
            # the HTTP client detected the interrupt (race condition — Scenario 2)
            self._check_interrupt("post-thinking")

            # SELF-CRITIQUE PHASE: Critique and refine thinking trace (when level is High)
            includes_critique = False
            if ctx.tool_registry and hasattr(ctx.tool_registry, "thinking_handler"):
                includes_critique = ctx.tool_registry.thinking_handler.includes_critique

            if includes_critique and thinking_trace:
                thinking_trace = self._critique_and_refine_thinking(
                    thinking_trace, ctx.messages, ctx.agent, ctx.ui_callback
                )

            self._current_thinking_trace = thinking_trace  # Track for persistence
            if thinking_trace:
                # Inject trace as user message for the action phase
                ctx.messages.append(
                    {
                        "role": "user",
                        "content": get_reminder(
                            "thinking_trace_reminder", thinking_trace=thinking_trace
                        ),
                    }
                )

        # CONTINUATION SIGNAL: After subagent completion, nudge agent to keep working
        # Skip if continue_after_subagent is True (e.g., caller handles post-subagent flow)
        if subagent_just_completed and not ctx.continue_after_subagent:
            _debug_log("[ITERATION] Injecting stop signal after subagent completion")
            ctx.messages.append(
                {
                    "role": "user",
                    "content": get_reminder("subagent_complete_signal"),
                }
            )

        # Drain any injected messages before action phase (EC4 — arrived during thinking)
        self._drain_injected_messages(ctx)

        # Phase boundary: catch ESC pressed during critique or late-arriving signals
        self._check_interrupt("pre-action")

        # Message pair integrity is enforced at write time by ValidatedMessageList.
        # No need for repair() here — invariants are maintained on every append.

        # ACTION PHASE: Call LLM with tools (no force_think)
        task_monitor = TaskMonitor()
        if self._active_interrupt_token:
            task_monitor.set_interrupt_token(self._active_interrupt_token)
        try:
            from opendev.ui_textual.debug_logger import debug_log
        except ImportError:
            debug_log = lambda *a, **kw: None  # noqa: E731

        debug_log(
            "ReactExecutor",
            f"Calling _call_action_llm, _llm_caller={id(self._llm_caller) if self._llm_caller else None}, task_monitor={task_monitor}",
        )
        _session_debug().log(
            "llm_call_start",
            "llm",
            model=getattr(ctx.agent, "model", "unknown"),
            message_count=len(ctx.messages),
            thinking_visible=thinking_visible,
        )
        response, latency_ms = self._call_action_llm(
            ctx.agent, ctx.messages, task_monitor, thinking_visible=thinking_visible
        )
        debug_log(
            "ReactExecutor", f"_call_action_llm returned, success={response.get('success')}"
        )
        self._last_latency_ms = latency_ms
        _session_debug().log(
            "llm_call_end",
            "llm",
            duration_ms=latency_ms,
            success=response.get("success", False),
            tokens=response.get("usage"),
            has_tool_calls=bool(
                response.get("tool_calls") or (response.get("message") or {}).get("tool_calls")
            ),
            content_preview=(response.get("content") or "")[:200],
        )

        # Debug logging
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_debug"):
            success = response.get("success", False)
            ctx.ui_callback.on_debug(
                f"LLM response (success={success}, latency={latency_ms}ms)", "LLM"
            )

        # Handle errors
        if not response["success"]:
            return self._handle_llm_error(response, ctx)

        # Parse response - now includes reasoning_content
        content, tool_calls, reasoning_content = self._parse_llm_response(response)
        self._current_reasoning_content = reasoning_content  # Track for persistence
        self._current_token_usage = response.get("usage")  # Track token usage

        # Cost tracking
        usage = response.get("usage")
        if usage and self._cost_tracker:
            model_info = self.config.get_model_info()
            self._cost_tracker.record_usage(usage, model_info)
            # Notify UI
            if ctx.ui_callback and hasattr(ctx.ui_callback, "on_cost_update"):
                ctx.ui_callback.on_cost_update(self._cost_tracker.total_cost_usd)
            # Persist in session metadata
            session = self.session_manager.get_current_session()
            if session is not None:
                session.metadata["cost_tracking"] = self._cost_tracker.to_metadata()

        # Calibrate compactor with real API token count
        if usage and self._compactor:
            prompt_tokens = usage.get("prompt_tokens", 0)
            _ctx_logger.info(
                "api_usage_received: prompt_tok=%d total_tok=%d completion_tok=%d",
                prompt_tokens,
                usage.get("total_tokens", 0),
                usage.get("completion_tokens", 0),
            )
            _session_debug().log(
                "api_usage_received",
                "compaction",
                prompt_tokens=prompt_tokens,
                total_tokens=usage.get("total_tokens", 0),
                completion_tokens=usage.get("completion_tokens", 0),
            )
            if prompt_tokens > 0:
                self._compactor.update_from_api_usage(prompt_tokens, len(ctx.messages))
                self._push_context_usage(ctx)

        # Log what the LLM decided to do
        _debug_log(
            f"[LLM_DECISION] content_len={len(content)}, "
            f"tool_calls={[tc['function']['name'] for tc in (tool_calls or [])]}"
        )

        # Display reasoning content via UI callback if thinking mode is ON
        # The visibility check is done inside on_thinking() which checks chat_app._thinking_visible
        if reasoning_content and ctx.ui_callback:
            if hasattr(ctx.ui_callback, "on_thinking"):
                ctx.ui_callback.on_thinking(reasoning_content)

        # Notify thinking complete
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_thinking_complete"):
            ctx.ui_callback.on_thinking_complete()

        # Record agent response
        self._record_agent_response(content, tool_calls)

        # Dispatch based on tool calls presence
        if not tool_calls:
            return self._handle_no_tool_calls(
                ctx, content, response.get("message", {}).get("content")
            )

        # Process tool calls
        return self._process_tool_calls(
            ctx, tool_calls, content, response.get("message", {}).get("content")
        )

    def _handle_llm_error(self, response: dict, ctx) -> "LoopAction":
        """Handle LLM errors."""
        from opendev.repl.react_executor.executor import LoopAction

        error_text = response.get("error", "Unknown error")
        _session_debug().log("llm_call_error", "llm", error=error_text)

        if "interrupted" in error_text.lower():
            self._last_error = error_text
            # Clear tracked values without persisting interrupt message
            # The interrupt message is already shown by ui_callback.on_interrupt()
            # We don't need to add a redundant message to the session
            self._current_thinking_trace = None
            self._current_reasoning_content = None
            self._current_token_usage = None

            if ctx.ui_callback and hasattr(ctx.ui_callback, "on_interrupt"):
                ctx.ui_callback.on_interrupt()
            elif not ctx.ui_callback and self.console:
                self.console.print(
                    "  ⎿  [bold red]Interrupted · What should I do instead?[/bold red]"
                )
        else:
            if self.console:
                self.console.print(f"[red]Error: {error_text}[/red]")
            # Include tracked metadata when persisting error
            fallback = ChatMessage(
                role=Role.ASSISTANT,
                content=f"{error_text}",
                thinking_trace=self._current_thinking_trace,
                reasoning_content=self._current_reasoning_content,
                token_usage=self._current_token_usage,
                metadata={"is_error": True},
            )
            self._last_error = error_text
            self.session_manager.add_message(fallback, self.config.auto_save_interval)
            # Clear tracked values after persistence
            self._current_thinking_trace = None
            self._current_reasoning_content = None
            self._current_token_usage = None

            if ctx.ui_callback and hasattr(ctx.ui_callback, "on_assistant_message"):
                ctx.ui_callback.on_assistant_message(fallback.content)

        return LoopAction.BREAK

    def _parse_llm_response(self, response: dict) -> tuple[str, list, Optional[str]]:
        """Parse LLM response into content, tool calls, and reasoning.

        Returns:
            Tuple of (content, tool_calls, reasoning_content):
            - content: The assistant's text response
            - tool_calls: List of tool calls to execute
            - reasoning_content: Native thinking/reasoning from models like o1 (may be None)
        """
        message_payload = response.get("message", {}) or {}
        raw_llm_content = message_payload.get("content")
        llm_description = response.get("content", raw_llm_content or "")

        tool_calls = response.get("tool_calls")
        if tool_calls is None:
            tool_calls = message_payload.get("tool_calls")

        # Extract reasoning_content for OpenAI reasoning models (o1, o3, etc.)
        reasoning_content = response.get("reasoning_content")

        return (llm_description or "").strip(), tool_calls, reasoning_content

    def _record_agent_response(self, content: str, tool_calls: Optional[list]):
        """Record agent response for ACE learning."""
        if self._tool_executor and hasattr(self._tool_executor, "set_last_agent_response"):
            self._tool_executor.set_last_agent_response(
                str(AgentResponse(content=content, tool_calls=tool_calls or []))
            )

    def _handle_no_tool_calls(self, ctx, content: str, raw_content: Optional[str]) -> "LoopAction":
        """Handle case where agent made no tool calls."""
        from opendev.repl.react_executor.executor import LoopAction

        # Check if last tool failed
        last_tool_failed = False
        for msg in reversed(ctx.messages):
            if msg.get("role") == "tool":
                msg_content = msg.get("content", "")
                if msg_content.startswith("Error"):
                    last_tool_failed = True
                break

        if last_tool_failed:
            return self._handle_failed_tool_nudge(ctx, content, raw_content)

        # Guard: nudge if there are incomplete todos before allowing implicit completion
        todo_handler = getattr(ctx.tool_registry, "todo_handler", None)
        if (
            todo_handler
            and todo_handler.has_todos()
            and todo_handler.has_incomplete_todos()
            and ctx.todo_nudge_count < self.MAX_TODO_NUDGES
        ):
            ctx.todo_nudge_count += 1
            incomplete = todo_handler.get_incomplete_todos()
            titles = [t.title for t in incomplete[:3]]
            nudge = get_reminder(
                "incomplete_todos_nudge",
                count=str(len(incomplete)),
                todo_list="\n".join(f"  - {t}" for t in titles),
            )
            if content:
                ctx.messages.append({"role": "assistant", "content": raw_content or content})
                self._display_message(content, ctx.ui_callback)
            append_nudge(ctx.messages, nudge)
            return LoopAction.CONTINUE

        # Check injection queue before accepting implicit completion
        if not self._injection_queue.empty():
            if content:
                ctx.messages.append({"role": "assistant", "content": raw_content or content})
                self._display_message(content, ctx.ui_callback)
            return LoopAction.CONTINUE

        # Nudge once for empty completion summary
        if not content and not ctx.completion_nudge_sent:
            ctx.completion_nudge_sent = True
            append_nudge(ctx.messages, get_reminder("completion_summary_nudge"))
            return LoopAction.CONTINUE

        # Accept completion (with or without content)
        if not content:
            content = "Done."

        self._display_message(content, ctx.ui_callback, dim=True)
        self._add_assistant_message(content, raw_content)
        return LoopAction.BREAK

    def _handle_failed_tool_nudge(
        self, ctx, content: str, raw_content: Optional[str]
    ) -> "LoopAction":
        """Nudge agent to retry after failure."""
        from opendev.repl.react_executor.executor import LoopAction

        ctx.consecutive_no_tool_calls += 1

        if ctx.consecutive_no_tool_calls >= self.MAX_NUDGE_ATTEMPTS:
            # Still check todos before accepting failure completion
            todo_handler = getattr(ctx.tool_registry, "todo_handler", None)
            if (
                todo_handler
                and todo_handler.has_incomplete_todos()
                and ctx.todo_nudge_count < self.MAX_TODO_NUDGES
            ):
                ctx.todo_nudge_count += 1
                incomplete = todo_handler.get_incomplete_todos()
                titles = [t.title for t in incomplete[:3]]
                nudge = get_reminder(
                    "incomplete_todos_nudge",
                    count=str(len(incomplete)),
                    todo_list="\n".join(f"  - {t}" for t in titles),
                )
                if content:
                    ctx.messages.append({"role": "assistant", "content": raw_content or content})
                    self._display_message(content, ctx.ui_callback)
                append_nudge(ctx.messages, nudge)
                ctx.consecutive_no_tool_calls = 0
                return LoopAction.CONTINUE

            if not content:
                content = "Warning: could not complete after multiple attempts."

            self._display_message(content, ctx.ui_callback, dim=True)
            self._add_assistant_message(content, raw_content)
            return LoopAction.BREAK

        # Nudge
        if content:
            ctx.messages.append({"role": "assistant", "content": raw_content or content})
            self._display_message(content, ctx.ui_callback)

        append_nudge(ctx.messages, get_reminder("failed_tool_nudge"))
        return LoopAction.CONTINUE

    def _should_nudge_agent(self, consecutive_reads: int, messages: list) -> bool:
        """Check if agent should be nudged to conclude."""
        if consecutive_reads >= 5:
            # Silently nudge the agent
            append_nudge(messages, get_reminder("consecutive_reads_nudge"))
            return True
        return False
