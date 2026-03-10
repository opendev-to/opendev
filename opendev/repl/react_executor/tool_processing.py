"""Tool call processing, execution, and result handling for ReactExecutor."""

from __future__ import annotations

import json
import logging
from concurrent.futures import as_completed
from pathlib import Path
from typing import TYPE_CHECKING, Dict, Optional

if TYPE_CHECKING:
    pass

from opendev.core.runtime.monitoring import TaskMonitor
from opendev.ui_textual.utils.tool_display import format_tool_call

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


class ToolProcessingMixin:
    """Mixin providing tool call processing, execution, and result handling.

    Expects the host class to provide:
        - self._active_interrupt_token
        - self._tool_executor
        - self._llm_caller
        - self._cost_tracker
        - self._compactor
        - self._snapshot_manager
        - self._parallel_executor
        - self._injection_queue
        - self._last_operation_summary
        - self.session_manager
        - self.config
        - self.console
        - self.READ_OPERATIONS
        - self.PARALLELIZABLE_TOOLS
        - self.MAX_NUDGE_ATTEMPTS
        - self.MAX_TODO_NUDGES
        - self.OFFLOAD_THRESHOLD
    """

    def _process_tool_calls(self, ctx, tool_calls: list, content: str, raw_content: Optional[str]):
        """Process a list of tool calls."""
        from opendev.core.agents.prompts import get_reminder
        from opendev.core.agents.prompts.reminders import append_nudge

        # Import LoopAction locally to avoid circular imports
        from opendev.repl.react_executor.executor import LoopAction

        # Reset no-tool-call counter
        ctx.consecutive_no_tool_calls = 0

        # Doom-loop detection: auto-recover with escalating nudges
        doom_warning = self._detect_doom_loop(tool_calls, ctx)
        if doom_warning:
            ctx.doom_loop_nudge_count += 1
            _debug_log(
                f"[DOOM_LOOP] nudge_count={ctx.doom_loop_nudge_count}: {doom_warning}"
            )

            if ctx.doom_loop_nudge_count >= 3:
                # Third strike — force stop
                if ctx.ui_callback and hasattr(ctx.ui_callback, "on_message"):
                    ctx.ui_callback.on_message(
                        f"Agent stuck in loop after multiple recovery attempts. "
                        f"Stopping. {doom_warning}"
                    )
                ctx.messages.append({
                    "role": "user",
                    "content": (
                        f"[SYSTEM] {doom_warning}\n"
                        "You have been stuck in a loop despite multiple warnings. "
                        "STOP and explain what you're trying to do."
                    ),
                })
                ctx.recent_tool_calls.clear()
                return LoopAction.BREAK

            if ctx.doom_loop_nudge_count == 2:
                # Second nudge — notify user silently (no blocking prompt)
                if ctx.ui_callback and hasattr(ctx.ui_callback, "on_message"):
                    ctx.ui_callback.on_message(f"Agent may be stuck: {doom_warning}")

            # Inject guidance (first and second nudge)
            ctx.messages.append({
                "role": "user",
                "content": (
                    f"[SYSTEM WARNING] {doom_warning}\n"
                    "You appear to be repeating the same action without progress. "
                    "Please try a completely different approach or explain what "
                    "you're trying to accomplish so we can find a better path."
                ),
            })
            ctx.recent_tool_calls.clear()
            return LoopAction.CONTINUE

        # Check for task completion FIRST (before displaying content)
        # This prevents duplicate bullets (one for content, one for summary)
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
                        append_nudge(ctx.messages, nudge)
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

        # Explore-first enforcement: block excessive exploration reads
        if not ctx.has_explored and all_reads and ctx.consecutive_reads >= 3:
            # Block execution — tell agent to use Code-Explorer instead
            for tc in tool_calls:
                append_nudge(
                    ctx.messages,
                    get_reminder("explore_delegate_nudge"),
                    role="tool",
                    tool_call_id=tc["id"],
                )
            ctx.consecutive_reads = 0
            ctx.skip_next_thinking = True
            return LoopAction.CONTINUE

        # Explore-first enforcement: block task subagent spawns until Code-Explorer has run
        EXPLORE_EXEMPT_SUBAGENTS = {"Code-Explorer", "ask-user"}
        if not ctx.has_explored:
            for tc in tool_calls:
                if tc["function"]["name"] == "spawn_subagent":
                    try:
                        args = json.loads(tc["function"]["arguments"])
                    except (json.JSONDecodeError, KeyError):
                        continue
                    subagent_type = args.get("subagent_type", "")
                    if subagent_type not in EXPLORE_EXEMPT_SUBAGENTS:
                        # Nudge the agent to explore first
                        append_nudge(
                            ctx.messages,
                            get_reminder("explore_first_nudge"),
                            role="tool",
                            tool_call_id=tc["id"],
                        )
                        # Fill remaining tool calls with synthetic results
                        for other_tc in tool_calls:
                            if other_tc["id"] != tc["id"]:
                                append_nudge(
                                    ctx.messages,
                                    "Blocked: explore first.",
                                    role="tool",
                                    tool_call_id=other_tc["id"],
                                )
                        ctx.skip_next_thinking = True
                        return LoopAction.CONTINUE

        # Mark explored when Code-Explorer is being spawned
        for tc in tool_calls:
            if tc["function"]["name"] == "spawn_subagent":
                try:
                    args = json.loads(tc["function"]["arguments"])
                except (json.JSONDecodeError, KeyError):
                    continue
                if args.get("subagent_type", "") == "Code-Explorer":
                    ctx.has_explored = True
                    break

        # Execute tools (parallel for spawn_subagent batches or read-only batches)
        spawn_calls = [tc for tc in tool_calls if tc["function"]["name"] == "spawn_subagent"]
        is_all_spawn_agents = len(spawn_calls) == len(tool_calls) and len(spawn_calls) > 1
        is_all_parallelizable = len(tool_calls) > 1 and all(
            tc["function"]["name"] in self.PARALLELIZABLE_TOOLS for tc in tool_calls
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
                if self._active_interrupt_token and self._active_interrupt_token.is_requested():
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
        from opendev.core.context_engineering.message_pair_validator import (
            MessagePairValidator,
        )

        tool_results_by_id = MessagePairValidator.validate_tool_results_complete(
            tool_calls, tool_results_by_id
        )

        # Snapshot tracking: capture state after write operations
        if self._snapshot_manager and not operation_cancelled:
            _write_tools = {"write_file", "edit_file", "run_command"}
            has_writes = any(tc["function"]["name"] in _write_tools for tc in tool_calls)
            if has_writes:
                self._snapshot_manager.track()

        # Check if agent has subagent capability (for dynamic truncation hints)
        _has_subagent = "spawn_subagent" in getattr(ctx.tool_registry, "_handlers", {})

        # Batch add all results after completion (maintains message order)
        for tool_call in tool_calls:
            self._add_tool_result_to_history(
                ctx.messages,
                tool_call,
                tool_results_by_id[tool_call["id"]],
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
                append_nudge(ctx.messages, get_reminder("all_todos_complete_nudge"))

        # Update context usage indicator after tool results are added
        if self._compactor:
            _ctx_logger.info("context_usage_after_tools: msg_count=%d", len(ctx.messages))
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
            append_nudge(ctx.messages, get_reminder("tool_denied_nudge"))

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
        self, tool_call: dict, ctx, suppress_separate_response: bool = False
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
        self._last_operation_summary = format_tool_call(tool_name, json.loads(args_str))

        # Notify UI result
        if ctx.ui_callback and hasattr(ctx.ui_callback, "on_tool_result"):
            ctx.ui_callback.on_tool_result(tool_name, args_str, result)

        # Handle subagent display (suppress in parallel mode for aggregation)
        separate_response = result.get("separate_response")
        if separate_response and not suppress_separate_response:
            self._display_message(separate_response, ctx.ui_callback)

        return result

    def _execute_tool_quietly(self, tool_call: dict, ctx) -> dict:
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
                mode_manager=self._mode_manager,
                approval_manager=ctx.approval_manager,
                undo_manager=ctx.undo_manager,
                task_monitor=tool_monitor,
                session_manager=self.session_manager,
                ui_callback=ctx.ui_callback,
                tool_call_id=tool_call_id,
            )
        except Exception as exc:
            if isinstance(exc, InterruptedError):
                raise
            _session_debug().log(
                "tool_call_error",
                "tool",
                name=tool_name,
                error=str(exc),
                traceback=traceback.format_exc(),
            )
            return {"success": False, "error": str(exc)}

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
        return result

    def _execute_tools_parallel(self, tool_calls: list, ctx) -> tuple[Dict[str, dict], bool]:
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
                except InterruptedError:
                    result = {
                        "success": False, "error": "Interrupted by user", "interrupted": True
                    }
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
                        success = result.get("success", True) if isinstance(result, dict) else True
                        if hasattr(ui_callback, "on_parallel_agent_complete"):
                            ui_callback.on_parallel_agent_complete(tool_call_id, success)

            # Notify UI that all parallel agents are done
            if ui_callback and hasattr(ui_callback, "on_parallel_agents_done"):
                ui_callback.on_parallel_agents_done()

        else:
            # --- Silent parallel: execute concurrently, display sequentially ---
            future_to_call = {
                executor.submit(self._execute_tool_quietly, tc, ctx): tc for tc in tool_calls
            }
            for future in as_completed(future_to_call):
                tool_call = future_to_call[future]
                try:
                    result = future.result()
                except InterruptedError:
                    result = {
                        "success": False, "error": "Interrupted by user", "interrupted": True
                    }
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
                self._last_operation_summary = format_tool_call(tool_name, json.loads(args_str))
                if ui_callback and hasattr(ui_callback, "on_tool_call"):
                    ui_callback.on_tool_call(tool_name, args_str)
                if ui_callback and hasattr(ui_callback, "on_tool_result"):
                    ui_callback.on_tool_result(tool_name, args_str, result)

        return tool_results_by_id, operation_cancelled

    def _add_tool_result_to_history(
        self,
        messages: list,
        tool_call: dict,
        result: dict,
        *,
        has_subagent_tool: bool = False,
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
            tool_result = f"Error in {tool_name}: {result.get('error', 'Tool execution failed')}"

        # Offload large outputs to scratch files
        tool_result = self._maybe_offload_output(
            tool_name,
            tool_call["id"],
            tool_result,
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
        self,
        tool_name: str,
        tool_call_id: str,
        output: str,
        *,
        has_subagent_tool: bool = False,
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
                hint = "Use read_file with offset/max_lines to page through the full output."

            return (
                f"{preview}\n\n"
                f"[Output offloaded: {line_count} lines, {char_count} chars → "
                f"`{scratch_path}`]\n"
                f"{hint}"
            )
        except OSError:
            logger.debug("Failed to offload tool output to scratch file", exc_info=True)
            return output

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

        progress = None
        if self.console:
            from opendev.ui_textual.components.task_progress import TaskProgressDisplay

            progress = TaskProgressDisplay(self.console, tool_monitor)
            progress.start()

        try:
            result = tool_registry.execute_tool(
                tool_name,
                tool_args,
                mode_manager=self._mode_manager,
                approval_manager=approval_manager,
                undo_manager=undo_manager,
                task_monitor=tool_monitor,
                session_manager=self.session_manager,
                ui_callback=ui_callback,
                tool_call_id=tool_call_id,  # Pass for subagent parent tracking
            )
            return result
        finally:
            if progress:
                progress.stop()
            if self._tool_executor:
                self._tool_executor._current_task_monitor = None
