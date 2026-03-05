"""Primary agent implementation for interactive sessions."""

from __future__ import annotations

import json
import logging
import queue as queue_mod
from typing import Any, Optional

from swecli.core.base.abstract import BaseAgent
from swecli.core.agents.components import (
    ResponseCleaner,
    SystemPromptBuilder,
    ThinkingPromptBuilder,
    ToolSchemaBuilder,
    create_http_client,
    create_http_client_for_provider,
)
from swecli.core.agents.prompts import get_reminder
from swecli.models.config import AppConfig
from swecli.core.utils.sound import play_finish_sound


class WebInterruptMonitor:
    """Monitor for checking web interrupt requests."""

    def __init__(self, web_state: Any):
        self.web_state = web_state

    def should_interrupt(self) -> bool:
        """Check if interrupt has been requested."""
        return self.web_state.is_interrupt_requested()


class MainAgent(BaseAgent):
    """Custom agent that coordinates LLM interactions via HTTP."""

    @staticmethod
    def _classify_error(error_text: str) -> str:
        """Classify error type for targeted nudge selection.

        Args:
            error_text: The error message from a failed tool execution

        Returns:
            Error classification string matching a nudge_* reminder name suffix
        """
        error_lower = error_text.lower()
        if "permission denied" in error_lower:
            return "permission_error"
        if "old_content" in error_lower or "old content" in error_lower:
            return "edit_mismatch"
        if "no such file" in error_lower or "not found" in error_lower:
            return "file_not_found"
        if "syntax" in error_lower:
            return "syntax_error"
        if "429" in error_lower or "rate limit" in error_lower:
            return "rate_limit"
        if "timeout" in error_lower or "timed out" in error_lower:
            return "timeout"
        return "generic"

    def _get_smart_nudge(self, error_text: str) -> str:
        """Get a failure-type-specific nudge message.

        Args:
            error_text: The error message from a failed tool execution

        Returns:
            Appropriate nudge message for the error type
        """
        error_type = self._classify_error(error_text)
        if error_type == "generic":
            return get_reminder("failed_tool_nudge")
        try:
            return get_reminder(f"nudge_{error_type}")
        except KeyError:
            return get_reminder("failed_tool_nudge")

    def _check_todo_completion(self) -> tuple[bool, str]:
        """Check if completion is allowed given todo state.

        This validation ensures todos are properly completed before the agent
        finishes. It covers all completion paths: implicit, exhausted nudges,
        and explicit task_complete.

        Returns:
            Tuple of (can_complete, nudge_message):
            - can_complete: True if OK to complete, False if incomplete todos exist
            - nudge_message: Message prompting agent to complete todos (empty if can_complete)
        """
        if not hasattr(self, "tool_registry") or not self.tool_registry:
            return True, ""

        todo_handler = getattr(self.tool_registry, "todo_handler", None)
        if not todo_handler:
            return True, ""

        if not todo_handler.has_todos():
            return True, ""  # No todos created - OK to complete

        incomplete = todo_handler.get_incomplete_todos()
        if not incomplete:
            return True, ""  # All todos done - OK to complete

        # Build nudge message with incomplete todo titles
        titles = [t.title for t in incomplete[:3]]
        todo_list = "\n".join(f"  \u2022 {title}" for title in titles)
        if len(incomplete) > 3:
            todo_list += "\n  ..."
        msg = get_reminder(
            "incomplete_todos_nudge",
            count=str(len(incomplete)),
            todo_list=todo_list,
        )
        return False, msg

    @staticmethod
    def _messages_contain_images(messages: list[dict]) -> bool:
        """Check if any message contains multimodal image content blocks."""
        for msg in messages:
            content = msg.get("content")
            if isinstance(content, list):
                for block in content:
                    if isinstance(block, dict) and block.get("type") == "image":
                        return True
        return False

    def __init__(
        self,
        config: AppConfig,
        tool_registry: Any,
        mode_manager: Any,
        working_dir: Any = None,
        allowed_tools: Any = None,
        env_context: Any = None,
    ) -> None:
        """Initialize the MainAgent.

        Args:
            config: Application configuration
            tool_registry: The tool registry for tool execution
            mode_manager: Mode manager for operation mode
            working_dir: Optional working directory for file operations
            allowed_tools: Optional list of allowed tool names for filtering.
                          If None, all tools are allowed. Used by subagents
                          to restrict available tools (e.g., Code-Explorer
                          only gets read_file, search, list_files, etc.)
            env_context: Optional EnvironmentContext for rich system prompt
        """
        self.__http_client = None  # Lazy initialization - defer API key validation
        self.__thinking_http_client = None  # Lazy initialization for Thinking model
        self.__critique_http_client = None  # Lazy initialization for Critique model
        self.__vlm_http_client = None  # Lazy initialization for VLM model
        self._compactor = None  # Lazy initialization for context compaction
        self._response_cleaner = ResponseCleaner()
        self._working_dir = working_dir
        self._env_context = env_context
        self._schema_builder = ToolSchemaBuilder(tool_registry, allowed_tools)
        self.is_subagent = allowed_tools is not None

        # Live message injection queue (thread-safe, bounded)
        self._injection_queue: queue_mod.Queue[str] = queue_mod.Queue(maxsize=10)

        super().__init__(config, tool_registry, mode_manager)

    @property
    def _http_client(self) -> Any:
        """Lazily create HTTP client on first access (defers API key validation)."""
        if self.__http_client is None:
            self.__http_client = create_http_client(self.config)
        return self.__http_client

    @property
    def _thinking_http_client(self) -> Any:
        """Lazily create HTTP client for Thinking model provider.

        Only created if Thinking model is configured with a different provider.
        Returns None if Thinking model uses same provider as Normal model.
        """
        if self.__thinking_http_client is None:
            # Only create if thinking provider is different from normal provider
            thinking_provider = self.config.model_thinking_provider
            if thinking_provider and thinking_provider != self.config.model_provider:
                try:
                    self.__thinking_http_client = create_http_client_for_provider(
                        thinking_provider, self.config
                    )
                except ValueError:
                    # API key not set - fall back to normal client
                    return self._http_client
        return self.__thinking_http_client

    @property
    def _critique_http_client(self) -> Any:
        """Lazily create HTTP client for Critique model provider.

        Only created if Critique model is configured with a different provider.
        Falls back to thinking client, then normal client.
        """
        if self.__critique_http_client is None:
            critique_provider = self.config.model_critique_provider
            if critique_provider and critique_provider != self.config.model_provider:
                # Different provider than normal - create dedicated client
                if critique_provider != self.config.model_thinking_provider:
                    # Also different from thinking - create new client
                    try:
                        self.__critique_http_client = create_http_client_for_provider(
                            critique_provider, self.config
                        )
                    except ValueError:
                        # API key not set - fall back to thinking or normal client
                        return self._thinking_http_client or self._http_client
                else:
                    # Same as thinking provider - reuse thinking client
                    return self._thinking_http_client or self._http_client
        return self.__critique_http_client

    @property
    def _vlm_http_client(self) -> Any:
        """Lazily create HTTP client for VLM model provider.

        Only created if VLM model is configured with a different provider.
        Falls back to normal client on error.
        """
        if self.__vlm_http_client is None:
            vlm_provider = self.config.model_vlm_provider
            if vlm_provider and vlm_provider != self.config.model_provider:
                try:
                    self.__vlm_http_client = create_http_client_for_provider(
                        vlm_provider, self.config
                    )
                except ValueError:
                    return self._http_client
        return self.__vlm_http_client

    def _resolve_vlm_model_and_client(self, messages: list[dict]) -> tuple[str, Any]:
        """Resolve model/client, routing to VLM when images are present."""
        if self._messages_contain_images(messages):
            vlm_info = self.config.get_vlm_model_info()
            if vlm_info is not None:
                _, vlm_model_id, _ = vlm_info
                vlm_provider = self.config.model_vlm_provider
                if vlm_provider and vlm_provider != self.config.model_provider:
                    http_client = self._vlm_http_client or self._http_client
                else:
                    http_client = self._http_client
                return vlm_model_id, http_client
        return self.config.model, self._http_client

    def build_system_prompt(self, thinking_visible: bool = False) -> str:
        """Build the system prompt for the agent.

        Also computes the stable/dynamic split for prompt caching. The
        stable part becomes the system message content; the dynamic part
        is passed as ``_system_dynamic`` in the payload so that
        AnthropicAdapter can build cache_control blocks.

        Args:
            thinking_visible: If True, use thinking-specialized prompt

        Returns:
            The formatted system prompt string (stable + dynamic combined)
        """
        if thinking_visible:
            builder = ThinkingPromptBuilder(
                self.tool_registry, self._working_dir, env_context=self._env_context
            )
            full = builder.build()
            self._system_stable = full
            self._system_dynamic = ""
            return full

        builder = SystemPromptBuilder(
            self.tool_registry, self._working_dir, env_context=self._env_context
        )
        stable, dynamic = builder.build_two_part()
        self._system_stable = stable
        self._system_dynamic = dynamic
        # Return combined prompt for contexts that need a single string
        if dynamic:
            return f"{stable}\n\n{dynamic}"
        return stable

    def build_tool_schemas(self, thinking_visible: bool = True) -> list[dict[str, Any]]:
        return self._schema_builder.build(thinking_visible=thinking_visible)

    def _maybe_compact(self, messages: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Auto-compact messages if approaching the context window limit."""
        if self._compactor is None:
            from swecli.core.context_engineering.compaction import ContextCompactor

            self._compactor = ContextCompactor(self.config, self._http_client)

        if self._compactor.should_compact(messages, self.system_prompt):
            return self._compactor.compact(messages, self.system_prompt)
        return messages

    def call_thinking_llm(
        self,
        messages: list[dict],
        task_monitor: Optional[Any] = None,
    ) -> dict:
        """Call LLM for thinking phase only - NO tools, just reasoning.

        This makes a separate LLM call using the thinking system prompt
        to get pure reasoning/analysis before the action phase.

        Args:
            messages: Conversation messages (will use thinking system prompt)
            task_monitor: Optional monitor for tracking progress

        Returns:
            Dict with success status and thinking content
        """
        # Use thinking model if configured, otherwise normal model
        if self.config.model_thinking:
            model_id = self.config.model_thinking
            http_client = self._thinking_http_client or self._http_client
        else:
            model_id = self.config.model
            http_client = self._http_client

        # NO tools - pure reasoning
        payload = {
            "model": model_id,
            "messages": messages,
            **http_client.build_temperature_param(model_id, self.config.temperature),
            **http_client.build_max_tokens_param(model_id, self.config.max_tokens),
        }

        result = http_client.post_json(payload, task_monitor=task_monitor)
        if not result.success or result.response is None:
            return {
                "success": False,
                "error": result.error or "Unknown error",
                "content": "",
            }

        response = result.response
        if response.status_code != 200:
            return {
                "success": False,
                "error": f"API Error {response.status_code}: {response.text}",
                "content": "",
            }

        response_data = response.json()
        choice = response_data["choices"][0]
        message_data = choice["message"]

        raw_content = message_data.get("content")
        cleaned_content = self._response_cleaner.clean(raw_content) if raw_content else ""

        return {
            "success": True,
            "content": cleaned_content,
        }

    def call_critique_llm(
        self,
        thinking_trace: str,
        task_monitor: Optional[Any] = None,
    ) -> dict:
        """Call LLM to critique a thinking trace.

        This makes a separate LLM call to analyze and critique the reasoning
        in a thinking trace, providing feedback to improve it.

        Args:
            thinking_trace: The thinking trace to critique
            task_monitor: Optional monitor for tracking progress

        Returns:
            Dict with success status and critique content
        """
        from swecli.core.agents.prompts import load_prompt

        # Use critique model if configured, fallback to thinking, then normal
        if self.config.model_critique:
            model_id = self.config.model_critique
            http_client = (
                self._critique_http_client or self._thinking_http_client or self._http_client
            )
        elif self.config.model_thinking:
            model_id = self.config.model_thinking
            http_client = self._thinking_http_client or self._http_client
        else:
            model_id = self.config.model
            http_client = self._http_client

        # Load critique system prompt
        critique_system_prompt = load_prompt("system/critique_system_prompt")

        # Build messages for critique
        critique_messages = [
            {"role": "system", "content": critique_system_prompt},
            {
                "role": "user",
                "content": f"Please critique the following thinking trace:\n\n{thinking_trace}",
            },
        ]

        # NO tools - pure critique
        payload = {
            "model": model_id,
            "messages": critique_messages,
            **http_client.build_temperature_param(model_id, self.config.temperature),
            **http_client.build_max_tokens_param(
                model_id, min(2048, self.config.max_tokens)
            ),  # Limit critique length
        }

        result = http_client.post_json(payload, task_monitor=task_monitor)
        if not result.success or result.response is None:
            return {
                "success": False,
                "error": result.error or "Unknown error",
                "content": "",
            }

        response = result.response
        if response.status_code != 200:
            return {
                "success": False,
                "error": f"API Error {response.status_code}: {response.text}",
                "content": "",
            }

        response_data = response.json()
        choice = response_data["choices"][0]
        message_data = choice["message"]

        raw_content = message_data.get("content")
        cleaned_content = self._response_cleaner.clean(raw_content) if raw_content else ""

        return {
            "success": True,
            "content": cleaned_content,
        }

    def call_llm(
        self,
        messages: list[dict],
        task_monitor: Optional[Any] = None,
        thinking_visible: bool = True,
    ) -> dict:
        """Call LLM with tools for action phase.

        Args:
            messages: Conversation messages
            task_monitor: Optional monitor for tracking progress
            thinking_visible: If False, excludes think tool from schemas

        Returns:
            Dict with success status, content, tool_calls, etc.
        """
        # Route to VLM model when images are present, otherwise use normal model
        model_id, http_client = self._resolve_vlm_model_and_client(messages)

        # Rebuild schemas with current thinking visibility
        # Think tool is excluded from schemas since thinking is now a pre-processing step
        tool_schemas = self._schema_builder.build(thinking_visible=False)

        # Always use auto tool_choice - no more force_think
        tool_choice = "auto"

        payload = {
            "model": model_id,
            "messages": messages,
            "tools": tool_schemas,
            "tool_choice": tool_choice,
            **http_client.build_temperature_param(model_id, self.config.temperature),
            **http_client.build_max_tokens_param(model_id, self.config.max_tokens),
        }

        result = http_client.post_json(payload, task_monitor=task_monitor)
        if not result.success or result.response is None:
            return {
                "success": False,
                "error": result.error or "Unknown error",
                "interrupted": result.interrupted,
            }

        response = result.response
        if response.status_code != 200:
            return {
                "success": False,
                "error": f"API Error {response.status_code}: {response.text}",
            }

        response_data = response.json()
        choice = response_data["choices"][0]
        message_data = choice["message"]

        raw_content = message_data.get("content")
        cleaned_content = self._response_cleaner.clean(raw_content) if raw_content else None

        # Extract reasoning_content for OpenAI reasoning models (o1, o3, etc.)
        # This is the native thinking/reasoning trace from models like o1-preview
        reasoning_content = message_data.get("reasoning_content")

        if task_monitor and "usage" in response_data:
            usage = response_data["usage"]
            total_tokens = usage.get("total_tokens", 0)
            if total_tokens > 0:
                task_monitor.update_tokens(total_tokens)

        return {
            "success": True,
            "message": message_data,
            "content": cleaned_content,
            "tool_calls": message_data.get("tool_calls"),
            "reasoning_content": reasoning_content,  # Native thinking trace from model
            "usage": response_data.get("usage"),
        }

    def inject_user_message(self, text: str) -> None:
        """Inject a user message into the running agent loop.

        Thread-safe. Called from the WebSocket handler thread.
        Messages exceeding the queue capacity (10) are logged and dropped.
        """
        _logger = logging.getLogger(__name__)
        try:
            self._injection_queue.put_nowait(text)
        except queue_mod.Full:
            _logger.warning("Injection queue full, dropping message: %s", text[:80])

    def _drain_injected_messages(self, messages: list, max_per_drain: int = 3) -> int:
        """Drain injected user messages into the conversation.

        Appends each message to *messages* and persists to the session if a
        session manager is available (set during run_sync). The WebSocket
        handler does NOT persist — all writes happen on the agent thread (EC5).

        Returns:
            Number of messages drained.
        """
        _logger = logging.getLogger(__name__)
        from swecli.models.message import ChatMessage, Role

        session_mgr = getattr(self, "_run_session_manager", None)
        count = 0
        while count < max_per_drain:
            try:
                text = self._injection_queue.get_nowait()
            except queue_mod.Empty:
                break
            messages.append({"role": "user", "content": text})
            if session_mgr is not None:
                user_msg = ChatMessage(role=Role.USER, content=text)
                session_mgr.add_message(user_msg)
            count += 1
            _logger.info("Drained injected message (%d): %s", count, text[:60])
        return count

    def _final_drain_injection_queue(self) -> None:
        """Persist any late-arriving injected messages before exiting run_sync (EC1)."""
        from swecli.models.message import ChatMessage, Role

        session_mgr = getattr(self, "_run_session_manager", None)
        while True:
            try:
                text = self._injection_queue.get_nowait()
                if session_mgr is not None:
                    user_msg = ChatMessage(role=Role.USER, content=text)
                    session_mgr.add_message(user_msg)
            except queue_mod.Empty:
                break
        self._run_session_manager = None

    def run_sync(
        self,
        message: str,
        deps: Any,
        message_history: Optional[list[dict]] = None,
        ui_callback: Optional[Any] = None,
        max_iterations: Optional[int] = None,  # None = unlimited
        task_monitor: Optional[Any] = None,  # Task monitor for interrupt support
        continue_after_subagent: bool = False,  # If True, don't inject stop signal after subagent
    ) -> dict:
        from swecli.core.context_engineering.validated_message_list import ValidatedMessageList

        messages = message_history or []
        if not isinstance(messages, ValidatedMessageList):
            messages = ValidatedMessageList(messages)

        if not messages or messages[0].get("role") != "system":
            # Use stable part as system content; dynamic part goes via _system_dynamic
            system_content = getattr(self, "_system_stable", None) or self.system_prompt
            messages.insert(0, {"role": "system", "content": system_content})

        messages.append({"role": "user", "content": message})

        # Store session manager for drain persistence (EC5)
        self._run_session_manager = getattr(deps, "session_manager", None)

        # Clear stale injected messages from any previous execution (EC2)
        while not self._injection_queue.empty():
            try:
                self._injection_queue.get_nowait()
            except queue_mod.Empty:
                break

        iteration = 0
        consecutive_no_tool_calls = 0
        MAX_NUDGE_ATTEMPTS = 3  # After this many nudges, treat as implicit completion
        todo_nudge_count = 0
        MAX_TODO_NUDGES = 2  # After this many todo nudges, allow completion anyway
        completion_nudge_sent = False
        interrupted = False

        try:
            while True:
                # Drain any injected user messages before this iteration
                self._drain_injected_messages(messages)

                iteration += 1

                # Safety limit only if explicitly set
                if max_iterations is not None and iteration > max_iterations:
                    return {
                        "content": "Max iterations reached without completion",
                        "messages": messages,
                        "success": False,
                    }

                # Check for interrupt request via task_monitor (Textual UI)
                if task_monitor is not None and task_monitor.should_interrupt():
                    interrupted = True
                    return {
                        "content": "Task interrupted by user",
                        "messages": messages,
                        "success": False,
                        "interrupted": True,
                    }

                # Check for interrupt request (for web UI)
                if hasattr(self, "web_state") and self.web_state.is_interrupt_requested():
                    self.web_state.clear_interrupt()
                    interrupted = True
                    return {
                        "content": "Task interrupted by user",
                        "messages": messages,
                        "success": False,
                        "interrupted": True,
                    }

                # Auto-compact context if approaching the model's token limit
                messages = self._maybe_compact(messages)

                # Route to VLM model when images are present
                model_id, http_client = self._resolve_vlm_model_and_client(messages)

                payload = {
                    "model": model_id,
                    "messages": messages,
                    "tools": self.tool_schemas,
                    "tool_choice": "auto",
                    **http_client.build_temperature_param(model_id, self.config.temperature),
                    **http_client.build_max_tokens_param(model_id, self.config.max_tokens),
                }

                # Pass dynamic system content for prompt caching (Anthropic)
                system_dynamic = getattr(self, "_system_dynamic", "")
                if system_dynamic:
                    payload["_system_dynamic"] = system_dynamic

                # Use provided task_monitor, or create WebInterruptMonitor for web UI
                monitor = task_monitor
                if monitor is None and hasattr(self, "web_state"):
                    monitor = WebInterruptMonitor(self.web_state)

                result = http_client.post_json(payload, task_monitor=monitor)
                if not result.success or result.response is None:
                    error_msg = result.error or "Unknown error"
                    return {
                        "content": error_msg,
                        "messages": messages,
                        "success": False,
                    }

                response = result.response
                if response.status_code != 200:
                    error_msg = f"API Error {response.status_code}: {response.text}"
                    return {
                        "content": error_msg,
                        "messages": messages,
                        "success": False,
                    }

                response_data = response.json()
                choice = response_data["choices"][0]
                message_data = choice["message"]

                raw_content = message_data.get("content")
                cleaned_content = self._response_cleaner.clean(raw_content) if raw_content else None

                assistant_msg: dict[str, Any] = {
                    "role": "assistant",
                    "content": raw_content or "",
                }
                if "tool_calls" in message_data and message_data["tool_calls"]:
                    assistant_msg["tool_calls"] = message_data["tool_calls"]
                messages.append(assistant_msg)

                if "tool_calls" not in message_data or not message_data["tool_calls"]:
                    # No tool calls - check if we should nudge or accept implicit completion
                    # Check if last tool execution failed (should nudge to retry)
                    last_tool_failed = False
                    last_error_text = ""
                    for msg in reversed(messages):
                        if msg.get("role") == "tool":
                            content = msg.get("content", "")
                            if content.startswith("Error:"):
                                last_tool_failed = True
                                last_error_text = content
                            break

                    if last_tool_failed:
                        # Last tool failed - nudge agent to fix and retry
                        consecutive_no_tool_calls += 1

                        if consecutive_no_tool_calls >= MAX_NUDGE_ATTEMPTS:
                            # Exhausted nudge attempts - check todos before accepting completion
                            can_complete, nudge_msg = self._check_todo_completion()
                            if not can_complete and todo_nudge_count < MAX_TODO_NUDGES:
                                todo_nudge_count += 1
                                messages.append({"role": "user", "content": nudge_msg})
                                continue

                            # Check injection queue before accepting completion
                            if not self._injection_queue.empty():
                                self._drain_injected_messages(messages)
                                consecutive_no_tool_calls = 0
                                continue

                            # Nudge once for empty completion summary
                            if not cleaned_content and not completion_nudge_sent:
                                completion_nudge_sent = True
                                messages.append(
                                    {
                                        "role": "user",
                                        "content": get_reminder("completion_summary_nudge"),
                                    }
                                )
                                continue

                            # Accept best-effort completion
                            return {
                                "content": cleaned_content or "Done.",
                                "messages": messages,
                                "success": True,
                            }

                        # Use smart nudge with error-specific guidance
                        messages.append(
                            {
                                "role": "user",
                                "content": self._get_smart_nudge(last_error_text),
                            }
                        )
                        continue

                    # Last tool succeeded (or no previous tool) - check todos before implicit completion
                    can_complete, nudge_msg = self._check_todo_completion()
                    if not can_complete and todo_nudge_count < MAX_TODO_NUDGES:
                        todo_nudge_count += 1
                        messages.append({"role": "user", "content": nudge_msg})
                        continue

                    # Check injection queue before accepting implicit completion
                    if not self._injection_queue.empty():
                        self._drain_injected_messages(messages)
                        continue

                    # Nudge once for empty completion summary
                    if not cleaned_content and not completion_nudge_sent:
                        completion_nudge_sent = True
                        messages.append(
                            {"role": "user", "content": get_reminder("completion_summary_nudge")}
                        )
                        continue

                    # Return the natural completion content directly without extra LLM calls
                    # This prevents subagents from making unnecessary tool calls (like get_subagent_output)
                    return {
                        "content": cleaned_content or "Done.",
                        "messages": messages,
                        "success": True,
                    }

                # Reset counter when we have tool calls
                consecutive_no_tool_calls = 0

                for tool_call in message_data["tool_calls"]:
                    tool_name = tool_call["function"]["name"]
                    tool_args = json.loads(tool_call["function"]["arguments"])

                    # Check for explicit task completion
                    if tool_name == "task_complete":
                        summary = tool_args.get("summary", "Task completed")
                        status = tool_args.get("status", "success")

                        # Only check todos for successful completions
                        if status == "success":
                            can_complete, nudge_msg = self._check_todo_completion()
                            if not can_complete and todo_nudge_count < MAX_TODO_NUDGES:
                                todo_nudge_count += 1
                                messages.append({"role": "user", "content": nudge_msg})
                                continue  # Reject task_complete, loop again

                        # Check injection queue before accepting task_complete
                        if not self._injection_queue.empty():
                            self._drain_injected_messages(messages)
                            # Add tool result so conversation stays valid, then continue
                            messages.append(
                                {
                                    "role": "tool",
                                    "tool_call_id": tool_call["id"],
                                    "content": "Completion deferred: new user messages arrived.",
                                }
                            )
                            break  # Break inner for-loop, continue outer while-loop

                        return {
                            "content": summary,
                            "messages": messages,
                            "success": status != "failed",
                            "completion_status": status,
                        }

                    # Notify UI callback before tool execution
                    if ui_callback and hasattr(ui_callback, "on_tool_call"):
                        ui_callback.on_tool_call(tool_name, tool_args)

                    # Check if this is a subagent (has overridden system prompt)
                    is_subagent = (
                        hasattr(self, "_subagent_system_prompt")
                        and self._subagent_system_prompt is not None
                    )

                    # Log tool registry type for debugging Docker execution
                    import logging

                    _logger = logging.getLogger(__name__)
                    _logger.info(f"MainAgent executing tool: {tool_name}")
                    _logger.info(f"  tool_registry type: {type(self.tool_registry).__name__}")

                    result = self.tool_registry.execute_tool(
                        tool_name,
                        tool_args,
                        mode_manager=deps.mode_manager,
                        approval_manager=deps.approval_manager,
                        undo_manager=deps.undo_manager,
                        task_monitor=task_monitor,
                        is_subagent=is_subagent,
                        ui_callback=ui_callback,
                    )

                    # Notify UI callback after tool execution
                    if ui_callback and hasattr(ui_callback, "on_tool_result"):
                        ui_callback.on_tool_result(tool_name, tool_args, result)

                    # Check if tool execution was interrupted (e.g., subagent cancelled via Escape)
                    if result.get("interrupted"):
                        interrupted = True
                        return {
                            "content": "Task interrupted by user",
                            "messages": messages,
                            "success": False,
                            "interrupted": True,
                        }

                    # Build tool result - prefer separate_response (subagent output) over output
                    separate_response = result.get("separate_response")
                    if result["success"]:
                        tool_result = (
                            separate_response if separate_response else result.get("output", "")
                        )
                        # Prepend completion status so agent knows subagent is done
                        completion_status = result.get("completion_status")
                        if completion_status:
                            tool_result = f"[completion_status={completion_status}]\n{tool_result}"
                    else:
                        tool_result = f"Error: {result.get('error', 'Tool execution failed')}"
                    # Append LLM-only suffix (e.g., retry prompts) - hidden from UI
                    if result.get("_llm_suffix"):
                        tool_result += result["_llm_suffix"]
                    messages.append(
                        {
                            "role": "tool",
                            "tool_call_id": tool_call["id"],
                            "content": tool_result,
                        }
                    )
        finally:
            self._final_drain_injection_queue()
            if (
                getattr(self.config, "enable_sound", False)
                and not getattr(self, "is_subagent", False)
                and not interrupted
            ):
                play_finish_sound()
