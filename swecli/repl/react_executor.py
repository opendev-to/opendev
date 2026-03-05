"""ReAct loop executor."""

import hashlib
import json
import logging
import os
import queue as queue_mod
from collections import deque
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from enum import Enum, auto
from pathlib import Path
from typing import TYPE_CHECKING, Optional, Dict, Any

# Maximum number of tools to execute in parallel
MAX_CONCURRENT_TOOLS = 5

# Safety cap to prevent runaway loops
MAX_REACT_ITERATIONS = 200

from swecli.models.message import ChatMessage, Role, ToolCall as ToolCallModel
from swecli.core.context_engineering.memory import AgentResponse
from swecli.core.runtime.monitoring import TaskMonitor
from swecli.ui_textual.utils.tool_display import format_tool_call
from swecli.ui_textual.components.task_progress import TaskProgressDisplay
from swecli.core.utils.tool_result_summarizer import summarize_tool_result
from swecli.core.agents.prompts import get_reminder
from swecli.core.utils.sound import play_finish_sound

logger = logging.getLogger(__name__)

_ctx_logger = logging.getLogger("swecli.context_debug")
_ctx_logger.setLevel(logging.DEBUG)
_fh = logging.FileHandler("/tmp/context_debug.log", mode="w")
_fh.setFormatter(logging.Formatter("%(asctime)s %(message)s"))
_ctx_logger.addHandler(_fh)


def _debug_log(message: str) -> None:
    """Write debug message to /tmp/swecli_react_debug.log."""
    from datetime import datetime

    log_file = "/tmp/swecli_react_debug.log"
    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
    with open(log_file, "a") as f:
        f.write(f"[{timestamp}] {message}\n")


def _session_debug() -> "SessionDebugLogger":
    """Get the current session debug logger."""
    from swecli.core.debug import get_debug_logger

    return get_debug_logger()


if TYPE_CHECKING:
    from rich.console import Console
    from swecli.core.context_engineering.history import SessionManager
    from swecli.models.config import Config
    from swecli.repl.llm_caller import LLMCaller
    from swecli.repl.tool_executor import ToolExecutor
    from swecli.core.runtime.approval import ApprovalManager
    from swecli.core.context_engineering.history import UndoManager
    from swecli.core.debug.session_debug_logger import SessionDebugLogger
    from swecli.core.runtime.cost_tracker import CostTracker


class LoopAction(Enum):
    """Action to take after an iteration."""

    CONTINUE = auto()
    BREAK = auto()


@dataclass
class IterationContext:
    """Context for a single ReAct iteration."""

    query: str
    messages: list
    agent: Any
    tool_registry: Any
    approval_manager: "ApprovalManager"
    undo_manager: "UndoManager"
    ui_callback: Optional[Any]
    iteration_count: int = 0
    consecutive_reads: int = 0
    consecutive_no_tool_calls: int = 0
    todo_nudge_count: int = 0
    plan_approved_signal_injected: bool = False
    all_todos_complete_nudged: bool = False
    completion_nudge_sent: bool = False
    continue_after_subagent: bool = False  # If True, don't inject stop signal after subagent
    # Doom-loop detection: track recent (tool_name, args_hash) tuples
    recent_tool_calls: deque = field(default_factory=lambda: deque(maxlen=20))
    doom_loop_warned: bool = False  # True if user already chose to continue


class ReactExecutor:
    """Executes ReAct loop (Reasoning → Acting → Observing)."""

    READ_OPERATIONS = {"read_file", "list_files", "search"}
    MAX_NUDGE_ATTEMPTS = 3
    MAX_TODO_NUDGES = 2  # After this many nudges, allow completion anyway
    DOOM_LOOP_THRESHOLD = 3  # Same tool+args N times → doom loop

    # Tools safe for silent parallel execution (read-only, no approval needed)
    PARALLELIZABLE_TOOLS = frozenset({
        "read_file", "list_files", "search",
        "fetch_url", "web_search", "capture_web_screenshot", "analyze_image",
        "list_processes", "get_process_output",
        "list_todos", "search_tools",
        "find_symbol", "find_referencing_symbols",
    })

    def __init__(
        self,
        console: "Console",
        session_manager: "SessionManager",
        config: "Config",
        llm_caller: "LLMCaller",
        tool_executor: "ToolExecutor",
        cost_tracker: Optional["CostTracker"] = None,
    ):
        """Initialize ReAct executor."""
        self.console = console
        self.session_manager = session_manager
        self.config = config
        self._llm_caller = llm_caller
        self._tool_executor = tool_executor
        self._cost_tracker = cost_tracker
        self._last_operation_summary = None
        self._last_error = None
        self._last_latency_ms = 0
        self._last_thinking_error: Optional[dict[str, Any]] = None

        # Tracking variables for current iteration (for session persistence)
        self._current_thinking_trace: Optional[str] = None
        self._current_reasoning_content: Optional[str] = None
        self._current_token_usage: Optional[dict] = None

        # Track current task monitor for interrupt support (thinking phase uses this)
        self._current_task_monitor: Optional[TaskMonitor] = None

        # Centralized interrupt token for the current run
        self._active_interrupt_token: Optional[Any] = None

        # Hook manager for lifecycle hooks
        self._hook_manager = None

        # Auto-compaction support
        self._compactor = None
        self._force_compact_next = False  # Set by /compact command

        # Shadow git snapshot system for per-step undo
        self._snapshot_manager = None

        # Persistent thread pool for parallel tool execution (enables connection reuse)
        self._parallel_executor = ThreadPoolExecutor(
            max_workers=MAX_CONCURRENT_TOOLS, thread_name_prefix="tool-worker"
        )

        # Live message injection queue (thread-safe, bounded)
        self._injection_queue: queue_mod.Queue[str] = queue_mod.Queue(maxsize=10)

        # Callback invoked when an injected message is consumed at a step boundary
        self._on_message_consumed: Optional[callable] = None
        # Callback for messages remaining after loop ends (need re-queuing)
        self._on_orphan_message: Optional[callable] = None


    def request_interrupt(self) -> bool:
        """Request interrupt of currently running task (thinking or tool execution).

        Returns:
            True if interrupt was requested, False if no task is running
        """
        from swecli.ui_textual.debug_logger import debug_log

        debug_log("ReactExecutor", "request_interrupt called")
        debug_log("ReactExecutor", f"_current_task_monitor={self._current_task_monitor}")

        if self._current_task_monitor is not None:
            self._current_task_monitor.request_interrupt()
            debug_log("ReactExecutor", "Called task_monitor.request_interrupt()")

        if self._active_interrupt_token is not None:
            self._active_interrupt_token.request()
            debug_log("ReactExecutor", "Called _active_interrupt_token.request()")
            return True

        if self._current_task_monitor is not None:
            return True
        debug_log("ReactExecutor", "No active task monitor")
        return False

    def set_hook_manager(self, hook_manager) -> None:
        """Set the hook manager for lifecycle hooks.

        Args:
            hook_manager: HookManager instance
        """
        self._hook_manager = hook_manager

    def inject_user_message(self, text: str) -> None:
        """Inject a user message into the running ReAct loop.

        Thread-safe. Called from the UI thread to deliver messages mid-execution.
        Messages exceeding the queue capacity (10) are logged and dropped.
        """
        try:
            self._injection_queue.put_nowait(text)
        except queue_mod.Full:
            logger.warning("Injection queue full, dropping message: %s", text[:80])

    def set_on_message_consumed(self, callback):
        self._on_message_consumed = callback

    def set_on_orphan_message(self, callback):
        self._on_orphan_message = callback

    def _drain_injected_messages(self, ctx: IterationContext, max_per_drain: int = 3) -> int:
        """Drain injected user messages into the conversation.

        Persists each message to the session and appends it to ctx.messages.
        Caps at *max_per_drain* messages per call; leftovers stay queued for
        the next iteration.

        Returns:
            Number of messages drained.
        """
        count = 0
        while count < max_per_drain:
            try:
                text = self._injection_queue.get_nowait()
            except queue_mod.Empty:
                break
            user_msg = ChatMessage(role=Role.USER, content=text)
            self.session_manager.add_message(user_msg, self.config.auto_save_interval)
            ctx.messages.append({"role": "user", "content": text})
            count += 1
            _debug_log(f"[INJECT] Drained injected message ({count}): {text[:60]}")
            if self._on_message_consumed is not None:
                try:
                    self._on_message_consumed(text)
                except Exception:
                    logger.debug("_on_message_consumed callback failed", exc_info=True)
        return count

    def _check_interrupt(self, phase: str = "") -> None:
        """Check interrupt token; raise InterruptedError if signaled.

        Call at every phase boundary in _run_iteration() to ensure
        prompt cancellation between thinking -> critique -> action -> tools.
        """
        if self._active_interrupt_token and self._active_interrupt_token.is_requested():
            _debug_log(f"[INTERRUPT] Token detected at phase boundary: {phase}")
            raise InterruptedError(
                f"Interrupted at {phase}" if phase else "Interrupted by user"
            )

    @staticmethod
    def _tool_call_fingerprint(tool_name: str, args_str: str) -> str:
        """Compute a compact fingerprint for a tool call (name + args hash)."""
        h = hashlib.md5(args_str.encode(), usedforsecurity=False).hexdigest()[:12]
        return f"{tool_name}:{h}"

    def _detect_doom_loop(
        self, tool_calls: list, ctx: IterationContext
    ) -> Optional[str]:
        """Check if the agent is stuck calling the same tool repeatedly.

        Returns a warning message if a doom loop is detected, None otherwise.
        """
        if ctx.doom_loop_warned:
            return None  # User already chose to continue

        for tc in tool_calls:
            fp = self._tool_call_fingerprint(
                tc["function"]["name"], tc["function"]["arguments"]
            )
            ctx.recent_tool_calls.append(fp)

        # Count recent occurrences of each fingerprint
        from collections import Counter

        counts = Counter(ctx.recent_tool_calls)
        for fp, count in counts.items():
            if count >= self.DOOM_LOOP_THRESHOLD:
                tool_name = fp.split(":")[0]
                return (
                    f"The agent has called `{tool_name}` with the same arguments "
                    f"{count} times. It may be stuck in a loop."
                )
        return None

    def _request_doom_loop_approval(
        self, ctx: IterationContext, warning: str,
    ) -> bool:
        """Pause execution and ask the user whether to continue or break.

        Uses the approval manager for genuine blocking pause. Falls back to
        injecting a warning if no approval manager is available.

        Returns:
            True if user allows the agent to continue, False to break the loop.
        """
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_message"):
            ctx.ui_callback.on_message(f"Doom-loop detected: {warning}")

        approval_manager = ctx.approval_manager
        if approval_manager is None:
            # No approval manager — fall back to automatic break
            return False

        from swecli.models.operation import Operation, OperationType
        from datetime import datetime

        operation = Operation(
            id=f"doom_loop_{datetime.now().timestamp()}",
            type=OperationType.BASH_EXECUTE,
            target="doom_loop_check",
            parameters={"warning": warning},
            created_at=datetime.now(),
        )

        try:
            import asyncio

            try:
                asyncio.get_running_loop()
                # In async context — can't block, auto-break
                return False
            except RuntimeError:
                pass

            result = approval_manager.request_approval(
                operation=operation,
                preview=warning,
                command=f"Agent is repeating: {warning}",
            )
            if hasattr(result, "approved"):
                return bool(result.approved)
            # If it's a coroutine, run it
            result = asyncio.run(result)
            return bool(result.approved)
        except Exception:
            logger.debug("Doom loop approval request failed", exc_info=True)
            return False

    def execute(
        self,
        query: str,
        messages: list,
        agent,
        tool_registry,
        approval_manager: "ApprovalManager",
        undo_manager: "UndoManager",
        ui_callback=None,
        continue_after_subagent: bool = False,
    ) -> tuple:
        """Execute ReAct loop."""

        # Clear stale injected messages from any previous execution (EC2)
        while not self._injection_queue.empty():
            try:
                self._injection_queue.get_nowait()
            except queue_mod.Empty:
                break

        from swecli.core.runtime.interrupt_token import InterruptToken

        # Create a single interrupt token for this entire run
        self._active_interrupt_token = InterruptToken()

        # Wire token to InterruptManager so ESC can signal it directly (Fix 1)
        _ui_callback = ui_callback  # Capture for finally block
        if _ui_callback and hasattr(_ui_callback, "chat_app"):
            app = _ui_callback.chat_app
            if app and hasattr(app, "_interrupt_manager"):
                app._interrupt_manager.set_interrupt_token(self._active_interrupt_token)

        # Wrap messages in ValidatedMessageList for write-time invariant enforcement
        from swecli.core.context_engineering.validated_message_list import ValidatedMessageList

        if not isinstance(messages, ValidatedMessageList):
            messages = ValidatedMessageList(messages)

        # Initialize context
        ctx = IterationContext(
            query=query,
            messages=messages,
            agent=agent,
            tool_registry=tool_registry,
            approval_manager=approval_manager,
            undo_manager=undo_manager,
            ui_callback=ui_callback,
            continue_after_subagent=continue_after_subagent,
        )

        # Restore cost tracker state from session metadata (for --continue)
        if self._cost_tracker:
            session = self.session_manager.get_current_session()
            if session and session.metadata.get("cost_tracking"):
                self._cost_tracker.restore_from_metadata(session.metadata)

        # Initialize snapshot manager for per-step undo
        if self._snapshot_manager is None:
            try:
                from swecli.core.context_engineering.history.snapshot import SnapshotManager

                working_dir = getattr(self.config, "working_directory", None) or os.getcwd()
                self._snapshot_manager = SnapshotManager(working_dir)
                # Capture initial state
                self._snapshot_manager.track()
            except Exception:
                logger.debug("Failed to initialize snapshot manager", exc_info=True)

        # Notify UI start
        if ui_callback and hasattr(ui_callback, "on_thinking_start"):
            ui_callback.on_thinking_start()

        # Debug: Query processing started
        if ui_callback and hasattr(ui_callback, "on_debug"):
            ui_callback.on_debug(
                f"Processing query: {query[:50]}{'...' if len(query) > 50 else ''}", "QUERY"
            )

        try:
            while True:
                # Drain any injected user messages before this iteration
                self._drain_injected_messages(ctx)

                ctx.iteration_count += 1

                # Check centralized interrupt token at each iteration boundary
                if self._active_interrupt_token and self._active_interrupt_token.is_requested():
                    _debug_log("[INTERRUPT] Token triggered, breaking loop")
                    if ctx.ui_callback and hasattr(ctx.ui_callback, "on_interrupt"):
                        ctx.ui_callback.on_interrupt()
                    break

                if ctx.iteration_count > MAX_REACT_ITERATIONS:
                    _debug_log(f"[SAFETY] Hit iteration limit ({MAX_REACT_ITERATIONS})")
                    if ctx.ui_callback and hasattr(ctx.ui_callback, "on_assistant_message"):
                        ctx.ui_callback.on_assistant_message(
                            "Reached maximum iteration limit."
                            " Please provide further instructions."
                        )
                    break

                _session_debug().log(
                    "react_iteration_start",
                    "react",
                    iteration=ctx.iteration_count,
                    query_preview=query[:200],
                    message_count=len(messages),
                )
                action = self._run_iteration(ctx)
                _session_debug().log(
                    "react_iteration_end",
                    "react",
                    iteration=ctx.iteration_count,
                    action=action.name.lower(),
                )
                if action == LoopAction.BREAK:
                    # Don't break if new messages arrived during this iteration
                    if not self._injection_queue.empty():
                        _debug_log("[INJECT] New messages in queue, continuing loop")
                        continue
                    break
        except Exception as e:
            self.console.print(f"[red]Error: {str(e)}[/red]")
            import traceback

            tb = traceback.format_exc()
            traceback.print_exc()
            self._last_error = str(e)
            _session_debug().log(
                "error", "react", error=str(e), traceback=tb
            )
        finally:
            interrupted = bool(
                self._active_interrupt_token
                and self._active_interrupt_token.is_requested()
            )

            # Fix 4: If interrupted but on_interrupt wasn't called yet, call it now
            if (
                interrupted
                and _ui_callback
                and hasattr(_ui_callback, "on_interrupt")
            ):
                _ui_callback.on_interrupt()

            # Fix 1: Clear token from InterruptManager
            if _ui_callback and hasattr(_ui_callback, "chat_app"):
                app = _ui_callback.chat_app
                if app and hasattr(app, "_interrupt_manager"):
                    app._interrupt_manager.clear_interrupt_token()

            self._active_interrupt_token = None

        # Final drain: re-queue or persist any late-arriving injected messages (EC1)
        while True:
            try:
                text = self._injection_queue.get_nowait()
                if self._on_orphan_message is not None:
                    self._on_orphan_message(text)
                else:
                    # Fallback: persist (preserves original behavior for non-TUI)
                    user_msg = ChatMessage(role=Role.USER, content=text)
                    self.session_manager.add_message(user_msg, self.config.auto_save_interval)
            except queue_mod.Empty:
                break

        # Clear callbacks (owned by the caller, not us)
        self._on_message_consumed = None
        self._on_orphan_message = None

        # Ensure session metadata (context_usage_pct, compaction_point, etc.)
        # is flushed to disk — auto-save may not have fired on the last turn.
        try:
            self.session_manager.save_session()
        except Exception:
            logger.debug("Final session save failed", exc_info=True)

        # Fire Stop hook (can prevent stopping by returning exit code 2)
        if self._hook_manager and not interrupted:
            from swecli.core.hooks.models import HookEvent

            if self._hook_manager.has_hooks_for(HookEvent.STOP):
                stop_outcome = self._hook_manager.run_hooks(HookEvent.STOP)
                if stop_outcome.blocked:
                    _debug_log("[HOOK] Stop hook blocked — would continue loop")
                    # Note: We can't re-enter the loop from here since we've
                    # already exited the while-loop. The Stop hook blocking
                    # is logged but the agent has already committed to stopping.
                    # Future: could set a flag for the next execution.

        # Play finish sound if enabled and NOT interrupted
        if getattr(self.config, "enable_sound", False) and not interrupted:
            play_finish_sound()

        return (self._last_operation_summary, self._last_error, self._last_latency_ms)

    def _build_messages_with_system_prompt(
        self, messages: list, system_prompt: str
    ) -> list:
        """Clone messages and replace the system prompt.

        Both thinking and main phases use this to build their message arrays
        from the same compacted base messages.
        """
        result = list(messages)  # shallow clone
        if result and result[0].get("role") == "system":
            result[0] = {"role": "system", "content": system_prompt}
        else:
            result.insert(0, {"role": "system", "content": system_prompt})
        return result

    def _get_thinking_trace(
        self,
        messages: list,
        agent,
        ui_callback=None,
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
        try:
            # Build thinking-specific system prompt
            thinking_system_prompt = agent.build_system_prompt(thinking_visible=True)

            # Clone messages with swapped system prompt
            thinking_messages = self._build_messages_with_system_prompt(
                messages, thinking_system_prompt
            )

            # Append analysis prompt as final user message
            thinking_messages.append(
                {
                    "role": "user",
                    "content": get_reminder("thinking_analysis_prompt"),
                },
            )

            # Call LLM WITHOUT tools - just get reasoning
            task_monitor = TaskMonitor()
            if self._active_interrupt_token:
                task_monitor.set_interrupt_token(self._active_interrupt_token)
            # Track task monitor for interrupt support
            self._current_task_monitor = task_monitor
            from swecli.ui_textual.debug_logger import debug_log

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
        from swecli.core.runtime.monitoring import TaskMonitor

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
        from swecli.core.runtime.monitoring import TaskMonitor

        try:
            # Build refinement system prompt
            refinement_system = agent.build_system_prompt(thinking_visible=True)

            # Clone messages with swapped system prompt
            refinement_messages = self._build_messages_with_system_prompt(
                messages, refinement_system
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
                            ui_callback.on_thinking(f"[Refined]\n{refined}")
                        return refined
            finally:
                self._current_task_monitor = None

        except Exception as e:
            if ui_callback and hasattr(ui_callback, "on_debug"):
                ui_callback.on_debug(f"Refinement error: {str(e)}", "CRITIQUE")

        return None

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

    def _maybe_compact(self, ctx: IterationContext) -> None:
        """Staged context optimization as usage grows.

        Stages:
        - 70%: Warning logged
        - 80%: Progressive observation masking (old tool results → compact refs)
        - 90%: Aggressive masking (only recent 3 tool results kept)
        - 99%: Full LLM-powered compaction
        """
        if self._compactor is None:
            from swecli.core.context_engineering.compaction import ContextCompactor

            self._compactor = ContextCompactor(self.config, ctx.agent._http_client)

        # Set session ID for scratch file paths
        session = self.session_manager.get_current_session()
        if session is not None:
            self._compactor._session_id = session.id

        system_prompt = ctx.agent.system_prompt

        # Check staged optimization level
        from swecli.core.context_engineering.compaction import OptimizationLevel

        level = self._compactor.check_usage(ctx.messages, system_prompt)

        # Push context usage % to UI
        self._push_context_usage(ctx)

        # Apply progressive observation masking at 80% and 90%
        if level in (OptimizationLevel.MASK, OptimizationLevel.AGGRESSIVE):
            self._compactor.mask_old_observations(ctx.messages, level)

        # Fast pruning at 85%: strip old tool outputs (cheaper than LLM compaction)
        if level == OptimizationLevel.PRUNE:
            self._compactor.prune_old_tool_outputs(ctx.messages)

        # Full compaction at 99% or manual /compact
        should = self._force_compact_next or level == OptimizationLevel.COMPACT

        if should:
            self._force_compact_next = False
            before_count = len(ctx.messages)
            compacted = self._compactor.compact(ctx.messages, system_prompt)
            ctx.messages[:] = compacted  # Mutate in-place
            after_count = len(ctx.messages)
            logger.info("Compacted %d messages → %d", before_count, after_count)
            # Store compaction point in session metadata (like /compact)
            if session is not None:
                summary_msg = next(
                    (
                        m
                        for m in compacted
                        if m.get("content", "").startswith("[CONVERSATION SUMMARY]")
                    ),
                    None,
                )
                if summary_msg:
                    session.metadata["compaction_point"] = {
                        "summary": summary_msg["content"],
                        "at_message_count": len(session.messages),
                    }
                    self.session_manager.save_session()
            if ctx.ui_callback and hasattr(ctx.ui_callback, "on_message"):
                ctx.ui_callback.on_message(
                    f"Context auto-compacted ({before_count} → {after_count} messages)"
                )

    def _push_context_usage(self, ctx: IterationContext) -> None:
        """Push current context usage percentage to the UI (best-effort)."""
        try:
            if (
                self._compactor
                and ctx.ui_callback
                and hasattr(ctx.ui_callback, "on_context_usage")
            ):
                pct = self._compactor.usage_pct
                new_msg = len(ctx.messages) - self._compactor._msg_count_at_calibration
                _ctx_logger.info(
                    "context_usage_push: pct=%.2f last_tok=%d max_ctx=%d "
                    "api_prompt_tok=%d msg_count_at_cal=%d cur_msg_count=%d new_msgs=%d",
                    pct,
                    self._compactor._last_token_count,
                    self._compactor._max_context,
                    self._compactor._api_prompt_tokens,
                    self._compactor._msg_count_at_calibration,
                    len(ctx.messages),
                    max(0, new_msg),
                )
                _session_debug().log(
                    "context_usage_push",
                    "compaction",
                    usage_pct=pct,
                    last_token_count=self._compactor._last_token_count,
                    max_context=self._compactor._max_context,
                    api_prompt_tokens=self._compactor._api_prompt_tokens,
                    msg_count_at_cal=self._compactor._msg_count_at_calibration,
                )
                ctx.ui_callback.on_context_usage(pct)
                # Persist for session resume
                session = self.session_manager.get_current_session()
                if session is not None:
                    session.metadata["context_usage_pct"] = round(pct, 1)
        except Exception as exc:
            logger.debug("_push_context_usage failed: %s", exc)

    def _run_iteration(self, ctx: IterationContext) -> LoopAction:
        """Run a single ReAct iteration."""
        try:
            return self._run_iteration_inner(ctx)
        except InterruptedError:
            _debug_log("[INTERRUPT] Caught InterruptedError in _run_iteration")
            if ctx.ui_callback and hasattr(ctx.ui_callback, "on_interrupt"):
                ctx.ui_callback.on_interrupt()
            return LoopAction.BREAK

    def _run_iteration_inner(self, ctx: IterationContext) -> LoopAction:
        """Inner implementation of _run_iteration (wrapped by interrupt handler)."""

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
        if thinking_visible and not subagent_just_completed:
            thinking_trace = self._get_thinking_trace(ctx.messages, ctx.agent, ctx.ui_callback)

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
        from swecli.ui_textual.debug_logger import debug_log

        debug_log(
            "ReactExecutor",
            f"Calling call_llm_with_progress, _llm_caller={id(self._llm_caller)}, task_monitor={task_monitor}",
        )
        _session_debug().log(
            "llm_call_start",
            "llm",
            model=getattr(ctx.agent, "model", "unknown"),
            message_count=len(ctx.messages),
            thinking_visible=thinking_visible,
        )
        response, latency_ms = self._llm_caller.call_llm_with_progress(
            ctx.agent, ctx.messages, task_monitor, thinking_visible=thinking_visible
        )
        debug_log(
            "ReactExecutor", f"call_llm_with_progress returned, success={response.get('success')}"
        )
        self._last_latency_ms = latency_ms
        _session_debug().log(
            "llm_call_end",
            "llm",
            duration_ms=latency_ms,
            success=response.get("success", False),
            tokens=response.get("usage"),
            has_tool_calls=bool(response.get("tool_calls") or (response.get("message") or {}).get("tool_calls")),
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

    def _handle_llm_error(self, response: dict, ctx: IterationContext) -> LoopAction:
        """Handle LLM errors."""
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
            elif not ctx.ui_callback:
                self.console.print(
                    "  ⎿  [bold red]Interrupted · What should I do instead?[/bold red]"
                )
        else:
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
        if hasattr(self._tool_executor, "set_last_agent_response"):
            self._tool_executor.set_last_agent_response(
                str(AgentResponse(content=content, tool_calls=tool_calls or []))
            )

    def _handle_no_tool_calls(
        self, ctx: IterationContext, content: str, raw_content: Optional[str]
    ) -> LoopAction:
        """Handle case where agent made no tool calls."""
        # Check if last tool failed
        last_tool_failed = False
        for msg in reversed(ctx.messages):
            if msg.get("role") == "tool":
                msg_content = msg.get("content", "")
                if msg_content.startswith("Error:"):
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
            ctx.messages.append({"role": "user", "content": nudge})
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
            ctx.messages.append(
                {"role": "user", "content": get_reminder("completion_summary_nudge")}
            )
            return LoopAction.CONTINUE

        # Accept completion (with or without content)
        if not content:
            content = "Done."

        self._display_message(content, ctx.ui_callback, dim=True)
        self._add_assistant_message(content, raw_content)
        return LoopAction.BREAK

    def _handle_failed_tool_nudge(
        self, ctx: IterationContext, content: str, raw_content: Optional[str]
    ) -> LoopAction:
        """Nudge agent to retry after failure."""
        ctx.consecutive_no_tool_calls += 1

        if ctx.consecutive_no_tool_calls >= self.MAX_NUDGE_ATTEMPTS:
            if not content:
                content = "Warning: could not complete after multiple attempts."

            self._display_message(content, ctx.ui_callback, dim=True)
            self._add_assistant_message(content, raw_content)
            return LoopAction.BREAK

        # Nudge
        if content:
            ctx.messages.append({"role": "assistant", "content": raw_content or content})
            self._display_message(content, ctx.ui_callback)

        ctx.messages.append(
            {
                "role": "user",
                "content": get_reminder("failed_tool_nudge"),
            }
        )
        return LoopAction.CONTINUE

    def _process_tool_calls(
        self, ctx: IterationContext, tool_calls: list, content: str, raw_content: Optional[str]
    ) -> LoopAction:
        """Process a list of tool calls."""
        import json

        # Reset no-tool-call counter
        ctx.consecutive_no_tool_calls = 0

        # Doom-loop detection: pause execution and ask user how to proceed
        doom_warning = self._detect_doom_loop(tool_calls, ctx)
        if doom_warning:
            _debug_log(f"[DOOM_LOOP] {doom_warning}")

            # Request user approval before continuing — genuine execution pause
            user_allowed = self._request_doom_loop_approval(ctx, doom_warning)

            if user_allowed:
                # User chose to continue — mark as warned so we don't ask again
                ctx.doom_loop_warned = True
            else:
                # User chose to break — inject guidance and skip these tool calls
                ctx.messages.append(
                    {
                        "role": "user",
                        "content": (
                            f"[SYSTEM WARNING] {doom_warning}\n"
                            "You appear to be repeating the same action without progress. "
                            "Please try a completely different approach or explain what "
                            "you're trying to accomplish so we can find a better path."
                        ),
                    }
                )
                # Reset the recent tool calls deque to give the agent a fresh start
                ctx.recent_tool_calls.clear()
                return LoopAction.CONTINUE

        # Check for task completion FIRST (before displaying content)
        # This prevents duplicate ⏺ bullets (one for content, one for summary)
        task_complete_call = next(
            (tc for tc in tool_calls if tc["function"]["name"] == "task_complete"), None
        )
        if task_complete_call:
            args = json.loads(task_complete_call["function"]["arguments"])
            summary = args.get("summary", "Task completed")
            status = args.get("status", "success")

            # Block completion if todos are incomplete (ported from main_agent.py)
            if status == "success":
                todo_handler = getattr(ctx.tool_registry, "todo_handler", None)
                if todo_handler and todo_handler.has_incomplete_todos():
                    if ctx.todo_nudge_count < self.MAX_TODO_NUDGES:
                        ctx.todo_nudge_count += 1
                        incomplete = todo_handler.get_incomplete_todos()
                        titles = [t.title for t in incomplete[:3]]
                        nudge = get_reminder(
                            "incomplete_todos_nudge",
                            count=str(len(incomplete)),
                            todo_list="\n".join(f"  - {t}" for t in titles),
                        )
                        ctx.messages.append({"role": "assistant", "content": summary})
                        ctx.messages.append({"role": "user", "content": nudge})
                        return LoopAction.CONTINUE

            # Check injection queue before accepting task_complete
            if not self._injection_queue.empty():
                _debug_log("[INJECT] task_complete deferred: new user messages in queue")
                ctx.messages.append({"role": "assistant", "content": summary})
                self._display_message(summary, ctx.ui_callback)
                return LoopAction.CONTINUE

            self._display_message(summary, ctx.ui_callback, dim=True)
            self._add_assistant_message(summary, raw_content)
            return LoopAction.BREAK

        # Display thinking (only when NOT task_complete)
        if content:
            self._display_message(content, ctx.ui_callback)

        # Add assistant message to history
        ctx.messages.append(
            {
                "role": "assistant",
                "content": raw_content,
                "tool_calls": tool_calls,
            }
        )

        # Track reads for nudging
        all_reads = all(tc["function"]["name"] in self.READ_OPERATIONS for tc in tool_calls)
        ctx.consecutive_reads = ctx.consecutive_reads + 1 if all_reads else 0

        # Execute tools (parallel for spawn_subagent batches or read-only batches)
        spawn_calls = [tc for tc in tool_calls if tc["function"]["name"] == "spawn_subagent"]
        is_all_spawn_agents = len(spawn_calls) == len(tool_calls) and len(spawn_calls) > 1
        is_all_parallelizable = (
            len(tool_calls) > 1
            and all(
                tc["function"]["name"] in self.PARALLELIZABLE_TOOLS for tc in tool_calls
            )
        )

        tool_denied = False
        if is_all_spawn_agents or is_all_parallelizable:
            # Parallel execution: subagent batches or read-only tool batches
            tool_results_by_id, operation_cancelled = self._execute_tools_parallel(tool_calls, ctx)
        else:
            # Sequential execution for all other tool calls
            tool_results_by_id = {}
            operation_cancelled = False
            for tool_call in tool_calls:
                # Check interrupt BEFORE executing the next tool (Fix 6)
                if (
                    self._active_interrupt_token
                    and self._active_interrupt_token.is_requested()
                ):
                    tool_results_by_id[tool_call["id"]] = {
                        "success": False,
                        "error": "Interrupted by user",
                        "output": None,
                        "interrupted": True,
                    }
                    operation_cancelled = True
                    break

                result = self._execute_single_tool(tool_call, ctx)
                tool_results_by_id[tool_call["id"]] = result
                if result.get("interrupted", False):
                    if result.get("denied", False):
                        tool_denied = True
                    else:
                        operation_cancelled = True
                    break

        # Guard: ensure every tool_call has a result (fills missing with synthetic errors)
        from swecli.core.context_engineering.message_pair_validator import (
            MessagePairValidator,
        )

        tool_results_by_id = MessagePairValidator.validate_tool_results_complete(
            tool_calls, tool_results_by_id
        )

        # Snapshot tracking: capture state after write operations
        if self._snapshot_manager and not operation_cancelled:
            _write_tools = {"write_file", "edit_file", "run_command"}
            has_writes = any(
                tc["function"]["name"] in _write_tools for tc in tool_calls
            )
            if has_writes:
                self._snapshot_manager.track()

        # Check if agent has subagent capability (for dynamic truncation hints)
        _has_subagent = "spawn_subagent" in getattr(ctx.tool_registry, "_handlers", {})

        # Batch add all results after completion (maintains message order)
        for tool_call in tool_calls:
            self._add_tool_result_to_history(
                ctx.messages, tool_call, tool_results_by_id[tool_call["id"]],
                has_subagent_tool=_has_subagent,
            )

        # Inject plan execution signal after plan approval
        for tool_call in tool_calls:
            if tool_call["function"]["name"] == "present_plan":
                tc_result = tool_results_by_id.get(tool_call["id"], {})
                if tc_result.get("plan_approved") and not ctx.plan_approved_signal_injected:
                    ctx.plan_approved_signal_injected = True
                    todos_created = tc_result.get("todos_created", 0)
                    plan_content = tc_result.get("plan_content", "")
                    ctx.messages.append(
                        {
                            "role": "user",
                            "content": get_reminder(
                                "plan_approved_signal",
                                todos_created=str(todos_created),
                                plan_content=plan_content,
                            ),
                        }
                    )
                    break

        # Nudge agent to finish when all todos are done (at most once)
        if not ctx.all_todos_complete_nudged:
            todo_handler = getattr(ctx.tool_registry, "todo_handler", None)
            if (
                todo_handler
                and todo_handler.has_todos()
                and not todo_handler.has_incomplete_todos()
            ):
                ctx.all_todos_complete_nudged = True
                ctx.messages.append(
                    {
                        "role": "user",
                        "content": get_reminder("all_todos_complete_nudge"),
                    }
                )

        # Update context usage indicator after tool results are added
        if self._compactor:
            _ctx_logger.info(
                "context_usage_after_tools: msg_count=%d", len(ctx.messages)
            )
            _session_debug().log(
                "context_usage_after_tools",
                "compaction",
                message_count=len(ctx.messages),
            )
            self._compactor.should_compact(ctx.messages, ctx.agent.system_prompt)
            self._push_context_usage(ctx)

        if operation_cancelled:
            return LoopAction.BREAK

        if tool_denied:
            ctx.messages.append(
                {
                    "role": "user",
                    "content": get_reminder("tool_denied_nudge"),
                }
            )

        # Persist and Learn
        _debug_log("[TOOLS] Before _persist_step")
        self._persist_step(ctx, tool_calls, tool_results_by_id, content, raw_content)
        _debug_log("[TOOLS] After _persist_step")

        # Check nudge for reads
        if self._should_nudge_agent(ctx.consecutive_reads, ctx.messages):
            ctx.consecutive_reads = 0

        _debug_log("[TOOLS] Returning LoopAction.CONTINUE")
        return LoopAction.CONTINUE

    def _execute_single_tool(
        self, tool_call: dict, ctx: IterationContext, suppress_separate_response: bool = False
    ) -> dict:
        """Execute a single tool and handle UI updates.

        Args:
            tool_call: The tool call dict from LLM response
            ctx: Iteration context with registry, callbacks, etc.
            suppress_separate_response: If True, don't display separate_response immediately.
                Used in parallel mode to aggregate responses later.
        """
        tool_name = tool_call["function"]["name"]

        if tool_name == "task_complete":
            return {}

        # Debug
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_debug"):
            ctx.ui_callback.on_debug(f"Executing tool: {tool_name}", "TOOL")

        args_str = tool_call["function"]["arguments"]
        _session_debug().log(
            "tool_call_start", "tool", name=tool_name, params_preview=args_str[:200]
        )

        # Notify UI call
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_tool_call"):
            ctx.ui_callback.on_tool_call(tool_name, args_str)

        # Execute
        import time as _time

        tool_start = _time.monotonic()
        try:
            result = self._execute_tool_call(
                tool_call,
                ctx.tool_registry,
                ctx.approval_manager,
                ctx.undo_manager,
                ui_callback=ctx.ui_callback,
            )
        except Exception as exc:
            import traceback

            _session_debug().log(
                "tool_call_error",
                "tool",
                name=tool_name,
                error=str(exc),
                traceback=traceback.format_exc(),
            )
            raise
        tool_duration_ms = int((_time.monotonic() - tool_start) * 1000)

        result_preview = (result.get("output") or result.get("error") or "")[:200]
        _session_debug().log(
            "tool_call_end",
            "tool",
            name=tool_name,
            duration_ms=tool_duration_ms,
            success=result.get("success", False),
            result_preview=result_preview,
        )

        # Store summary
        self._last_operation_summary = format_tool_call(
            tool_name, json.loads(args_str)
        )

        # Notify UI result
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_tool_result"):
            ctx.ui_callback.on_tool_result(tool_name, args_str, result)

        # Handle subagent display (suppress in parallel mode for aggregation)
        separate_response = result.get("separate_response")
        if separate_response and not suppress_separate_response:
            self._display_message(separate_response, ctx.ui_callback)

        return result

    def _execute_tool_quietly(self, tool_call: dict, ctx: IterationContext) -> dict:
        """Execute a tool without UI notifications (for silent parallel mode).

        Skips on_tool_call/on_tool_result callbacks and spinner display.
        Keeps debug logging and interrupt support.
        """
        import time as _time
        import traceback

        tool_name = tool_call["function"]["name"]
        if tool_name == "task_complete":
            return {}

        tool_args = json.loads(tool_call["function"]["arguments"])
        tool_call_id = tool_call["id"]
        args_str = tool_call["function"]["arguments"]
        _session_debug().log(
            "tool_call_start", "tool", name=tool_name, params_preview=args_str[:200]
        )

        tool_monitor = TaskMonitor()
        if self._active_interrupt_token:
            tool_monitor.set_interrupt_token(self._active_interrupt_token)

        tool_start = _time.monotonic()
        try:
            result = ctx.tool_registry.execute_tool(
                tool_name,
                tool_args,
                mode_manager=self._tool_executor.mode_manager,
                approval_manager=ctx.approval_manager,
                undo_manager=ctx.undo_manager,
                task_monitor=tool_monitor,
                session_manager=self.session_manager,
                ui_callback=ctx.ui_callback,
                tool_call_id=tool_call_id,
            )
        except Exception as exc:
            _session_debug().log(
                "tool_call_error", "tool", name=tool_name,
                error=str(exc), traceback=traceback.format_exc(),
            )
            return {"success": False, "error": str(exc)}

        tool_duration_ms = int((_time.monotonic() - tool_start) * 1000)
        result_preview = (result.get("output") or result.get("error") or "")[:200]
        _session_debug().log(
            "tool_call_end", "tool", name=tool_name, duration_ms=tool_duration_ms,
            success=result.get("success", False), result_preview=result_preview,
        )
        return result

    def _execute_tools_parallel(
        self, tool_calls: list, ctx: IterationContext
    ) -> tuple[Dict[str, dict], bool]:
        """Execute tools in parallel using managed thread pool.

        Uses `with` statement to ensure executor cleanup (no memory leaks).
        ThreadPoolExecutor's max_workers naturally limits concurrency.

        Args:
            tool_calls: List of tool call dicts from LLM response
            ctx: Iteration context with registry, callbacks, etc.

        Returns:
            Tuple of (results_by_id dict, operation_cancelled bool)
        """
        tool_results_by_id: Dict[str, dict] = {}
        operation_cancelled = False
        ui_callback = ctx.ui_callback

        # Check if ALL tools are spawn_subagent (parallel agent scenario)
        spawn_calls = [tc for tc in tool_calls if tc["function"]["name"] == "spawn_subagent"]
        is_parallel_agents = len(spawn_calls) == len(tool_calls) and len(spawn_calls) > 1

        # Build agent info mapping (tool_call_id -> agent info)
        # Pass full agent info to UI for individual agent tracking
        agent_name_map: Dict[str, str] = {}
        if is_parallel_agents and ui_callback:
            # Collect full agent info for each parallel agent
            agent_infos: list[dict] = []
            for tc in spawn_calls:
                args = json.loads(tc["function"]["arguments"])
                agent_type = args.get("subagent_type", "Agent")
                description = args.get("description", "")
                tool_call_id = tc["id"]
                # Map tool_call_id to base type (for completion tracking)
                agent_name_map[tool_call_id] = agent_type
                # Collect full info for UI display
                agent_infos.append(
                    {
                        "agent_type": agent_type,
                        "description": description,
                        "tool_call_id": tool_call_id,
                    }
                )
            if hasattr(ui_callback, "on_parallel_agents_start"):
                import sys

                print(
                    f"[DEBUG] on_parallel_agents_start with agent_infos={agent_infos}",
                    file=sys.stderr,
                )
                ui_callback.on_parallel_agents_start(agent_infos)

        # Check interrupt before launching parallel execution
        if self._active_interrupt_token and self._active_interrupt_token.is_requested():
            for tc in tool_calls:
                tool_results_by_id[tc["id"]] = {
                    "success": False,
                    "error": "Interrupted by user",
                    "output": None,
                    "interrupted": True,
                }
            return tool_results_by_id, True

        executor = self._parallel_executor

        if is_parallel_agents:
            # --- Existing subagent path (with per-agent UI tracking) ---
            future_to_call = {
                executor.submit(
                    self._execute_single_tool,
                    tc,
                    ctx,
                    suppress_separate_response=True,
                ): tc
                for tc in tool_calls
            }

            for future in as_completed(future_to_call):
                tool_call = future_to_call[future]
                try:
                    result = future.result()
                except Exception as e:
                    result = {"success": False, "error": str(e)}

                tool_results_by_id[tool_call["id"]] = result
                if result.get("interrupted"):
                    operation_cancelled = True

                # Track individual agent completion
                if ui_callback:
                    tool_name = tool_call["function"]["name"]
                    if tool_name == "spawn_subagent":
                        tool_call_id = tool_call["id"]
                        success = (
                            result.get("success", True)
                            if isinstance(result, dict)
                            else True
                        )
                        if hasattr(ui_callback, "on_parallel_agent_complete"):
                            ui_callback.on_parallel_agent_complete(
                                tool_call_id, success
                            )

            # Notify UI that all parallel agents are done
            if ui_callback and hasattr(ui_callback, "on_parallel_agents_done"):
                ui_callback.on_parallel_agents_done()

        else:
            # --- Silent parallel: execute concurrently, display sequentially ---
            future_to_call = {
                executor.submit(self._execute_tool_quietly, tc, ctx): tc
                for tc in tool_calls
            }
            for future in as_completed(future_to_call):
                tool_call = future_to_call[future]
                try:
                    result = future.result()
                except Exception as e:
                    result = {"success": False, "error": str(e)}
                tool_results_by_id[tool_call["id"]] = result
                if result.get("interrupted"):
                    operation_cancelled = True

            # Replay display in original order (looks sequential to user)
            for tc in tool_calls:
                result = tool_results_by_id.get(tc["id"], {})
                tool_name = tc["function"]["name"]
                args_str = tc["function"]["arguments"]
                self._last_operation_summary = format_tool_call(
                    tool_name, json.loads(args_str)
                )
                if ui_callback and hasattr(ui_callback, "on_tool_call"):
                    ui_callback.on_tool_call(tool_name, args_str)
                if ui_callback and hasattr(ui_callback, "on_tool_result"):
                    ui_callback.on_tool_result(tool_name, args_str, result)

        return tool_results_by_id, operation_cancelled

    # Threshold for offloading tool output to scratch files (chars, ~2000 tokens)
    OFFLOAD_THRESHOLD = 8000

    def _add_tool_result_to_history(
        self, messages: list, tool_call: dict, result: dict,
        *, has_subagent_tool: bool = False,
    ):
        """Add tool execution result to message history.

        Large outputs (>8000 chars) are offloaded to scratch files and replaced
        with a summary + file reference, preventing context bloat.
        """
        tool_name = tool_call["function"]["name"]

        separate_response = result.get("separate_response")
        completion_status = result.get("completion_status")

        if result.get("success", False):
            tool_result = separate_response if separate_response else result.get("output", "")
            if completion_status:
                tool_result = f"[completion_status={completion_status}]\n{tool_result}"
        else:
            tool_result = f"Error: {result.get('error', 'Tool execution failed')}"

        # Offload large outputs to scratch files
        tool_result = self._maybe_offload_output(
            tool_name, tool_call["id"], tool_result,
            has_subagent_tool=has_subagent_tool,
        )

        _ctx_logger.info(
            "tool_result_added: tool=%s content_len=%d",
            tool_name,
            len(tool_result) if tool_result else 0,
        )

        messages.append(
            {
                "role": "tool",
                "tool_call_id": tool_call["id"],
                "content": tool_result,
            }
        )

    def _maybe_offload_output(
        self, tool_name: str, tool_call_id: str, output: str,
        *, has_subagent_tool: bool = False,
    ) -> str:
        """Offload large tool output to a scratch file, return summary + ref.

        Tool outputs are ~80% of context token usage. Writing outputs >8000 chars
        to scratch files and replacing them with a summary + file reference
        dramatically reduces context consumption.

        Args:
            tool_name: Name of the tool.
            tool_call_id: Unique tool call ID for the filename.
            output: Full tool output string.
            has_subagent_tool: Whether the current agent can spawn subagents.

        Returns:
            Original output if small enough, or summary + file reference.
        """
        if not output or len(output) <= self.OFFLOAD_THRESHOLD:
            return output

        # Don't offload subagent results or completion status messages
        if "[completion_status=" in output or "[SYNC COMPLETE]" in output:
            return output

        # Determine session ID for file path
        session = self.session_manager.get_current_session()
        session_id = session.id if session else "unknown"
        scratch_dir = Path.home() / ".opendev" / "scratch" / session_id

        try:
            scratch_dir.mkdir(parents=True, exist_ok=True)
            # Use tool name + truncated call ID for readable filenames
            safe_name = tool_name.replace("/", "_")
            short_id = tool_call_id[:8] if tool_call_id else "unknown"
            scratch_path = scratch_dir / f"{safe_name}_{short_id}.txt"
            scratch_path.write_text(output, encoding="utf-8")

            # Build summary: keep first 500 chars for immediate context
            line_count = output.count("\n") + 1
            char_count = len(output)
            preview = output[:500]
            if len(output) > 500:
                preview += "\n..."

            # Dynamic truncation hint based on agent capabilities
            if has_subagent_tool:
                hint = (
                    "Delegate to an explore subagent to process the full output via "
                    "search/read_file, or use read_file with offset/max_lines to page through it."
                )
            else:
                hint = (
                    "Use read_file with offset/max_lines to page through the full output."
                )

            return (
                f"{preview}\n\n"
                f"[Output offloaded: {line_count} lines, {char_count} chars → "
                f"`{scratch_path}`]\n"
                f"{hint}"
            )
        except OSError:
            logger.debug("Failed to offload tool output to scratch file", exc_info=True)
            return output

    def _persist_step(
        self,
        ctx: IterationContext,
        tool_calls: list,
        results: Dict[str, dict],
        content: str,
        raw_content: Optional[str],
    ):
        """Persist the step to session manager and record learnings."""
        tool_call_objects = []

        for tc in tool_calls:
            tool_name = tc["function"]["name"]
            _debug_log(f"[PERSIST] Processing tool call: {tool_name}")
            if tool_name == "task_complete":
                continue

            full_result = results.get(tc["id"], {})
            _debug_log(f"[PERSIST] full_result keys: {list(full_result.keys())}")
            tool_error = full_result.get("error") if not full_result.get("success", True) else None
            tool_result_str = (
                full_result.get("output", "") if full_result.get("success", True) else None
            )
            result_summary = summarize_tool_result(tool_name, tool_result_str, tool_error)
            _debug_log(
                f"[PERSIST] result_summary: {result_summary[:100] if result_summary else None}"
            )

            nested_calls = []
            if (
                tool_name == "spawn_subagent"
                and ctx.ui_callback
                and hasattr(ctx.ui_callback, "get_and_clear_nested_calls")
            ):
                nested_calls = ctx.ui_callback.get_and_clear_nested_calls()

            _debug_log("[PERSIST] Creating ToolCallModel")
            tool_call_objects.append(
                ToolCallModel(
                    id=tc["id"],
                    name=tool_name,
                    parameters=json.loads(tc["function"]["arguments"]),
                    result=full_result,
                    result_summary=result_summary,
                    error=tool_error,
                    approved=True,
                    nested_tool_calls=nested_calls,
                )
            )
            _debug_log("[PERSIST] ToolCallModel created")

            # Record artifact in compactor's artifact index
            self._record_artifact(tool_name, tc, full_result)

        if tool_call_objects or content:
            _debug_log(f"[PERSIST] Creating msg with {len(tool_call_objects)} tool calls")
            _debug_log(
                f"[PERSIST] content={content[:50] if content else None}, raw_content={raw_content[:50] if raw_content else None}"
            )
            metadata = {"raw_content": raw_content} if raw_content is not None else {}
            _debug_log("[PERSIST] About to create ChatMessage")
            try:
                assistant_msg = ChatMessage(
                    role=Role.ASSISTANT,
                    content=content or "",
                    metadata=metadata,
                    tool_calls=tool_call_objects,
                    # Include tracked iteration data for session persistence
                    thinking_trace=self._current_thinking_trace,
                    reasoning_content=self._current_reasoning_content,
                    token_usage=self._current_token_usage,
                )
                _debug_log("[PERSIST] ChatMessage created successfully")
            except Exception as e:
                _debug_log(f"[PERSIST] ChatMessage creation failed: {e}")
                raise

            _debug_log("[PERSIST] Calling add_message")
            self.session_manager.add_message(assistant_msg, self.config.auto_save_interval)

            _debug_log("[PERSIST] Clearing tracked values")
            # Clear tracked values after persistence
            self._current_thinking_trace = None
            self._current_reasoning_content = None
            self._current_token_usage = None

        _debug_log("[PERSIST] Completed")

        if tool_call_objects:
            outcome = "error" if any(tc.error for tc in tool_call_objects) else "success"
            self._tool_executor.record_tool_learnings(
                ctx.query, tool_call_objects, outcome, ctx.agent
            )

    def _record_artifact(
        self,
        tool_name: str,
        tool_call: dict,
        full_result: dict,
    ) -> None:
        """Record file operations in the compactor's artifact index."""
        if self._compactor is None:
            return

        try:
            args = json.loads(tool_call["function"]["arguments"])
        except (json.JSONDecodeError, KeyError):
            return

        file_path = args.get("file_path", "")
        success = full_result.get("success", False)
        if not success or not file_path:
            return

        if tool_name in ("read_file", "Read"):
            output = full_result.get("output", "")
            line_count = output.count("\n") + 1 if output else 0
            self._compactor.artifact_index.record(
                file_path, "read", f"{line_count} lines"
            )
        elif tool_name in ("write_file", "Write"):
            content = args.get("content", "")
            line_count = content.count("\n") + 1 if content else 0
            self._compactor.artifact_index.record(
                file_path, "created", f"{line_count} lines"
            )
        elif tool_name in ("edit_file", "Edit"):
            added = full_result.get("lines_added", 0)
            removed = full_result.get("lines_removed", 0)
            self._compactor.artifact_index.record(
                file_path, "modified", f"+{added}/-{removed}"
            )

    def _display_message(self, message: str, ui_callback, dim: bool = False):
        """Display a message via UI callback or console."""
        if not message:
            return

        if ui_callback and hasattr(ui_callback, "on_assistant_message"):
            ui_callback.on_assistant_message(message)
        else:
            style = "[dim]" if dim else ""
            end_style = "[/dim]" if dim else ""
            self.console.print(f"\n{style}{message}{end_style}")

    def _add_assistant_message(self, content: str, raw_content: Optional[str]):
        """Add assistant message to session."""
        metadata = {"raw_content": raw_content} if raw_content is not None else {}
        assistant_msg = ChatMessage(
            role=Role.ASSISTANT,
            content=content,
            metadata=metadata,
            # Include tracked iteration data for session persistence
            thinking_trace=self._current_thinking_trace,
            reasoning_content=self._current_reasoning_content,
            token_usage=self._current_token_usage,
        )
        self.session_manager.add_message(assistant_msg, self.config.auto_save_interval)

        # Clear tracked values after persistence
        self._current_thinking_trace = None
        self._current_reasoning_content = None
        self._current_token_usage = None

    def _should_nudge_agent(self, consecutive_reads: int, messages: list) -> bool:
        """Check if agent should be nudged to conclude."""
        if consecutive_reads >= 5:
            # Silently nudge the agent
            messages.append(
                {
                    "role": "user",
                    "content": get_reminder("consecutive_reads_nudge"),
                }
            )
            return True
        return False

    def _execute_tool_call(
        self,
        tool_call: dict,
        tool_registry,
        approval_manager,
        undo_manager,
        ui_callback=None,
    ) -> dict:
        """Execute a single tool call."""

        tool_name = tool_call["function"]["name"]
        tool_args = json.loads(tool_call["function"]["arguments"])
        tool_call_id = tool_call["id"]
        tool_call_display = format_tool_call(tool_name, tool_args)

        tool_monitor = TaskMonitor()
        if self._active_interrupt_token:
            tool_monitor.set_interrupt_token(self._active_interrupt_token)
        tool_monitor.start(tool_call_display, initial_tokens=0)

        if self._tool_executor:
            self._tool_executor._current_task_monitor = tool_monitor

        progress = TaskProgressDisplay(self.console, tool_monitor)
        progress.start()

        try:
            result = tool_registry.execute_tool(
                tool_name,
                tool_args,
                mode_manager=self._tool_executor.mode_manager,
                approval_manager=approval_manager,
                undo_manager=undo_manager,
                task_monitor=tool_monitor,
                session_manager=self.session_manager,
                ui_callback=ui_callback,
                tool_call_id=tool_call_id,  # Pass for subagent parent tracking
            )
            return result
        finally:
            progress.stop()
            if self._tool_executor:
                self._tool_executor._current_task_monitor = None
