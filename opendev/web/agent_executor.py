"""Agent executor for WebSocket queries with streaming support."""

from __future__ import annotations

import atexit
import asyncio
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any, Dict, Optional, Tuple

from opendev.web.state import WebState
from opendev.web.logging_config import logger
from opendev.models.message import ChatMessage, Role
from opendev.models.agent_deps import AgentDependencies
from opendev.core.runtime import ConfigManager
from opendev.models.config import AppConfig


class AgentExecutor:
    """Executes agent queries in background with WebSocket streaming."""

    def __init__(self, state: WebState):
        """Initialize agent executor.

        Args:
            state: Shared web state
        """
        self.state = state
        self.executor = ThreadPoolExecutor(max_workers=4)
        atexit.register(self.executor.shutdown, wait=False)

    async def execute_query(
        self,
        message: str,
        ws_manager: Any,
        *,
        session_id: str,
        session: Any,
    ) -> None:
        """Execute query and stream results via WebSocket.

        Args:
            message: User query
            ws_manager: WebSocket manager for broadcasting
            session_id: Session ID for scoping this execution
            session: Pre-loaded Session object (avoids mutating current_session)
        """
        try:
            # Mark session as running
            self.state.set_session_running(session_id)
            await ws_manager.broadcast({
                "type": "session_activity",
                "data": {"session_id": session_id, "status": "running"},
            })

            # Broadcast message start
            try:
                await ws_manager.broadcast(
                    {
                        "type": "message_start",
                        "data": {
                            "messageId": str(time.time()),
                            "session_id": session_id,
                        },
                    }
                )
            except Exception as e:
                logger.error(f"Failed to broadcast message_start: {e}")

            # Run agent in thread pool to avoid blocking event loop
            loop = asyncio.get_event_loop()
            response = await loop.run_in_executor(
                self.executor,
                self._run_agent_sync,
                message,
                ws_manager,
                loop,
                session_id,
                session,
            )

            # Reconstruct and persist all assistant steps (tool calls + final response)
            logger.info(
                f"Agent response: success={response.get('success')}, "
                f"has_content={bool(response.get('content'))}"
            )
            if response and response.get("success"):
                web_ui_callback = response.pop("_web_ui_callback", None)
                thinking_trace = response.get("thinking_trace")
                reasoning_content = response.get("reasoning_content")
                token_usage = response.get("usage")

                self._reconstruct_and_persist_messages(
                    session,
                    response,
                    thinking_trace,
                    reasoning_content,
                    token_usage,
                    web_ui_callback,
                )

            # Save session to persist messages immediately
            self.state.session_manager.save_session(session)

            # Broadcast message complete
            try:
                await ws_manager.broadcast(
                    {
                        "type": "message_complete",
                        "data": {
                            "messageId": str(time.time()),
                            "session_id": session_id,
                        },
                    }
                )
            except Exception as e:
                logger.error(f"Failed to broadcast message_complete: {e}")

        except Exception as e:
            # Broadcast error
            logger.error(f"❌ Agent execution error: {e}")
            import traceback

            logger.error(traceback.format_exc())
            try:
                await ws_manager.broadcast({
                    "type": "error",
                    "data": {"message": str(e), "session_id": session_id},
                })
            except Exception as broadcast_err:
                logger.error(f"Failed to broadcast error: {broadcast_err}")
        finally:
            # Always mark session as idle and clean up injection queue
            self.state.set_session_idle(session_id)
            self.state.clear_injection_queue(session_id)
            try:
                await ws_manager.broadcast({
                    "type": "session_activity",
                    "data": {"session_id": session_id, "status": "idle"},
                })
            except Exception:
                pass

    def _run_agent_sync(
        self,
        message: str,
        ws_manager: Any,
        loop: asyncio.AbstractEventLoop,
        session_id: str,
        session: Any,
    ) -> Dict[str, Any]:
        """Run agent synchronously in thread pool.

        Args:
            message: User query
            ws_manager: WebSocket manager
            loop: Event loop for async operations
            session_id: Session ID for scoping
            session: Pre-loaded Session object

        Returns:
            Agent response
        """
        from opendev.core.runtime.services import RuntimeService
        from opendev.core.context_engineering.tools.implementations import (
            FileOperations,
            WriteTool,
            EditTool,
            BashTool,
            WebFetchTool,
            OpenBrowserTool,
            WebScreenshotTool,
        )
        from opendev.core.context_engineering.tools.implementations.web_search_tool import (
            WebSearchTool,
        )
        from opendev.core.context_engineering.tools.implementations.notebook_edit_tool import (
            NotebookEditTool,
        )
        from opendev.core.context_engineering.tools.implementations.ask_user_tool import AskUserTool
        from opendev.web.web_approval_manager import WebApprovalManager
        from opendev.web.web_ask_user_manager import WebAskUserManager
        from opendev.web.web_ui_callback import WebUICallback
        from opendev.web.ws_tool_broadcaster import WebSocketToolBroadcaster

        # Clear any previous interrupt flags
        self.state.clear_interrupt()

        # Resolve config/working directory from session (no mutation of current_session)
        config_manager, config, working_dir = self._resolve_runtime_context_for_session(session)

        # Initialize tools
        file_ops = FileOperations(config, working_dir)
        write_tool = WriteTool(config, working_dir)
        edit_tool = EditTool(config, working_dir)
        bash_tool = BashTool(config, working_dir)
        web_fetch_tool = WebFetchTool(config, working_dir)
        web_search_tool = WebSearchTool(config, working_dir)
        notebook_edit_tool = NotebookEditTool(working_dir)
        # Create web-based ask-user manager with session_id
        web_ask_user_manager = WebAskUserManager(ws_manager, loop, session_id=session_id)
        ask_user_tool = AskUserTool(ui_prompt_callback=web_ask_user_manager.prompt_user)
        open_browser_tool = OpenBrowserTool(config, working_dir)
        web_screenshot_tool = WebScreenshotTool(config, working_dir)

        # Create web-based approval manager with session_id
        web_approval_manager = WebApprovalManager(ws_manager, loop, session_id=session_id)

        # Create web UI callback for plan approval, subagent events, etc.
        web_ui_callback = WebUICallback(ws_manager, loop, session_id, self.state)

        # Build runtime suite
        runtime_service = RuntimeService(config_manager, self.state.mode_manager)
        runtime_suite = runtime_service.build_suite(
            file_ops=file_ops,
            write_tool=write_tool,
            edit_tool=edit_tool,
            bash_tool=bash_tool,
            web_fetch_tool=web_fetch_tool,
            web_search_tool=web_search_tool,
            notebook_edit_tool=notebook_edit_tool,
            ask_user_tool=ask_user_tool,
            open_browser_tool=open_browser_tool,
            web_screenshot_tool=web_screenshot_tool,
            mcp_manager=self.state.mcp_manager,
        )

        # Wire hooks system
        try:
            from opendev.core.hooks.loader import load_hooks_config
            from opendev.core.hooks.manager import HookManager

            hooks_config = load_hooks_config(working_dir)
            if hooks_config and hooks_config.hooks:
                hook_manager = HookManager(
                    hooks_config, session_id=session_id, cwd=str(working_dir)
                )
                runtime_suite.tool_registry.set_hook_manager(hook_manager)
                subagent_mgr = runtime_suite.tool_registry.get_subagent_manager()
                if subagent_mgr and hasattr(subagent_mgr, "set_hook_manager"):
                    subagent_mgr.set_hook_manager(hook_manager)
        except Exception as e:
            logger.warning(f"Failed to wire hooks: {e}")

        # Set thinking level from web state
        from opendev.core.context_engineering.tools.handlers.thinking_handler import ThinkingLevel
        thinking_level_str = self.state.get_thinking_level()
        try:
            thinking_level = ThinkingLevel(thinking_level_str)
        except ValueError:
            thinking_level = ThinkingLevel.MEDIUM
        runtime_suite.tool_registry.thinking_handler.set_level(thinking_level)

        # Wrap tool registry with WebSocket broadcaster (includes session_id)
        wrapped_registry = WebSocketToolBroadcaster(
            runtime_suite.tool_registry,
            ws_manager,
            loop,
            working_dir=working_dir,
            session_id=session_id,
        )

        # Instantiate CostTracker for this execution
        from opendev.core.runtime.cost_tracker import CostTracker

        cost_tracker = CostTracker()

        # Get agent and replace its tool registry with wrapped version
        agent = runtime_suite.agents.normal
        agent.tool_registry = wrapped_registry
        agent._cost_tracker = cost_tracker
        # Pass the state to the agent for interrupt checking
        agent.web_state = self.state
        # Wire injection queue so mid-execution user messages reach the agent loop
        agent._injection_queue = self.state.get_injection_queue(session_id)

        # Use session directly (no mutation of current_session)
        message_history = session.to_api_messages()

        # Create agent dependencies with web approval manager
        deps = AgentDependencies(
            mode_manager=self.state.mode_manager,
            approval_manager=web_approval_manager,  # Use web-based approval
            undo_manager=self.state.undo_manager,
            session_manager=self.state.session_manager,
            working_dir=working_dir,
            console=None,  # No console for web
            config=config,
        )

        # === THINKING PRE-PHASE (mirrors ReactExecutor._get_thinking_trace) ===
        thinking_trace = None
        thinking_level_str = self.state.get_thinking_level()
        if thinking_level_str != "Off":
            thinking_trace = self._run_thinking_phase(
                agent, message_history, ws_manager, loop, thinking_level_str,
                session_id=session_id,
            )
            # Inject thinking trace into messages for the action phase
            if thinking_trace:
                from opendev.core.agents.prompts.reminders import get_reminder

                message_history.append(
                    {
                        "role": "user",
                        "content": get_reminder(
                            "thinking_trace_reminder", thinking_trace=thinking_trace
                        ),
                    }
                )

        # Run agent
        try:
            result = agent.run_sync(
                message,
                deps,
                message_history=message_history,
                ui_callback=web_ui_callback,
            )

            # Broadcast the full response as a chunk
            logger.info(f"Agent run_sync completed: success={result.get('success')}")
            if result.get("success"):
                content = result.get("content", "")
                logger.info(
                    f"Broadcasting message_chunk with content length: {len(str(content))}"
                )
                try:
                    future = asyncio.run_coroutine_threadsafe(
                        ws_manager.broadcast(
                            {
                                "type": "message_chunk",
                                "data": {
                                    "content": str(content),
                                    "session_id": session_id,
                                },
                            }
                        ),
                        loop,
                    )
                    future.result(timeout=5)
                    logger.info("message_chunk broadcasted successfully")
                except Exception as e:
                    logger.error(f"Failed to broadcast message_chunk: {e}")
            else:
                logger.warning("Agent returned success=False, not broadcasting message_chunk")

            # Include thinking_trace and callback in the returned result for persistence
            result["thinking_trace"] = thinking_trace
            result["_web_ui_callback"] = web_ui_callback
            return result

        except Exception as e:
            return {"success": False, "error": str(e), "content": f"Error: {str(e)}"}

    def _reconstruct_and_persist_messages(
        self,
        session: Any,
        result: Dict[str, Any],
        thinking_trace: Optional[str],
        reasoning_content: Optional[str],
        token_usage: Optional[Dict[str, Any]],
        web_ui_callback: Any,
    ) -> None:
        """Parse run_sync message history and persist all assistant steps with tool calls."""
        import json
        from opendev.models.message import ToolCall as ToolCallModel
        from opendev.core.utils.tool_result_summarizer import summarize_tool_result

        api_messages = result.get("messages", [])
        original_content = result.get("content", "")
        is_first_assistant = True

        for i, msg in enumerate(api_messages):
            if msg.get("role") != "assistant":
                continue

            content = msg.get("content") or ""
            raw_tool_calls = msg.get("tool_calls")

            if not raw_tool_calls:
                # Final assistant message (no tool calls) — persist with metadata
                assistant_msg = ChatMessage(
                    role=Role.ASSISTANT,
                    content=content,
                    thinking_trace=thinking_trace if is_first_assistant else None,
                    reasoning_content=reasoning_content if is_first_assistant else None,
                    token_usage=token_usage,
                )
                session.add_message(assistant_msg)
                is_first_assistant = False
                continue

            # Assistant message WITH tool_calls — reconstruct ToolCall objects
            tool_call_objects = []
            for tc in raw_tool_calls:
                tc_id = tc["id"]
                tool_name = tc["function"]["name"]
                try:
                    tool_args = json.loads(tc["function"]["arguments"])
                except (json.JSONDecodeError, TypeError):
                    tool_args = {"raw": tc["function"].get("arguments", "")}

                # Find matching tool result in subsequent messages
                tool_result_str = None
                for j in range(i + 1, len(api_messages)):
                    if (
                        api_messages[j].get("role") == "tool"
                        and api_messages[j].get("tool_call_id") == tc_id
                    ):
                        tool_result_str = api_messages[j].get("content", "")
                        break

                # Detect errors (run_loop prefixes errors with "Error")
                is_error = bool(tool_result_str and tool_result_str.startswith("Error"))
                tool_error = tool_result_str if is_error else None
                tool_output = None if is_error else tool_result_str

                # Get nested calls for subagent tools
                nested_calls = []
                if tool_name == "spawn_subagent" and web_ui_callback:
                    nested_calls = web_ui_callback.get_and_clear_nested_calls()

                tool_call_objects.append(
                    ToolCallModel(
                        id=tc_id,
                        name=tool_name,
                        parameters=tool_args,
                        result=tool_result_str,
                        result_summary=summarize_tool_result(
                            tool_name, tool_output, tool_error
                        ),
                        error=tool_error,
                        approved=True,
                        nested_tool_calls=nested_calls,
                    )
                )

            assistant_msg = ChatMessage(
                role=Role.ASSISTANT,
                content=content,
                tool_calls=tool_call_objects,
                thinking_trace=thinking_trace if is_first_assistant else None,
                reasoning_content=reasoning_content if is_first_assistant else None,
            )
            session.add_message(assistant_msg)
            is_first_assistant = False

        # Fallback: if no assistant messages were found but we have content, save it
        if is_first_assistant and original_content:
            assistant_msg = ChatMessage(
                role=Role.ASSISTANT,
                content=original_content,
                thinking_trace=thinking_trace,
                reasoning_content=reasoning_content,
                token_usage=token_usage,
            )
            session.add_message(assistant_msg)

    def _run_thinking_phase(
        self,
        agent: Any,
        message_history: list,
        ws_manager: Any,
        loop: asyncio.AbstractEventLoop,
        thinking_level_str: str,
        session_id: Optional[str] = None,
    ) -> Optional[str]:
        """Run pre-thinking phase and broadcast result via WebSocket.

        Mirrors ReactExecutor._get_thinking_trace() from the TUI.
        Uses the full conversation history with a swapped thinking system prompt.
        """
        from opendev.core.agents.prompts.reminders import get_reminder

        try:
            # Build thinking system prompt
            thinking_system_prompt = agent.build_system_prompt(thinking_visible=True)

            # Clone messages with swapped system prompt
            thinking_messages = list(message_history)
            if thinking_messages and thinking_messages[0].get("role") == "system":
                thinking_messages[0] = {"role": "system", "content": thinking_system_prompt}
            else:
                thinking_messages.insert(0, {"role": "system", "content": thinking_system_prompt})

            # Build analysis prompt — todo-aware when todos exist
            todo_handler = getattr(
                getattr(agent, "tool_registry", None), "todo_handler", None
            )
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

            response = agent.call_thinking_llm(thinking_messages)

            if response.get("success"):
                thinking_trace = response.get("content", "")
                if thinking_trace and thinking_trace.strip():
                    # Broadcast as thinking_block
                    try:
                        future = asyncio.run_coroutine_threadsafe(
                            ws_manager.broadcast(
                                {
                                    "type": "thinking_block",
                                    "data": {
                                        "content": thinking_trace.strip(),
                                        "level": thinking_level_str,
                                        "session_id": session_id,
                                    },
                                }
                            ),
                            loop,
                        )
                        future.result(timeout=5)
                    except Exception as e:
                        logger.error(f"Failed to broadcast thinking_block: {e}")

                    # Handle High level (includes self-critique)
                    if thinking_level_str == "High":
                        thinking_trace = self._run_critique_phase(
                            agent, thinking_trace, message_history, ws_manager, loop,
                            session_id=session_id,
                        )

                    return thinking_trace.strip()
        except Exception as e:
            logger.error(f"Thinking phase error: {e}")
        return None

    def _run_critique_phase(
        self,
        agent: Any,
        thinking_trace: str,
        message_history: list,
        ws_manager: Any,
        loop: asyncio.AbstractEventLoop,
        session_id: Optional[str] = None,
    ) -> str:
        """Run critique phase (part of High level): critique and broadcast."""
        try:
            critique_response = agent.call_critique_llm(thinking_trace)
            if critique_response.get("success"):
                critique = critique_response.get("content", "")
                if critique and critique.strip():
                    # Broadcast critique block
                    try:
                        future = asyncio.run_coroutine_threadsafe(
                            ws_manager.broadcast(
                                {
                                    "type": "thinking_block",
                                    "data": {
                                        "content": critique.strip(),
                                        "level": "High",
                                        "session_id": session_id,
                                    },
                                }
                            ),
                            loop,
                        )
                        future.result(timeout=5)
                    except Exception:
                        pass
        except Exception as e:
            logger.error(f"Critique phase error: {e}")
        return thinking_trace

    def _resolve_runtime_context_for_session(
        self, session: Any
    ) -> Tuple[ConfigManager, AppConfig, Path]:
        """Determine config manager, config, and working dir for a specific session."""
        if session and session.working_directory:
            working_dir = Path(session.working_directory).expanduser().resolve()
            config_manager = ConfigManager(working_dir)
            config = config_manager.get_config()
        else:
            config_manager = self.state.config_manager
            config = config_manager.get_config()
            working_dir = Path(config_manager.working_dir).resolve()

        try:
            config_manager.ensure_directories()
        except Exception:
            pass

        return config_manager, config, working_dir
