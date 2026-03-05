"""Textual-based chat application for OpenDev - POC."""

import threading
from typing import Any, Callable, Mapping, Optional

from prompt_toolkit.completion import Completer

from rich.console import RenderableType
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Container, Vertical
from textual.widgets import Header, Rule, Static

from opendev.ui_textual.widgets import AnimatedWelcomePanel, ConversationLog, ProgressBar
from opendev.ui_textual.widgets.chat_text_area import ChatTextArea
from opendev.ui_textual.widgets.status_bar import StatusBar
from opendev.ui_textual.widgets.todo_panel import TodoPanel
from opendev.ui_textual.components import TipsManager
from opendev.ui_textual.controllers.approval_prompt_controller import ApprovalPromptController
from opendev.ui_textual.controllers.ask_user_prompt_controller import AskUserPromptController
from opendev.ui_textual.controllers.plan_approval_controller import PlanApprovalController
from opendev.ui_textual.controllers.autocomplete_popup_controller import AutocompletePopupController
from opendev.ui_textual.controllers.command_router import CommandRouter
from opendev.ui_textual.controllers.message_controller import MessageController
from opendev.ui_textual.controllers.model_picker_controller import ModelPickerController
from opendev.ui_textual.controllers.agent_creator_controller import AgentCreatorController
from opendev.ui_textual.controllers.skill_creator_controller import SkillCreatorController
from opendev.ui_textual.controllers.spinner_controller import SpinnerController
from opendev.ui_textual.managers.console_buffer_manager import ConsoleBufferManager
from opendev.ui_textual.managers.message_history import MessageHistory
from opendev.ui_textual.managers.tool_summary_manager import ToolSummaryManager
from opendev.ui_textual.managers.spinner_service import SpinnerService
from opendev.ui_textual.managers.interrupt_manager import InterruptManager, InterruptState

# Note: render_welcome_panel no longer used - replaced by AnimatedWelcomePanel widget


class SWECLIChatApp(App):
    """OpenDev Chat Application using Textual."""

    CSS_PATH = "styles/chat.tcss"

    # Disable mouse to avoid escape sequence issues and random character input
    ENABLE_MOUSE = False

    BINDINGS = [
        Binding("ctrl+c", "clear_or_quit", "", show=False, priority=True),
        Binding("ctrl+l", "clear_conversation", "", show=False),
        Binding("ctrl+t", "toggle_todo_panel", "Toggle Todos", show=False),
        Binding("ctrl+shift+t", "toggle_thinking", "Thinking", show=False),
        Binding("ctrl+o", "toggle_parallel_expansion", "Expand/Collapse", show=False),
        Binding("escape", "interrupt", "", show=False),
        Binding("pageup", "scroll_up", "Scroll Up", show=False),
        Binding("pagedown", "scroll_down", "Scroll Down", show=False),
        Binding("up", "scroll_up_line", "Scroll Up One Line", show=False),
        Binding("down", "scroll_down_line", "Scroll Down One Line", show=False),
        Binding("ctrl+up", "focus_conversation", "Focus Conversation", show=False),
        Binding("ctrl+down", "focus_input", "Focus Input", show=False),
        Binding("shift+tab", "cycle_mode", "Switch Mode"),
        Binding("ctrl+shift+a", "cycle_autonomy", "Autonomy", show=False),
    ]

    def __init__(
        self,
        on_message: Optional[Callable[[str], None]] = None,
        model: str = "claude-sonnet-4",
        model_slots: Mapping[str, tuple[str, str]] | None = None,
        on_cycle_mode: Optional[Callable[[], str]] = None,
        completer: Optional[Completer] = None,
        on_model_selected: Optional[Callable[[str, str, str], Any]] = None,
        get_model_config: Optional[Callable[[], Mapping[str, Any]]] = None,
        on_ready: Optional[Callable[[], None]] = None,
        on_interrupt: Optional[Callable[[], bool]] = None,
        working_dir: Optional[str] = None,
        todo_handler: Optional[Any] = None,
        is_resumed_session: bool = False,
        **kwargs,
    ):
        """Initialize chat application.

        Args:
            on_message: Callback for when user sends a message
            model: Model name to display in status bar
            model_slots: Mapping of model slots (normal/thinking/vision) to human-readable values
            completer: Autocomplete provider for slash commands and @ mentions
            on_model_selected: Callback invoked after a model is selected
            get_model_config: Callback returning current model configuration details
            on_ready: Callback invoked once the UI finishes its first layout pass
            on_interrupt: Callback for when user presses ESC to interrupt
            working_dir: Working directory path for repo display
            todo_handler: TodoHandler instance for managing todos
            is_resumed_session: If True, skip the welcome panel (resuming existing session)
        """
        # Set color system to inherit from terminal
        kwargs.setdefault("ansi_color", "auto")
        super().__init__(**kwargs)
        self.on_message = on_message
        self.on_cycle_mode = on_cycle_mode
        self.on_interrupt = on_interrupt
        self.model = model
        self.completer = completer
        self.model_slots = dict(model_slots or {})
        self.on_model_selected = on_model_selected
        self.get_model_config = get_model_config
        self._on_ready = on_ready
        self.working_dir = working_dir or ""
        self.todo_handler = todo_handler
        self.autocomplete_popup: Static | None = None
        self._autocomplete_controller: AutocompletePopupController | None = None
        self.footer = None  # Removed – no longer rendered
        self._is_processing = False
        self._last_assistant_lines: set[str] = set()
        self._last_rendered_assistant: str | None = None
        self._last_assistant_normalized: str | None = None
        self._buffer_console_output = False
        self._pending_assistant_normalized: str | None = None
        self._ui_thread: threading.Thread | None = None
        self._tips_manager = TipsManager()
        self._interrupt_manager = InterruptManager(self)
        self._model_picker: ModelPickerController = ModelPickerController(self, self._interrupt_manager)
        self._agent_creator: AgentCreatorController = AgentCreatorController(self, self._interrupt_manager)
        self._skill_creator: SkillCreatorController = SkillCreatorController(self, self._interrupt_manager)
        self._approval_controller = ApprovalPromptController(self, self._interrupt_manager)
        self._ask_user_controller = AskUserPromptController(self, self._interrupt_manager)
        self._plan_approval_controller = PlanApprovalController(self, self._interrupt_manager)
        # Register all controllers with the InterruptManager registry
        for ctrl in (
            self._model_picker,
            self._agent_creator,
            self._skill_creator,
            self._approval_controller,
            self._ask_user_controller,
            self._plan_approval_controller,
        ):
            self._interrupt_manager.register_controller(ctrl)
        self._spinner = SpinnerController(self, self._tips_manager, todo_handler=self.todo_handler)
        self.spinner_service = SpinnerService(self)
        self.spinner_service.set_tips_manager(self._tips_manager)
        self._console_buffer = ConsoleBufferManager(self)
        self._queued_console_renderables = self._console_buffer._queue
        self._tool_summary = ToolSummaryManager(self)
        self._command_router = CommandRouter(self)
        self._history = MessageHistory()
        self._message_controller = MessageController(self)
        self._exit_confirmation_mode = False
        self._selection_tip_timer: Any | None = None
        self._default_label = "› Type your message (Enter to send, Shift+Enter for new line):"
        self._thinking_visible = True  # Thinking mode visibility state (default ON)
        self._thinking_level = None  # ThinkingLevel enum, set when handler available
        self._progress_bar: ProgressBar | None = None
        self._welcome_panel: AnimatedWelcomePanel | None = None
        self._is_resumed_session = is_resumed_session
        self._welcome_visible = not is_resumed_session

    def compose(self) -> ComposeResult:
        """Create child widgets."""
        yield Header(show_clock=True)

        with Container(id="main-container"):
            # Animated welcome panel (only for new sessions, fades out on first message)
            if not self._is_resumed_session:
                yield AnimatedWelcomePanel(id="welcome-panel")

            # Conversation area
            yield ConversationLog(id="conversation")

            # Separator line between conversation and input
            yield Rule(line_style="solid")

            # Todo panel (persistent, toggleable with Ctrl+T)
            yield TodoPanel(todo_handler=self.todo_handler, id="todo-panel")

            # Input area
            with Vertical(id="input-container"):
                yield Static(
                    "› Type your message (Enter to send, Shift+Enter for new line):",
                    id="input-label",
                )
                yield ChatTextArea(
                    id="input",
                    placeholder="Type your message...",
                    soft_wrap=True,
                    completer=self.completer,
                )

            # Progress bar (above status bar, hidden by default)
            yield ProgressBar(id="progress-bar")

            # Status bar
            yield StatusBar(model=self.model, working_dir=self.working_dir, id="status-bar")


    def call_from_thread(
        self,
        callback: Callable[..., Any],
        *args: Any,
        **kwargs: Any,
    ) -> Any:
        """Thread-safe override: invoke directly if already on the UI thread.

        Textual's call_from_thread raises RuntimeError when called from the app
        thread. This override detects that case and calls the callback directly,
        which is semantically equivalent (we are already on the event loop).
        """
        if self._loop is None:
            return callback(*args, **kwargs)
        if self._thread_id == threading.get_ident():
            return callback(*args, **kwargs)
        return super().call_from_thread(callback, *args, **kwargs)

    def call_from_thread_nonblocking(
        self,
        callback: Callable[..., Any],
        *args: Any,
        **kwargs: Any,
    ) -> None:
        """Schedule a callback on the UI thread without waiting for the result.

        Safe from any thread. From UI thread: executes directly.
        From worker thread: schedules via loop.call_soon_threadsafe.
        """
        if self._loop is None or self._thread_id == threading.get_ident():
            callback(*args, **kwargs)
            return
        self._loop.call_soon_threadsafe(lambda: callback(*args, **kwargs))

    def on_mount(self) -> None:
        """Initialize the app on mount."""
        # Clear debug log for fresh session
        from opendev.ui_textual.debug_logger import clear_debug_log, debug_log
        clear_debug_log()
        debug_log("ChatApp", "App mounted - debug log cleared")

        self._ui_thread = threading.current_thread()

        # Get widgets
        self.conversation = self.query_one("#conversation", ConversationLog)
        self.input_field = self.query_one("#input", ChatTextArea)
        self.input_label = self.query_one("#input-label", Static)
        input_container = self.query_one("#input-container")
        self.status_bar = self.query_one("#status-bar", StatusBar)
        self._progress_bar = self.query_one("#progress-bar", ProgressBar)

        # Welcome panel is only present for new sessions
        try:
            self._welcome_panel = self.query_one("#welcome-panel", AnimatedWelcomePanel)
        except Exception:
            self._welcome_panel = None

        # Inject app reference into progress bar (for polling _is_processing)
        self._progress_bar.set_app(self)

        # Inject SpinnerService into TodoPanel (uses callback API for arrow spinner)
        try:
            todo_panel = self.query_one("#todo-panel", TodoPanel)
            todo_panel.set_spinner_service(self.spinner_service)
        except Exception:
            pass  # TodoPanel might not exist

        self.input_field.set_completer(self.completer)
        self.autocomplete_popup = Static("", id="autocomplete-popup")
        self.autocomplete_popup.can_focus = False
        self.autocomplete_popup.styles.display = "none"
        input_container.mount(self.autocomplete_popup)
        self._autocomplete_controller = AutocompletePopupController(self.autocomplete_popup)
        self.update_autocomplete([], None)

        # Focus input field
        self.input_field.focus()

        # Set titles based on whether we have real backend integration
        if self.on_message:
            self.title = "OpenDev Chat"
            self.sub_title = "AI-powered coding assistant"
        else:
            self.title = "OpenDev Chat (Textual POC)"
            self.sub_title = "Full-screen terminal interface"

        # Note: Welcome panel is now an animated widget mounted in compose()
        # It will be hidden on first user message via _hide_welcome_panel()

        if self._on_ready is not None:
            self.call_after_refresh(self._on_ready)

    def on_resize(self, event) -> None:
        """Manage welcome panel visibility on terminal resize."""
        if not self._welcome_visible or self._welcome_panel is None:
            return
        terminal_height = self.size.height
        if terminal_height < 15:
            self._welcome_panel.styles.display = "none"
        elif self._welcome_panel.styles.display == "none":
            self._welcome_panel.styles.display = "block"

    def update_model_slots(self, model_slots: Mapping[str, tuple[str, str]] | None) -> None:
        """Update model slot information."""
        self.model_slots = dict(model_slots or {})

    def update_primary_model(self, model: str) -> None:
        """Update the primary model label shown in the status bar."""
        self.model = model
        if hasattr(self, "status_bar"):
            self.status_bar.set_model_name(model)

    def update_autocomplete(
        self,
        entries: list[tuple[str, str]],
        selected_index: int | None = None,
    ) -> None:
        """Render autocomplete options directly beneath the input field."""

        controller = self._autocomplete_controller
        if controller is None:
            return

        if self._approval_controller.active:
            controller.reset()
            return

        controller.render(entries, selected_index)

    def _start_local_spinner(self, message: str | None = None) -> None:
        """Begin local spinner animation while backend processes."""

        if hasattr(self, "_console_buffer"):
            self._console_buffer.clear()
        self._spinner.start(message)

        # Note: Progress bar is shown by SpinnerService when tool spinners start

    def _stop_local_spinner(self) -> None:
        """Stop spinner animation and clear indicators."""

        self._spinner.stop()

        # Note: Progress bar is hidden by SpinnerService when all tool spinners complete
        # Don't hide here as tool execution may still be in progress

        if hasattr(self, "_console_buffer"):
            self._console_buffer.clear_assistant_history()
            self._console_buffer.flush()
        self.flush_console_buffer()

    def resume_reasoning_spinner(self) -> None:
        """Restart the thinking spinner after tool output while waiting for reply."""

        if not self._is_processing:
            return

        self._stop_local_spinner()
        self._start_local_spinner()

    def flush_console_buffer(self) -> None:
        """Flush queued console renderables after assistant message is recorded."""

        if hasattr(self, "_console_buffer"):
            self._console_buffer.flush()

    def start_console_buffer(self) -> None:
        """Begin buffering console output to avoid interleaving with spinner."""

        self._buffer_console_output = True
        if hasattr(self, "_console_buffer"):
            self._console_buffer.start()

    def stop_console_buffer(self) -> None:
        """Stop buffering console output and flush any pending items."""

        self._buffer_console_output = False
        if hasattr(self, "_console_buffer"):
            self._console_buffer.stop()

    @staticmethod
    def _normalize_assistant_line(line: str) -> str:
        return ConsoleBufferManager._normalize_line(line)

    @staticmethod
    def _normalize_paragraph(text: str) -> str:
        return ConsoleBufferManager._normalize_paragraph(text)

    def _should_suppress_renderable(self, renderable: RenderableType) -> bool:
        if hasattr(self, "_console_buffer"):
            return self._console_buffer.should_suppress(renderable)
        return False

    def render_console_output(self, renderable: RenderableType) -> None:
        """Render console output, buffering if spinner is active."""

        if hasattr(self, "_console_buffer"):
            self._console_buffer.enqueue_or_write(renderable)
        elif hasattr(self, "conversation"):
            self.conversation.write(renderable)

    def record_assistant_message(self, message: str) -> None:
        """Track assistant lines to suppress duplicate console echoes."""

        if hasattr(self, "_console_buffer"):
            self._console_buffer.record_assistant_message(message)
        if hasattr(self, "_tool_summary"):
            self._tool_summary.on_assistant_message(message)

    async def action_send_message(self) -> None:
        """Send message when user presses Enter."""
        # Hide welcome panel on first message
        if self._welcome_visible:
            self._hide_welcome_panel()
        await self._message_controller.submit(self.input_field.text)

    async def on_chat_text_area_submitted(self, event: ChatTextArea.Submitted) -> None:
        """Handle chat submissions from the custom text area."""
        # Hide welcome panel on first message
        if self._welcome_visible:
            self._hide_welcome_panel()
        await self._message_controller.submit(event.value)

    def add_assistant_message(self, message: str) -> None:
        """Proxy to conversation helper for compatibility with approval manager."""
        if hasattr(self, "conversation"):
            self.conversation.add_assistant_message(message)

    def add_system_message(self, message: str) -> None:
        """Proxy system message helper."""
        if hasattr(self, "conversation"):
            self.conversation.add_system_message(message)

    def _hide_welcome_panel(self) -> None:
        """Fade out and remove the welcome panel."""
        if not self._welcome_visible or self._welcome_panel is None:
            return

        self._welcome_visible = False

        def on_fade_complete() -> None:
            if self._welcome_panel is not None:
                self._welcome_panel.remove()
                self._welcome_panel = None

        self._welcome_panel.fade_out(on_fade_complete)

    async def handle_command(self, command: str) -> bool:
        """Handle slash commands.

        Returns True if the command was handled locally, False to allow higher-level
        handlers (e.g., REPL runner) to process it.
        """
        handled = await self._command_router.handle(command)
        if not handled and not self.on_message:
            self.conversation.add_command_result(
                ["Unknown command", "Type /help for available commands"],
                is_error=True,
            )
        return handled

    async def _start_model_picker(self) -> None:
        """Launch the in-conversation model picker flow."""
        await self._model_picker.start()

    def _model_picker_move(self, delta: int) -> None:
        self._model_picker.move(delta)

    def _model_picker_back(self) -> None:
        self._model_picker.back()

    def _model_picker_cancel(self) -> None:
        self._model_picker.cancel()

    async def _model_picker_confirm(self) -> None:
        await self._model_picker.confirm()

    async def _handle_model_picker_input(self, raw_value: str) -> bool:
        return await self._model_picker.handle_input(raw_value)

    # Agent Creator Wizard Methods
    def _agent_wizard_move(self, delta: int) -> None:
        """Navigate selection in agent wizard."""
        self._agent_creator.move(delta)

    def _agent_wizard_back(self) -> None:
        """Go back in agent wizard."""
        self._agent_creator.back()

    def _agent_wizard_cancel(self) -> None:
        """Cancel the agent wizard."""
        self._agent_creator.cancel()

    async def _agent_wizard_confirm(self) -> None:
        """Confirm current wizard step."""
        await self._agent_creator.confirm()

    # Skill Creator Wizard Methods
    def _skill_wizard_move(self, delta: int) -> None:
        """Navigate selection in skill wizard."""
        self._skill_creator.move(delta)

    def _skill_wizard_back(self) -> None:
        """Go back in skill wizard."""
        self._skill_creator.back()

    def _skill_wizard_cancel(self) -> None:
        """Cancel the skill wizard."""
        self._skill_creator.cancel()

    async def _skill_wizard_confirm(self) -> None:
        """Confirm current skill wizard step."""
        await self._skill_creator.confirm()

    async def show_approval_modal(self, command: str, working_dir: str) -> tuple[bool, str, str]:
        """Display an inline approval prompt inside the conversation log."""
        return await self._approval_controller.start(command, working_dir)

    def _render_approval_prompt(self) -> None:
        self._approval_controller.render()

    def _approval_move(self, delta: int) -> None:
        self._approval_controller.move(delta)

    def _approval_confirm(self) -> None:
        self._approval_controller.confirm()

    def _approval_cancel(self) -> None:
        self._approval_controller.cancel()

    # Plan Approval prompt methods
    def _plan_approval_move(self, delta: int) -> None:
        self._plan_approval_controller.move(delta)

    def _plan_approval_confirm(self) -> None:
        self._plan_approval_controller.confirm()

    def _plan_approval_cancel(self) -> None:
        self._plan_approval_controller.cancel()

    # Ask-User prompt methods
    def _ask_user_move(self, delta: int) -> None:
        """Move selection in ask-user prompt."""
        if self._ask_user_controller.active:
            self._ask_user_controller.move(delta)

    def _ask_user_toggle(self) -> None:
        """Toggle selection for multi-select questions (Space)."""
        if self._ask_user_controller.active:
            self._ask_user_controller.toggle_selection()

    def _ask_user_confirm(self) -> None:
        """Confirm selection in ask-user prompt."""
        if self._ask_user_controller.active:
            self._ask_user_controller.confirm()

    def _ask_user_cancel(self) -> None:
        """Cancel/skip ask-user prompt."""
        if self._ask_user_controller.active:
            self._ask_user_controller.cancel()

    def _ask_user_go_back(self) -> None:
        """Navigate to previous question."""
        if self._ask_user_controller.active:
            self._ask_user_controller.go_back()

    def _ask_user_go_forward(self) -> None:
        """Navigate to next question."""
        if self._ask_user_controller.active:
            self._ask_user_controller.go_forward()

    async def process_message(self, message: str) -> None:
        """Send the user message to the backend for processing."""
        await self._message_controller.process(message)

    def _set_processing_state(self, active: bool) -> None:
        """Update internal processing state and status bar indicator."""
        self._message_controller.set_processing_state(active)

    def record_tool_summary(
        self, tool_name: str, tool_args: dict[str, Any], result_lines: list[str]
    ) -> None:
        """Record a tool result summary for fallback assistant messaging."""
        if hasattr(self, "_tool_summary"):
            self._tool_summary.record_summary(tool_name, tool_args, result_lines)

    def _emit_tool_follow_up_if_needed(self) -> None:
        """Render a fallback assistant follow-up if tools finished without LLM wrap-up."""
        if hasattr(self, "_tool_summary"):
            self._tool_summary.emit_follow_up_if_needed()

    @property
    def _pending_tool_summaries(self) -> list[str]:
        if hasattr(self, "_tool_summary"):
            return self._tool_summary._pending
        return []

    @_pending_tool_summaries.setter
    def _pending_tool_summaries(self, value: list[str]) -> None:
        if hasattr(self, "_tool_summary"):
            self._tool_summary._pending = list(value)

    @property
    def _assistant_response_received(self) -> bool:
        if hasattr(self, "_tool_summary"):
            return self._tool_summary._assistant_response_received
        return False

    @_assistant_response_received.setter
    def _assistant_response_received(self, value: bool) -> None:
        if hasattr(self, "_tool_summary"):
            self._tool_summary._assistant_response_received = value

    @property
    def _saw_tool_result(self) -> bool:
        if hasattr(self, "_tool_summary"):
            return self._tool_summary._saw_tool_result
        return False

    @_saw_tool_result.setter
    def _saw_tool_result(self, value: bool) -> None:
        if hasattr(self, "_tool_summary"):
            self._tool_summary._saw_tool_result = value

    def notify_processing_complete(self) -> None:
        """Reset processing indicators once backend work completes."""
        self._message_controller.notify_processing_complete()

    def notify_processing_error(self, error: str) -> None:
        """Display an error message and reset processing indicators."""
        self._message_controller.notify_processing_error(error)

    def action_clear_conversation(self) -> None:
        """Clear the conversation (Ctrl+L)."""
        self.conversation.clear()
        self.conversation.add_system_message("Conversation cleared (Ctrl+L)")

    def action_interrupt(self) -> None:
        """Interrupt processing (ESC).

        When processing: modal-first priority — if a controller (ask_user/approval)
        is active, cancel it and consume ESC. Otherwise signal the interrupt token
        so the agent loop stops, with immediate UI feedback.
        When idle: delegate to InterruptManager for modals/exit confirmation.
        """
        # Autocomplete: highest priority fast path
        if self._interrupt_manager._has_autocomplete():
            self._interrupt_manager._dismiss_autocomplete()
            return

        if self._is_processing:
            # Modal-first: if a controller (ask_user/approval) is active, just cancel it
            if self._interrupt_manager._cancel_active_controller():
                return  # ESC consumed by modal — don't interrupt the run
            # No modal — interrupt the agent run
            self._interrupt_manager.request_run_interrupt()
            if self.on_interrupt:
                self.on_interrupt()
            self._show_interrupt_feedback()
            return

        # NOT PROCESSING: delegate to InterruptManager for modals/exit confirmation
        self._interrupt_manager.handle_interrupt()

    def _show_interrupt_feedback(self) -> None:
        """Show immediate interrupt feedback on the UI thread."""
        # Stop progress bar immediately by clearing processing flag
        self._is_processing = False
        # Collapse subagent display immediately (before spinner stop)
        if hasattr(self, "conversation") and hasattr(self.conversation, "interrupt_cleanup"):
            self.conversation.interrupt_cleanup()
        # Mark interrupt shown on ui_callback so Phase 2 callbacks are suppressed
        runner = getattr(self, "_runner", None)
        if runner and hasattr(runner, "_ui_callback"):
            runner._ui_callback.mark_interrupt_shown()
        # Stop all spinners immediately (renders red bullets)
        if hasattr(self, "spinner_service"):
            self.spinner_service.stop_all(immediate=True)
        # Stop local spinner
        if hasattr(self, "_stop_local_spinner"):
            self._stop_local_spinner()

    def action_clear_or_quit(self) -> None:
        """Clear input text or quit (Ctrl+C).

        First Ctrl+C: Clear input (if any) + show exit confirmation
        Second Ctrl+C: Quit the application
        """
        # If already in exit confirmation mode, quit
        if self._exit_confirmation_mode:
            self.exit()
            return

        # Clear input if there's text
        if self.input_field.text.strip():
            self.input_field.clear()

        # Enter exit confirmation mode
        self._exit_confirmation_mode = True
        self._interrupt_manager.enter_state(InterruptState.EXIT_CONFIRMATION)
        self.input_label.update("› Press Ctrl+C again to quit")

    def _cancel_exit_confirmation(self) -> None:
        """Cancel exit confirmation and restore normal state."""
        if self._exit_confirmation_mode:
            self._exit_confirmation_mode = False
            self._interrupt_manager.exit_state()
            self.input_label.update(
                "› Type your message (Enter to send, Shift+Enter for new line):"
            )

    def update_queue_indicator(self, queue_size: int) -> None:
        """Update input label to show queue status.

        Args:
            queue_size: Number of messages waiting in queue
        """
        # Don't override exit confirmation message
        if self._exit_confirmation_mode:
            return

        if queue_size > 0:
            msg = "message" if queue_size == 1 else "messages"
            self.input_label.update(f"› {queue_size} {msg} queued")
        else:
            self.input_label.update(
                "› Type your message (Enter to send, Shift+Enter for new line):"
            )

    def show_selection_tip(self) -> None:
        """Show temporary tip for text selection."""
        # Don't override exit confirmation message
        if self._exit_confirmation_mode:
            return

        # Cancel existing timer if any
        if self._selection_tip_timer:
            self._selection_tip_timer.stop()

        self.input_label.update("› Tip: Shift+drag (or Option+drag on Mac) to select text")
        self._selection_tip_timer = self.set_timer(4, self._revert_input_label)

    def _revert_input_label(self) -> None:
        """Revert input label to default or queue indicator."""
        if self._exit_confirmation_mode:
            return

        # Check if there are queued messages - show queue indicator instead
        runner = getattr(self, "_runner", None)
        queue_size = runner.get_queue_size() if runner else 0

        if queue_size > 0:
            msg = "message" if queue_size == 1 else "messages"
            self.input_label.update(f"› {queue_size} {msg} queued")
        else:
            self.input_label.update(self._default_label)

        self._selection_tip_timer = None

    def action_quit(self) -> None:
        """Quit the application (Ctrl+C)."""
        # Stop all spinners before exiting
        if hasattr(self, "spinner_service"):
            self.spinner_service.stop_all(immediate=True)
        self.exit()

    def action_scroll_up(self) -> None:
        """Scroll conversation up (Page Up)."""
        self.conversation.scroll_partial_page(direction=-1)

    def action_scroll_down(self) -> None:
        """Scroll conversation down (Page Down)."""
        self.conversation.scroll_partial_page(direction=1)

    def action_scroll_up_line(self) -> None:
        """Scroll conversation up one line (Up Arrow)."""
        # Only scroll if conversation has focus, otherwise let input handle it
        if self.conversation.has_focus:
            self.conversation.scroll_up()
        elif not self.input_field.has_focus:
            # If nothing focused, scroll conversation anyway
            self.conversation.scroll_up()

    def action_scroll_down_line(self) -> None:
        """Scroll conversation down one line (Down Arrow)."""
        # Only scroll if conversation has focus, otherwise let input handle it
        if self.conversation.has_focus:
            self.conversation.scroll_down()
        elif not self.input_field.has_focus:
            # If nothing focused, scroll conversation anyway
            self.conversation.scroll_down()

    def action_focus_conversation(self) -> None:
        """Focus the conversation area for scrolling (Ctrl+Up)."""
        self.conversation.focus()
        self.conversation.add_system_message(
            "📜 Conversation focused - use arrow keys or trackpad to scroll"
        )

    def action_focus_input(self) -> None:
        """Focus the input field for typing (Ctrl+Down)."""
        self.input_field.focus()

    def action_history_up(self) -> None:
        """Navigate backward through previously submitted messages."""

        if not hasattr(self, "_history"):
            return

        result = self._history.navigate_up(self.input_field.text)
        if result is None:
            return

        self.input_field.load_text(result)
        self.input_field.move_cursor_to_end()

    def action_history_down(self) -> None:
        """Navigate forward through history or restore unsent input."""

        if not hasattr(self, "_history"):
            return

        result = self._history.navigate_down()
        if result is None:
            return

        self.input_field.load_text(result)
        self.input_field.move_cursor_to_end()

    def action_cycle_mode(self) -> None:
        """Cycle between PLAN and NORMAL modes (Shift+Tab)."""

        if not self.on_cycle_mode:
            return

        try:
            new_mode = self.on_cycle_mode()
        except Exception:  # pragma: no cover - defensive
            return

        if not new_mode:
            return

        mode_label = new_mode.lower()
        self.status_bar.set_mode(mode_label)

    def action_cycle_autonomy(self) -> None:
        """Cycle through autonomy levels: Manual -> Semi-Auto -> Auto (Shift+A)."""
        if getattr(self, "_autonomy_locked", False):
            return
        levels = ["Manual", "Semi-Auto", "Auto"]
        current = self.status_bar.autonomy
        try:
            next_idx = (levels.index(current) + 1) % len(levels)
        except ValueError:
            next_idx = 0
        new_level = levels[next_idx]
        self.status_bar.set_autonomy(new_level)

        # Update approval manager if available
        if hasattr(self, "_approval_manager"):
            self._approval_manager.set_autonomy_level(new_level)

    def action_toggle_todo_panel(self) -> None:
        """Toggle todo panel visibility (Ctrl+T)."""
        try:
            panel = self.query_one("#todo-panel", TodoPanel)
            panel.toggle_expansion()
        except Exception:  # pragma: no cover - defensive
            pass

    def action_toggle_thinking(self) -> None:
        """Cycle thinking level (Ctrl+Shift+T).

        Cycles through: Off → Low → Medium → High → Off
        Controls thinking depth and whether self-critique is enabled.
        """
        # Cycle thinking level via handler
        if hasattr(self, "_thinking_handler") and self._thinking_handler:
            new_level = self._thinking_handler.cycle_level()
            self._thinking_level = new_level
            self._thinking_visible = new_level.is_enabled
        else:
            # Fallback if no handler - cycle through levels manually
            levels = ["Off", "Low", "Medium", "High"]
            current = getattr(self, "_thinking_level_str", "Medium")
            try:
                idx = levels.index(current)
                new_level_str = levels[(idx + 1) % len(levels)]
            except ValueError:
                new_level_str = "Medium"
            self._thinking_level_str = new_level_str
            self._thinking_visible = new_level_str != "Off"

        # Update status bar with new level
        try:
            status_bar = self.query_one("#status-bar", StatusBar)
            if hasattr(self, "_thinking_handler") and self._thinking_handler:
                status_bar.set_thinking_level(self._thinking_handler.level.value)
            else:
                status_bar.set_thinking_level(getattr(self, "_thinking_level_str", "Medium"))
        except Exception:  # pragma: no cover - defensive
            pass

    def action_toggle_parallel_expansion(self) -> None:
        """Toggle parallel agent or collapsible output expansion (Ctrl+O).

        Priority order:
        1. If there's an active parallel agent group, toggle its expansion
        2. If there's a completed single agent with tool records, toggle its expansion
        3. Otherwise, if there's collapsible output, toggle it
        """
        # First priority: parallel agent groups
        if hasattr(self.conversation, "has_active_parallel_group"):
            if self.conversation.has_active_parallel_group():
                self.conversation.toggle_parallel_expansion()
                return

        # Second priority: completed single agent tool expansion
        if hasattr(self.conversation, "has_expandable_single_agent"):
            if self.conversation.has_expandable_single_agent():
                self.conversation.toggle_single_agent_expansion()
                return

        # Third priority: collapsible output regions
        if hasattr(self.conversation, "has_collapsible_output"):
            if self.conversation.has_collapsible_output():
                self.conversation.toggle_output_expansion()


def create_chat_app(
    on_message: Optional[Callable[[str], None]] = None,
    model: str = "claude-sonnet-4",
    model_slots: Mapping[str, tuple[str, str]] | None = None,
    on_cycle_mode: Optional[Callable[[], str]] = None,
    completer: Optional[Completer] = None,
    on_model_selected: Optional[Callable[[str, str, str], Any]] = None,
    get_model_config: Optional[Callable[[], Mapping[str, Any]]] = None,
    on_ready: Optional[Callable[[], None]] = None,
    on_interrupt: Optional[Callable[[], bool]] = None,
    working_dir: Optional[str] = None,
    todo_handler: Optional[Any] = None,
    is_resumed_session: bool = False,
) -> SWECLIChatApp:
    """Create and return a new chat application instance.

    Args:
        on_message: Optional callback for message processing
        model: Model name to display in status bar
        model_slots: Mapping of model slots to formatted provider/model names
        completer: Autocomplete provider for @ mentions and slash commands
        on_model_selected: Callback invoked after a model is selected
        get_model_config: Callback returning current model configuration details
        on_ready: Callback invoked once the UI completes its first render pass
        on_interrupt: Callback for when user presses ESC to interrupt
        working_dir: Working directory path for repo display
        todo_handler: TodoHandler instance for managing todos
        is_resumed_session: If True, skip the welcome panel (resuming existing session)

    Returns:
        Configured SWECLIChatApp instance
    """
    return SWECLIChatApp(
        on_message=on_message,
        model=model,
        model_slots=model_slots,
        on_cycle_mode=on_cycle_mode,
        completer=completer,
        on_model_selected=on_model_selected,
        get_model_config=get_model_config,
        on_ready=on_ready,
        on_interrupt=on_interrupt,
        working_dir=working_dir,
        todo_handler=todo_handler,
        is_resumed_session=is_resumed_session,
    )


if __name__ == "__main__":
    # Run standalone for testing
    def handle_message(text: str):
        # Callback for external message handling
        # Don't print here - it will interfere with the UI!
        pass

    app = create_chat_app(on_message=handle_message)
    # Run in application mode (full screen with alternate screen buffer)
    # This is the default behavior when inline is not specified
    app.run()
