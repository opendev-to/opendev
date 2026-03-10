"""Query processing for REPL."""

import json
from typing import TYPE_CHECKING, Iterable

from opendev.core.context_engineering.memory import (
    Reflector,
    Curator,
)
from opendev.core.agents.prompts import get_reminder
from opendev.repl.react_executor import ReactExecutor


if TYPE_CHECKING:
    from rich.console import Console
    from opendev.core.runtime import ModeManager
    from opendev.core.context_engineering.history import SessionManager
    from opendev.core.runtime.approval import ApprovalManager
    from opendev.core.context_engineering.history import UndoManager
    from opendev.core.context_engineering.tools.implementations import FileOperations
    from opendev.ui_textual.formatters_internal.output_formatter import OutputFormatter
    from opendev.ui_textual.components import StatusLine
    from opendev.models.config import Config
    from opendev.core.runtime import ConfigManager
    from opendev.models.message import ToolCall


class QueryProcessor:
    """Processes user queries using ReAct pattern.

    This class orchestrates query processing by coordinating:
    - Query enhancement (@ file references)
    - Message preparation with playbook context
    - LLM calls with progress display
    - Tool execution with approval/undo
    - ACE learning from tool execution results

    Note:
        THINKING_VERBS constant has been consolidated into LLMCaller.
        This class is being incrementally refactored to compose
        specialized components (LLMCaller, QueryEnhancer, ToolExecutor).
    """

    REFLECTION_WINDOW_SIZE = 10
    MAX_PLAYBOOK_STRATEGIES = 30
    PLAYBOOK_DEBUG_PATH = "/tmp/swecli_debug/playbook_evolution.log"

    def __init__(
        self,
        console: "Console",
        session_manager: "SessionManager",
        config: "Config",
        config_manager: "ConfigManager",
        mode_manager: "ModeManager",
        file_ops: "FileOperations",
        output_formatter: "OutputFormatter",
        status_line: "StatusLine",
        message_printer_callback,
    ):
        """Initialize query processor.

        Args:
            console: Rich console for output
            session_manager: Session manager for message tracking
            config: Configuration
            config_manager: Configuration manager
            mode_manager: Mode manager for current mode
            file_ops: File operations for query enhancement
            output_formatter: Output formatter for tool results
            status_line: Status line renderer
            message_printer_callback: Callback to print markdown messages
        """
        self.console = console
        self.session_manager = session_manager
        self.config = config
        self.config_manager = config_manager
        self.mode_manager = mode_manager
        self.file_ops = file_ops
        self.output_formatter = output_formatter
        self.status_line = status_line
        self._print_markdown_message = message_printer_callback

        # UI state trackers
        self._last_latency_ms = None
        self._last_operation_summary = "—"
        self._last_error = None
        self._notification_center = None

        # Interrupt support - track current task monitor
        self._current_task_monitor: Optional[Any] = None

        # Hooks manager (set via set_hook_manager)
        self._hook_manager = None

        # ACE Components - Initialize on first use (lazy loading)
        self._ace_reflector: Optional[Reflector] = None
        self._ace_curator: Optional[Curator] = None
        self._last_agent_response: Optional[AgentResponse] = None
        self._execution_count = 0

        # Topic detection - lazy initialized
        self._topic_detector: Optional[Any] = None

        # Composed components (SOLID refactoring)
        from opendev.repl.query_enhancer import QueryEnhancer

        self._query_enhancer = QueryEnhancer(
            file_ops=file_ops,
            session_manager=session_manager,
            config=config,
            console=console,
        )

        # Context Picker - unified context engineering
        from opendev.core.context_engineering.context_picker import ContextPicker

        self._context_picker = ContextPicker(
            session_manager=session_manager,
            config=config,
            file_ops=file_ops,
            console=console,
        )

        from opendev.repl.llm_caller import LLMCaller

        self._llm_caller = LLMCaller(console=console)

        from opendev.repl.tool_executor import ToolExecutor

        self._tool_executor = ToolExecutor(
            console,
            output_formatter,
            mode_manager,
            session_manager,
            self._ace_reflector,
            self._ace_curator,
        )
        from opendev.core.runtime.cost_tracker import CostTracker

        self._cost_tracker = CostTracker()
        self._react_executor = ReactExecutor(
            session_manager=session_manager,
            config=config,
            mode_manager=self._tool_executor.mode_manager,
            console=console,
            llm_caller=self._llm_caller,
            tool_executor=self._tool_executor,
            cost_tracker=self._cost_tracker,
        )

    def set_notification_center(self, notification_center):
        """Set notification center for status line rendering.

        Args:
            notification_center: Notification center instance
        """
        self._notification_center = notification_center

    def set_hook_manager(self, hook_manager):
        """Set the hook manager for lifecycle hooks.

        Args:
            hook_manager: HookManager instance
        """
        self._hook_manager = hook_manager
        # Also wire into the ReactExecutor
        if self._react_executor:
            self._react_executor.set_hook_manager(hook_manager)

    def request_interrupt(self) -> bool:
        """Request interrupt of currently running task (LLM call or tool execution).

        Returns:
            True if interrupt was requested, False if no task is running
        """
        from opendev.ui_textual.debug_logger import debug_log

        debug_log("QueryProcessor", "request_interrupt called")
        debug_log(
            "QueryProcessor",
            f"self._llm_caller id={id(self._llm_caller) if hasattr(self, '_llm_caller') else 'N/A'}",
        )

        interrupted = False

        # Check our own task monitor (for direct _call_llm_with_progress usage)
        debug_log("QueryProcessor", f"_current_task_monitor={self._current_task_monitor}")
        if self._current_task_monitor is not None:
            self._current_task_monitor.request_interrupt()
            interrupted = True
            debug_log("QueryProcessor", "Interrupted via _current_task_monitor")

        # Also check llm_caller's monitor (for react_executor flow)
        has_llm_caller = hasattr(self, "_llm_caller") and self._llm_caller is not None
        debug_log("QueryProcessor", f"has _llm_caller={has_llm_caller}")
        if has_llm_caller:
            llm_monitor = getattr(self._llm_caller, "_current_task_monitor", None)
            debug_log(
                "QueryProcessor",
                f"_llm_caller id={id(self._llm_caller)}, _current_task_monitor={llm_monitor}",
            )
            if self._llm_caller.request_interrupt():
                interrupted = True
                debug_log("QueryProcessor", "Interrupted via _llm_caller")

        # Also check tool_executor's monitor (for parallel tool execution)
        has_tool_executor = hasattr(self, "_tool_executor") and self._tool_executor is not None
        debug_log("QueryProcessor", f"has _tool_executor={has_tool_executor}")
        if has_tool_executor:
            tool_monitor = getattr(self._tool_executor, "_current_task_monitor", None)
            debug_log("QueryProcessor", f"_tool_executor._current_task_monitor={tool_monitor}")
            if self._tool_executor.request_interrupt():
                interrupted = True
                debug_log("QueryProcessor", "Interrupted via _tool_executor")

        # Also check react_executor's monitor (for thinking phase)
        has_react_executor = hasattr(self, "_react_executor") and self._react_executor is not None
        debug_log("QueryProcessor", f"has _react_executor={has_react_executor}")
        if has_react_executor:
            react_monitor = getattr(self._react_executor, "_current_task_monitor", None)
            debug_log("QueryProcessor", f"_react_executor._current_task_monitor={react_monitor}")
            if self._react_executor.request_interrupt():
                interrupted = True
                debug_log("QueryProcessor", "Interrupted via _react_executor")

        # Also try centralized interrupt token on react_executor
        if has_react_executor:
            active_token = getattr(self._react_executor, "_active_interrupt_token", None)
            if active_token is not None:
                active_token.request()
                interrupted = True
                debug_log("QueryProcessor", "Interrupted via _active_interrupt_token")

        debug_log("QueryProcessor", f"Final result: interrupted={interrupted}")
        return interrupted

    def _init_ace_components(self, agent):
        """Initialize ACE components lazily on first use.

        Safe to call multiple times - idempotent and handles errors gracefully.

        Args:
            agent: Agent with LLM client
        """
        if self._ace_reflector is None:
            try:
                # Initialize ACE roles with native implementation
                # The native components use swecli's LLM client directly
                self._ace_reflector = Reflector(agent.client)
                self._ace_curator = Curator(agent.client)
            except Exception:  # pragma: no cover
                # ACE initialization failed - leave components as None
                # record_tool_learnings will safely handle None components
                pass

    def enhance_query(self, query: str) -> tuple[str, list[dict]]:
        """Enhance query with file contents if referenced.

        Delegates to QueryEnhancer.

        Args:
            query: Original query

        Returns:
            Tuple of (enhanced_query, image_blocks):
            - enhanced_query: Query with @ stripped and file contents appended
            - image_blocks: List of multimodal image blocks for vision API
        """
        return self._query_enhancer.enhance_query(query)

    def pick_context(self, query: str, agent, *, trace: bool = False):
        """Pick and assemble all context for an LLM call.

        This is the unified entry point for context engineering.
        Uses ContextPicker to coordinate:
        - File reference injection (@mentions)
        - Conversation history
        - Playbook strategies
        - System prompt

        Args:
            query: User's query (may contain @file references)
            agent: Agent with system prompt
            trace: If True, log context selection details

        Returns:
            AssembledContext with messages and context pieces
        """
        return self._context_picker.pick_context(query, agent, trace=trace)

    def _prepare_messages(
        self,
        query: str,
        enhanced_query: str,
        agent,
        image_blocks: list[dict] | None = None,
    ) -> list:
        """Prepare messages for LLM API call.

        Delegates to QueryEnhancer.

        Args:
            query: Original query
            enhanced_query: Query with file contents or @ references processed
            agent: Agent with system prompt
            image_blocks: Optional list of multimodal image blocks for vision API

        Returns:
            List of API messages
        """
        return self._query_enhancer.prepare_messages(
            query, enhanced_query, agent, image_blocks=image_blocks
        )

    def _call_llm_with_progress(self, agent, messages, task_monitor) -> tuple:
        """Call LLM with progress display.

        Delegates to LLMCaller for improved error handling and logging.

        Args:
            agent: Agent to use
            messages: Message history
            task_monitor: Task monitor for tracking

        Returns:
            Tuple of (response, latency_ms)
        """
        # Track current monitor for interrupt support
        self._current_task_monitor = task_monitor
        try:
            return self._llm_caller.call_llm_with_progress(agent, messages, task_monitor)
        finally:
            self._current_task_monitor = None

    def _record_tool_learnings(
        self,
        query: str,
        tool_call_objects: Iterable["ToolCall"],
        outcome: str,
        agent,
    ) -> None:
        """Use ACE Reflector and Curator to evolve playbook from tool execution.

        Delegates to ToolExecutor. ToolExecutor.record_tool_learnings has
        proper error handling, so we don't need additional try-except here.

        Args:
            query: User's query
            tool_call_objects: Tool calls that were executed
            outcome: "success", "error", or "partial"
            agent: Agent with LLM client (for ACE initialization)
        """
        # Initialize ACE components (safe - handles errors internally)
        self._init_ace_components(agent)

        # Set ACE components on ToolExecutor (may be None if init failed)
        self._tool_executor.set_ace_components(self._ace_reflector, self._ace_curator)

        # Set last agent response
        if self._last_agent_response:
            self._tool_executor.set_last_agent_response(str(self._last_agent_response))

        # Delegate to ToolExecutor (has internal error handling)
        self._tool_executor.record_tool_learnings(query, tool_call_objects, outcome, agent)

    def _format_tool_feedback(self, tool_calls: list, outcome: str) -> str:
        """Format tool execution results as feedback string for ACE Reflector.

        Delegates to ToolExecutor.

        Args:
            tool_calls: List of ToolCall objects with results
            outcome: "success", "error", or "partial"

        Returns:
            Formatted feedback string
        """
        return self._tool_executor._format_tool_feedback(tool_calls, outcome)

    def _render_status_line(self):
        """Render the status line with current context."""
        total_tokens = (
            self.session_manager.current_session.total_tokens()
            if self.session_manager.current_session
            else 0
        )
        self.status_line.render(
            model=self.config.model,
            working_dir=self.config_manager.working_dir,
            tokens_used=total_tokens,
            tokens_limit=self.config.max_context_tokens,
            mode=self.mode_manager.current_mode.value.upper(),
            latency_ms=self._last_latency_ms,
            key_hints=[
                ("Esc S", "Status detail"),
                ("Esc C", "Context"),
                ("Esc N", "Notifications"),
                ("/help", "Commands"),
            ],
            notifications=(
                [note.summary() for note in self._notification_center.latest(2)]
                if self._notification_center and self._notification_center.has_items()
                else None
            ),
        )

    def _trigger_topic_detection(self, query: str) -> None:
        """Fire-and-forget LLM topic detection for dynamic session titling."""
        from opendev.models.message import Role

        session = self.session_manager.get_current_session()
        if not session:
            return

        if self._topic_detector is None:
            try:
                from opendev.core.context_engineering.history import TopicDetector

                self._topic_detector = TopicDetector(self.config)
            except Exception:
                return

        # Extract plain user/assistant text messages only
        plain_messages = [
            {"role": m.role.value, "content": m.content}
            for m in session.messages
            if m.role in (Role.USER, Role.ASSISTANT) and m.content.strip()
        ]
        self._topic_detector.detect(self.session_manager, session.id, plain_messages)

    def process_query(
        self,
        query: str,
        agent,
        tool_registry,
        approval_manager: "ApprovalManager",
        undo_manager: "UndoManager",
        plan_requested: bool = False,
    ) -> tuple:
        """Process a user query with AI using ReAct pattern.

        Args:
            query: User query
            agent: Agent to use for LLM calls
            tool_registry: Tool registry for executing tools
            approval_manager: Approval manager for user confirmations
            undo_manager: Undo manager for operation history
            plan_requested: If True, inject plan mode request message

        Returns:
            Tuple of (last_operation_summary, last_error, last_latency_ms)
        """
        from opendev.models.message import ChatMessage, Role

        # Add user message to session
        user_msg = ChatMessage(role=Role.USER, content=query)
        self.session_manager.add_message(user_msg, self.config.auto_save_interval)
        self._trigger_topic_detection(query)

        # Fire UserPromptSubmit hook
        if self._hook_manager:
            from opendev.core.hooks.models import HookEvent

            if self._hook_manager.has_hooks_for(HookEvent.USER_PROMPT_SUBMIT):
                outcome = self._hook_manager.run_hooks(
                    HookEvent.USER_PROMPT_SUBMIT,
                    event_data={"user_prompt": query},
                )
                if outcome.blocked:
                    return (f"Blocked: {outcome.block_reason}", outcome.block_reason, 0)

        # Enhance query with file contents (returns enhanced text + image blocks)
        enhanced_query, image_blocks = self.enhance_query(query)

        # Prepare messages for API (handles multimodal content if images present)
        messages = self._prepare_messages(query, enhanced_query, agent, image_blocks)

        # Inject plan mode request if user toggled Shift+Tab
        if plan_requested:
            plan_path = self.mode_manager.get_plan_file_path() or "~/.opendev/plans/plan.md"
            messages.append(
                {
                    "role": "user",
                    "content": f"<system-reminder>{get_reminder('plan_subagent_request', plan_file_path=plan_path)}</system-reminder>",
                }
            )

        # Delegate to ReactExecutor
        return self._react_executor.execute(
            query,
            messages,
            agent,
            tool_registry,
            approval_manager,
            undo_manager,
        )

    def process_query_with_callback(
        self,
        query: str,
        agent,
        tool_registry,
        approval_manager: "ApprovalManager",
        undo_manager: "UndoManager",
        ui_callback,
        plan_requested: bool = False,
    ) -> tuple:
        """Process a user query with AI using ReAct pattern with UI callback for real-time updates.

        Args:
            query: User query
            agent: Agent to use for LLM calls
            tool_registry: Tool registry for executing tools
            approval_manager: Approval manager for user confirmations
            undo_manager: Undo manager for operation history
            ui_callback: UI callback for real-time tool display
            plan_requested: If True, inject plan mode request message

        Returns:
            Tuple of (last_operation_summary, last_error, last_latency_ms)
        """
        from opendev.models.message import ChatMessage, Role

        # Add user message to session
        user_msg = ChatMessage(role=Role.USER, content=query)
        self.session_manager.add_message(user_msg, self.config.auto_save_interval)
        self._trigger_topic_detection(query)

        # Fire UserPromptSubmit hook
        if self._hook_manager:
            from opendev.core.hooks.models import HookEvent

            if self._hook_manager.has_hooks_for(HookEvent.USER_PROMPT_SUBMIT):
                outcome = self._hook_manager.run_hooks(
                    HookEvent.USER_PROMPT_SUBMIT,
                    event_data={"user_prompt": query},
                )
                if outcome.blocked:
                    return (f"Blocked: {outcome.block_reason}", outcome.block_reason, 0)

        # Enhance query with file contents (returns enhanced text + image blocks)
        enhanced_query, image_blocks = self.enhance_query(query)

        # Prepare messages for API (handles multimodal content if images present)
        messages = self._prepare_messages(query, enhanced_query, agent, image_blocks)

        # Inject plan mode request if user toggled Shift+Tab
        if plan_requested:
            plan_path = self.mode_manager.get_plan_file_path() or "~/.opendev/plans/plan.md"
            messages.append(
                {
                    "role": "user",
                    "content": f"<system-reminder>{get_reminder('plan_subagent_request', plan_file_path=plan_path)}</system-reminder>",
                }
            )

        # Delegate to ReactExecutor with ui_callback
        return self._react_executor.execute(
            query,
            messages,
            agent,
            tool_registry,
            approval_manager,
            undo_manager,
            ui_callback=ui_callback,
        )
