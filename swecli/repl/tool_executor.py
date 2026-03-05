"""Tool execution and learning from results."""

import json
import os
from datetime import datetime
from typing import Iterable, TYPE_CHECKING

if TYPE_CHECKING:
    from swecli.models.message import ToolCall


class ToolExecutor:
    """Handles tool execution with approval, undo, and learning."""

    def __init__(self, console, output_formatter, mode_manager, session_manager, ace_reflector=None, ace_curator=None):
        """Initialize tool executor.

        Args:
            console: Rich console for output
            output_formatter: Output formatter for tool results
            mode_manager: Mode manager for operations
            session_manager: Session manager
            ace_reflector: ACE Reflector component (optional, will be initialized if needed)
            ace_curator: ACE Curator component (optional, will be initialized if needed)
        """
        self.console = console
        self.output_formatter = output_formatter
        self.mode_manager = mode_manager
        self.session_manager = session_manager
        self._ace_reflector = ace_reflector
        self._ace_curator = ace_curator
        self._current_task_monitor = None
        self._last_operation_summary = None
        self._last_error = None
        self._last_agent_response = None
        self._execution_count = 0
        self.PLAYBOOK_DEBUG_PATH = "/tmp/swecli_debug/playbook_evolution.log"

    def set_ace_components(self, reflector, curator):
        """Set ACE components for learning.

        Args:
            reflector: ACE Reflector component
            curator: ACE Curator component
        """
        self._ace_reflector = reflector
        self._ace_curator = curator

    def set_last_agent_response(self, response: str):
        """Set the last agent response for learning.

        Args:
            response: Agent's response text
        """
        self._last_agent_response = response

    def execute_tool_call(self, tool_call: dict, tool_registry, approval_manager, undo_manager, tool_call_display: str = None) -> dict:
        """Execute a single tool call.

        Args:
            tool_call: Tool call specification
            tool_registry: Tool registry
            approval_manager: Approval manager
            undo_manager: Undo manager
            tool_call_display: Pre-formatted display string (optional, will format if not provided)

        Returns:
            Tool execution result
        """
        import time as _time
        from swecli.core.debug import get_debug_logger
        from swecli.core.runtime.monitoring import TaskMonitor
        from swecli.ui_textual.components.task_progress import TaskProgressDisplay
        from swecli.ui_textual.utils.tool_display import format_tool_call

        tool_name = tool_call["function"]["name"]
        tool_args = json.loads(tool_call["function"]["arguments"])

        # Format tool call display if not provided
        if tool_call_display is None:
            tool_call_display = format_tool_call(tool_name, tool_args)

        get_debug_logger().log(
            "tool_call_start", "tool", name=tool_name, params_preview=str(tool_args)[:200]
        )

        # Create task monitor for interrupt support
        tool_monitor = TaskMonitor()
        tool_monitor.start(tool_call_display, initial_tokens=0)

        # Track current monitor for interrupt support
        self._current_task_monitor = tool_monitor

        # Start progress display (spinner) - don't print the tool call line again
        # It was already printed by the caller in process_query()
        tool_progress = TaskProgressDisplay(self.console, tool_monitor)
        tool_progress.start()

        tool_start = _time.monotonic()
        try:
            # Execute tool with interrupt support
            result = tool_registry.execute_tool(
                tool_name,
                tool_args,
                mode_manager=self.mode_manager,
                approval_manager=approval_manager,
                undo_manager=undo_manager,
                task_monitor=tool_monitor,
                session_manager=self.session_manager,
            )

            tool_duration_ms = int((_time.monotonic() - tool_start) * 1000)

            # Update state
            self._last_operation_summary = tool_call_display
            if result.get("success"):
                self._last_error = None
            else:
                self._last_error = result.get("error", "Tool execution failed")

            # Stop progress if it was started
            if tool_progress:
                tool_progress.stop()

            # Display result
            panel = self.output_formatter.format_tool_result(tool_name, tool_args, result)
            self.console.print(panel)

            get_debug_logger().log(
                "tool_call_end",
                "tool",
                name=tool_name,
                duration_ms=tool_duration_ms,
                success=result.get("success", False),
                result_preview=(result.get("output") or result.get("error") or "")[:200],
            )

            return result
        except Exception as exc:
            import traceback

            get_debug_logger().log(
                "tool_call_error",
                "tool",
                name=tool_name,
                error=str(exc),
                traceback=traceback.format_exc(),
            )
            raise
        finally:
            # Clear current monitor
            self._current_task_monitor = None

    def request_interrupt(self) -> bool:
        """Request interrupt of currently running tool execution.

        Returns:
            True if interrupt was requested, False if no task is running
        """
        if self._current_task_monitor is not None:
            self._current_task_monitor.request_interrupt()
            return True
        return False

    def record_tool_learnings(
        self,
        query: str,
        tool_call_objects: Iterable["ToolCall"],
        outcome: str,
        agent,
    ) -> None:
        """Use ACE Reflector and Curator to evolve playbook from tool execution.

        This implements the full ACE workflow:
        1. Reflector analyzes what happened (LLM-powered)
        2. Curator decides playbook changes (delta operations)
        3. Apply deltas to evolve playbook

        Args:
            query: User's query
            tool_call_objects: Tool calls that were executed
            outcome: "success", "error", or "partial"
            agent: Agent with LLM client (for ACE initialization)
        """
        session = self.session_manager.current_session
        if not session:
            return

        tool_calls = list(tool_call_objects)
        if not tool_calls:
            return

        # Skip if no agent response (ACE workflow needs it)
        if not self._last_agent_response:
            return

        try:
            # Ensure ACE components are available
            if not self._ace_reflector or not self._ace_curator:
                return

            playbook = session.get_playbook()

            # Format tool feedback for reflector
            feedback = self._format_tool_feedback(tool_calls, outcome)

            # STEP 1: Reflect on execution using ACE Reflector
            reflection = self._ace_reflector.reflect(
                question=query,
                agent_response=self._last_agent_response,
                playbook=playbook,
                ground_truth=None,
                feedback=feedback
            )

            # STEP 2: Apply bullet tags from reflection
            for bullet_tag in reflection.bullet_tags:
                try:
                    playbook.tag_bullet(bullet_tag.id, bullet_tag.tag)
                except (ValueError, KeyError):
                    continue

            # STEP 3: Curate playbook updates using ACE Curator
            self._execution_count += 1
            curator_output = self._ace_curator.curate(
                reflection=reflection,
                playbook=playbook,
                question_context=query,
                progress=f"Query #{self._execution_count}"
            )

            # STEP 4: Apply delta operations
            bullets_before = len(playbook.bullets())
            playbook.apply_delta(curator_output.delta)
            bullets_after = len(playbook.bullets())

            # Save updated playbook
            session.update_playbook(playbook)

            # Debug logging
            if bullets_after != bullets_before or curator_output.delta.operations:
                debug_dir = os.path.dirname(self.PLAYBOOK_DEBUG_PATH)
                os.makedirs(debug_dir, exist_ok=True)
                with open(self.PLAYBOOK_DEBUG_PATH, "a", encoding="utf-8") as log:
                    timestamp = datetime.now().isoformat()
                    log.write(f"\n{'=' * 60}\n")
                    log.write(f"🧠 ACE PLAYBOOK EVOLUTION - {timestamp}\n")
                    log.write(f"{'=' * 60}\n")
                    log.write(f"Query: {query}\n")
                    log.write(f"Outcome: {outcome}\n")
                    log.write(f"Bullets: {bullets_before} -> {bullets_after}\n")
                    log.write(f"Delta Operations: {len(curator_output.delta.operations)}\n")
                    for op in curator_output.delta.operations:
                        log.write(f"  - {op.type}: {op.section} - {op.content[:80] if op.content else op.bullet_id}\n")
                    log.write(f"Reflection Key Insight: {reflection.key_insight}\n")
                    log.write(f"Curator Reasoning: {curator_output.delta.reasoning[:200]}\n")

        except Exception as e:  # pragma: no cover
            # Log error but don't break query processing
            import traceback
            debug_dir = os.path.dirname(self.PLAYBOOK_DEBUG_PATH)
            os.makedirs(debug_dir, exist_ok=True)
            with open(self.PLAYBOOK_DEBUG_PATH, "a", encoding="utf-8") as log:
                log.write(f"\n{'!' * 60}\n")
                log.write(f"❌ ACE ERROR: {str(e)}\n")
                log.write(traceback.format_exc())

    def _format_tool_feedback(self, tool_calls: list, outcome: str) -> str:
        """Format tool execution results as feedback string for ACE Reflector.

        Args:
            tool_calls: List of ToolCall objects with results
            outcome: "success", "error", or "partial"

        Returns:
            Formatted feedback string
        """
        lines = [f"Outcome: {outcome}"]
        lines.append(f"Tools executed: {len(tool_calls)}")

        if outcome == "success":
            lines.append("All tools completed successfully")
            # Add brief summary of what was done
            tool_names = [tc.name for tc in tool_calls]
            lines.append(f"Tools: {', '.join(tool_names)}")
        elif outcome == "error":
            # List errors
            errors = [f"{tc.name}: {tc.error}" for tc in tool_calls if tc.error]
            lines.append(f"Errors ({len(errors)}):")
            for error in errors[:3]:  # First 3 errors
                lines.append(f"  - {error[:200]}")
        else:  # partial
            successes = sum(1 for tc in tool_calls if not tc.error)
            lines.append(f"Partial success: {successes}/{len(tool_calls)} tools succeeded")

        return "\n".join(lines)
