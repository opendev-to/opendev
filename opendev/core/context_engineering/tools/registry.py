"""Primary tool registry implementation coordinating handlers."""

from __future__ import annotations

from typing import Any, Union, TYPE_CHECKING

from opendev.core.context_engineering.tools.context import ToolExecutionContext
import logging

if TYPE_CHECKING:
    from opendev.core.hooks.manager import HookManager

from opendev.core.context_engineering.tools.handlers.file_handlers import FileToolHandler
from opendev.core.context_engineering.mcp.handler import McpToolHandler
from opendev.core.context_engineering.tools.handlers.process_handlers import ProcessToolHandler
from opendev.core.context_engineering.tools.handlers.web_handlers import WebToolHandler
from opendev.core.context_engineering.tools.handlers.web_search_handler import WebSearchHandler
from opendev.core.context_engineering.tools.handlers.notebook_edit_handler import NotebookEditHandler
from opendev.core.context_engineering.tools.handlers.ask_user_handler import AskUserHandler
from opendev.core.context_engineering.tools.handlers.screenshot_handler import ScreenshotToolHandler
from opendev.core.context_engineering.tools.handlers.todo_handler import TodoHandler
from opendev.core.context_engineering.tools.handlers.thinking_handler import ThinkingHandler
from opendev.core.context_engineering.tools.handlers.search_tools_handler import SearchToolsHandler
from opendev.core.context_engineering.tools.handlers.batch_handler import BatchToolHandler

if TYPE_CHECKING:
    from opendev.core.skills import SkillLoader

logger = logging.getLogger(__name__)
from opendev.core.context_engineering.tools.implementations.pdf_tool import PDFTool
from opendev.core.context_engineering.tools.implementations.task_complete_tool import (
    TaskCompleteTool,
)
from opendev.core.context_engineering.tools.implementations.present_plan_tool import (
    PresentPlanTool,
)
from opendev.core.context_engineering.tools.symbol_tools import (
    handle_find_symbol,
    handle_find_referencing_symbols,
    handle_insert_before_symbol,
    handle_insert_after_symbol,
    handle_replace_symbol_body,
    handle_rename_symbol,
)

class ToolRegistry:
    """Dispatches tool invocations to dedicated handlers."""

    def __init__(
        self,
        file_ops: Union[Any, None] = None,
        write_tool: Union[Any, None] = None,
        edit_tool: Union[Any, None] = None,
        bash_tool: Union[Any, None] = None,
        web_fetch_tool: Union[Any, None] = None,
        web_search_tool: Union[Any, None] = None,
        notebook_edit_tool: Union[Any, None] = None,
        ask_user_tool: Union[Any, None] = None,
        open_browser_tool: Union[Any, None] = None,
        vlm_tool: Union[Any, None] = None,
        web_screenshot_tool: Union[Any, None] = None,
        mcp_manager: Union[Any, None] = None,
    ) -> None:
        self.file_ops = file_ops
        self.write_tool = write_tool
        self.edit_tool = edit_tool
        self.bash_tool = bash_tool
        self.web_fetch_tool = web_fetch_tool
        self.web_search_tool = web_search_tool
        self.notebook_edit_tool = notebook_edit_tool
        self.ask_user_tool = ask_user_tool
        self.open_browser_tool = open_browser_tool
        self.vlm_tool = vlm_tool
        self.web_screenshot_tool = web_screenshot_tool

        self._file_handler = FileToolHandler(file_ops, write_tool, edit_tool)
        self._process_handler = ProcessToolHandler(bash_tool)
        self._web_handler = WebToolHandler(web_fetch_tool)
        self._web_search_handler = WebSearchHandler(web_search_tool)
        self._notebook_edit_handler = NotebookEditHandler(notebook_edit_tool)
        self._ask_user_handler = AskUserHandler(ask_user_tool)
        self._mcp_handler = McpToolHandler(mcp_manager)
        self._screenshot_handler = ScreenshotToolHandler()
        self.todo_handler = TodoHandler()
        self.thinking_handler = ThinkingHandler()
        self._pdf_tool = PDFTool()
        self._task_complete_tool = TaskCompleteTool()
        self._present_plan_tool = PresentPlanTool()
        self._subagent_manager: Union[Any, None] = None
        self._hook_manager: Union["HookManager", None] = None
        self._skill_loader: Union["SkillLoader", None] = None

        # FileTime stale-read detection (shared across the session)
        from opendev.core.context_engineering.tools.file_time import FileTimeTracker
        self._file_time_tracker = FileTimeTracker()
        self._invoked_skills: set[str] = set()  # Track skills already loaded in this session

        # Token-efficient MCP tool discovery
        # Only tools in this set will have their schemas included in LLM context
        self._discovered_mcp_tools: set[str] = set()
        self._search_tools_handler = SearchToolsHandler(
            mcp_manager=mcp_manager,
            on_discover=self.discover_mcp_tool,
        )
        self._batch_handler: Union[BatchToolHandler, None] = None  # Lazy init after registry ready

        self.set_mcp_manager(mcp_manager)

        self._handlers: dict[str, Any] = {
            "write_file": self._file_handler.write_file,
            "edit_file": self._file_handler.edit_file,
            "read_file": self._file_handler.read_file,
            "list_files": self._file_handler.list_files,
            "search": self._file_handler.search,  # Unified: type="text" (default) or "ast"
            "run_command": self._process_handler.run_command,
            "list_processes": lambda args, ctx: self._process_handler.list_processes(),
            "get_process_output": self._process_handler.get_process_output,
            "kill_process": self._process_handler.kill_process,
            "fetch_url": self._web_handler.fetch_url,
            "web_search": self._web_search_handler.search,
            "notebook_edit": self._notebook_edit_handler.edit_cell,
            "ask_user": self._ask_user_handler.ask_questions,
            "open_browser": self._open_browser,
            "capture_screenshot": self._screenshot_handler.capture_screenshot,
            "analyze_image": self._analyze_image,
            "capture_web_screenshot": self._capture_web_screenshot,
            "write_todos": self._write_todos,
            "update_todo": self._update_todo,
            "complete_todo": self._complete_todo,
            "list_todos": lambda args, ctx=None: self.todo_handler.list_todos(),
            "clear_todos": lambda args, ctx=None: self.todo_handler.clear_todos(),
            # Symbol tools (LSP-based)
            "find_symbol": lambda args: handle_find_symbol(args),
            "find_referencing_symbols": lambda args: handle_find_referencing_symbols(args),
            "insert_before_symbol": lambda args: handle_insert_before_symbol(args),
            "insert_after_symbol": lambda args: handle_insert_after_symbol(args),
            "replace_symbol_body": lambda args: handle_replace_symbol_body(args),
            "rename_symbol": lambda args: handle_rename_symbol(args),
            # Subagent spawning tool
            "spawn_subagent": self._execute_spawn_subagent,
            # Get output from background subagent
            "get_subagent_output": self._get_subagent_output,
            # PDF extraction tool
            "read_pdf": self._read_pdf,
            # MCP tool discovery (token-efficient)
            "search_tools": self._search_tools_handler.search_tools,
            # Task completion tool
            "task_complete": self._execute_task_complete,
            # Plan presentation tool
            "present_plan": self._execute_present_plan,
            # Skills system tool
            "invoke_skill": self._handle_invoke_skill,
            # Batch tool for parallel/serial multi-tool execution
            "batch_tool": self._execute_batch_tool,
        }

        # Initialize batch handler now that _handlers is set up
        self._batch_handler = BatchToolHandler(self)

    def set_subagent_manager(self, manager: Any) -> None:
        """Set the subagent manager for task tool execution.

        Args:
            manager: SubAgentManager instance
        """
        self._subagent_manager = manager

    def get_subagent_manager(self) -> Union[Any, None]:
        """Get the subagent manager.

        Returns:
            SubAgentManager instance or None
        """
        return self._subagent_manager

    def set_hook_manager(self, manager: "HookManager") -> None:
        """Set the hook manager for lifecycle hooks.

        Args:
            manager: HookManager instance
        """
        self._hook_manager = manager

    def set_skill_loader(self, loader: "SkillLoader") -> None:
        """Set the skill loader for invoke_skill tool.

        Args:
            loader: SkillLoader instance
        """
        self._skill_loader = loader

    def get_skill_loader(self) -> Union["SkillLoader", None]:
        """Get the skill loader.

        Returns:
            SkillLoader instance or None
        """
        return self._skill_loader

    # ===== Token-Efficient MCP Tool Discovery =====

    def discover_mcp_tool(self, tool_name: str) -> None:
        """Mark an MCP tool as discovered.

        Discovered tools will have their schemas included in subsequent LLM calls.
        This enables token-efficient tool loading - only tools the agent has
        explicitly searched for (or attempted to use) will consume context tokens.

        Args:
            tool_name: Full MCP tool name (e.g., 'mcp__github__create_issue')
        """
        if tool_name and tool_name.startswith("mcp__"):
            self._discovered_mcp_tools.add(tool_name)
            logger.debug(f"Discovered MCP tool: {tool_name}")

    def get_discovered_mcp_tools(self) -> list[dict[str, Any]]:
        """Get schemas only for discovered MCP tools.

        Returns:
            List of tool schema dicts for discovered tools only
        """
        if not self.mcp_manager:
            return []

        all_tools = self.mcp_manager.get_all_tools()
        return [t for t in all_tools if t.get("name") in self._discovered_mcp_tools]

    def clear_discovered_tools(self) -> None:
        """Clear all discovered MCP tools.

        Useful when starting a new conversation or resetting state.
        """
        self._discovered_mcp_tools.clear()
        logger.debug("Cleared all discovered MCP tools")

    def _execute_spawn_subagent(
        self,
        arguments: dict[str, Any],
        context: Any = None,
        tool_call_id: Union[str, None] = None,
    ) -> dict[str, Any]:
        """Execute the spawn_subagent tool to spawn a subagent.

        Args:
            arguments: Tool arguments with 'description', 'prompt', and 'subagent_type'
            context: Tool execution context
            tool_call_id: Unique tool call ID for parent context tracking

        Returns:
            Result from subagent execution
        """
        if not self._subagent_manager:
            return {
                "success": False,
                "error": "SubAgentManager not configured. spawn_subagent tool unavailable.",
                "output": None,
            }

        description = arguments.get("description", "")
        # Use 'prompt' as task content, fallback to 'description' for backward compatibility
        task = arguments.get("prompt") or description
        subagent_type = arguments.get("subagent_type", "general-purpose")

        if not task:
            return {
                "success": False,
                "error": "Task prompt is required for spawn_subagent",
                "output": None,
            }

        # Create deps from context
        from opendev.core.agents.subagents.manager import SubAgentDeps

        deps = SubAgentDeps(
            mode_manager=context.mode_manager if context else None,
            approval_manager=context.approval_manager if context else None,
            undo_manager=context.undo_manager if context else None,
        )

        # Get ui_callback from context for nested tool call display
        ui_callback = context.ui_callback if context else None

        # Get task_monitor from context for interrupt support
        task_monitor = context.task_monitor if context else None

        # show_spawn_header=False because react_executor already showed the Spawn[] header
        # via on_tool_call before calling this tool handler
        result = self._subagent_manager.execute_subagent(
            name=subagent_type,
            task=task,
            deps=deps,
            ui_callback=ui_callback,
            task_monitor=task_monitor,
            show_spawn_header=False,
            tool_call_id=tool_call_id,  # Pass for parent context tracking
        )

        # Save subagent conversation as a child session for navigation (Ctrl+G)
        self._save_subagent_session(
            result, subagent_type, tool_call_id, context,
        )

        # Format output for consistency
        if result.get("success"):
            content = result.get("content", "")
            # Always set completion_status for sync subagents (they complete immediately)
            # This helps the LLM understand that results are already included
            completion_status = result.get("completion_status", "success")
            response = {
                "success": True,
                "output": "[SYNC COMPLETE] Subagent finished. Results included below.",
                "separate_response": content,  # Show as separate assistant message
                "subagent_type": subagent_type,
                "completion_status": completion_status,  # Always include for sync completions
            }
            return response
        else:
            # Check both "error" and "content" fields for error message
            # MainAgent.run_sync() puts errors in "content", not "error"
            error = result.get("error") or result.get("content") or "Unknown error"
            return {
                "success": False,
                "error": f"[{subagent_type}] {error}",
                "output": None,
                "interrupted": result.get("interrupted", False),  # Propagate interrupt flag
            }

    def _save_subagent_session(
        self,
        result: dict[str, Any],
        subagent_type: str,
        tool_call_id: Union[str, None],
        context: Any,
    ) -> None:
        """Save subagent conversation as a child session and record mapping.

        Creates a new session from the subagent's messages and stores a
        tool_call_id -> child_session_id mapping in the parent session's
        subagent_sessions field for later navigation (Ctrl+G).

        Args:
            result: Result dict from execute_subagent (contains 'messages')
            subagent_type: Name of the subagent type
            tool_call_id: Tool call ID from the parent context
            context: Tool execution context with session_manager
        """
        if tool_call_id is None:
            return

        subagent_messages = result.get("messages")
        if not subagent_messages:
            return

        session_manager = getattr(context, "session_manager", None) if context else None
        if session_manager is None:
            return

        parent_session = session_manager.get_current_session()
        if parent_session is None:
            return

        try:
            from opendev.models.message import ChatMessage, Role
            from opendev.models.session import Session

            # Create a child session for the subagent conversation
            child_session = Session(
                working_directory=parent_session.working_directory,
                parent_id=parent_session.id,
                metadata={"title": f"Subagent: {subagent_type}"},
            )

            # Convert raw message dicts to ChatMessage objects
            valid_roles = {r.value for r in Role}
            for msg in subagent_messages:
                if isinstance(msg, dict):
                    role_str = msg.get("role", "user")
                    content = msg.get("content", "")
                    # Skip system and tool messages (prompt and tool results)
                    if role_str not in valid_roles or role_str == "system":
                        continue
                    child_session.add_message(
                        ChatMessage(role=Role(role_str), content=str(content) if content else "")
                    )

            # Save child session
            session_manager.save_session(child_session)

            # Record mapping in parent session
            if not hasattr(parent_session, "subagent_sessions"):
                parent_session.subagent_sessions = {}
            parent_session.subagent_sessions[tool_call_id] = child_session.id
            session_manager.save_session(parent_session)
        except Exception:
            # Non-critical — don't break subagent execution if session save fails
            logger.debug("Failed to save subagent session", exc_info=True)

    def _get_subagent_output(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Get output from a background subagent task.

        Args:
            arguments: Tool arguments with 'task_id', optional 'block' and 'timeout'
            context: Tool execution context

        Returns:
            Result from background subagent or status information
        """
        task_id = arguments.get("task_id", "")
        block = arguments.get("block", True)
        timeout = arguments.get("timeout", 30000)

        if not task_id:
            return {
                "success": False,
                "error": "task_id is required",
                "output": None,
            }

        if not self._subagent_manager:
            return {
                "success": False,
                "error": "SubAgentManager not configured",
                "output": None,
            }

        # Check if manager has background task support
        if hasattr(self._subagent_manager, "get_background_task_output"):
            return self._subagent_manager.get_background_task_output(
                task_id, block=block, timeout=timeout
            )

        # Fallback for managers without background support
        return {
            "success": False,
            "error": f"Background task support not available. Task ID '{task_id}' not found.",
            "output": "Background subagent execution is not yet fully implemented. "
            "Subagents currently run synchronously.",
        }

    def get_schemas(self) -> list[dict[str, Any]]:
        """Compatibility hook (schemas generated elsewhere)."""
        return []

    def execute_tool(
        self,
        tool_name: str,
        arguments: dict[str, Any],
        *,
        mode_manager: Union[Any, None] = None,
        approval_manager: Union[Any, None] = None,
        undo_manager: Union[Any, None] = None,
        task_monitor: Union[Any, None] = None,
        session_manager: Union[Any, None] = None,
        ui_callback: Union[Any, None] = None,
        is_subagent: bool = False,
        tool_call_id: Union[str, None] = None,
    ) -> dict[str, Any]:
        """Execute a tool by delegating to registered handlers."""
        if tool_name.startswith("mcp__"):
            # Auto-discover MCP tools when they're called directly
            # This ensures the tool schema will be available in future LLM calls
            if tool_name not in self._discovered_mcp_tools:
                self.discover_mcp_tool(tool_name)
                logger.info(
                    f"Auto-discovered MCP tool: {tool_name}. "
                    "Tip: Use search_tools() to discover tools before using them."
                )
            return self._mcp_handler.execute(tool_name, arguments, task_monitor=task_monitor)

        if tool_name not in self._handlers:
            return {"success": False, "error": f"Unknown tool: {tool_name}", "output": None}

        # --- PreToolUse hook ---
        if self._hook_manager:
            from opendev.core.hooks.models import HookEvent

            if self._hook_manager.has_hooks_for(HookEvent.PRE_TOOL_USE):
                outcome = self._hook_manager.run_hooks(
                    HookEvent.PRE_TOOL_USE,
                    match_value=tool_name,
                    event_data={"tool_input": arguments},
                )
                if outcome.blocked:
                    return {
                        "success": False,
                        "error": f"Blocked by hook: {outcome.block_reason}",
                        "output": None,
                        "denied": True,
                    }
                if outcome.updated_input and isinstance(outcome.updated_input, dict):
                    arguments = {**arguments, **outcome.updated_input}

        context = ToolExecutionContext(
            mode_manager=mode_manager,
            approval_manager=approval_manager,
            undo_manager=undo_manager,
            task_monitor=task_monitor,
            session_manager=session_manager,
            ui_callback=ui_callback,
            is_subagent=is_subagent,
            file_time_tracker=self._file_time_tracker,
        )

        handler = self._handlers[tool_name]
        try:
            if tool_name == "spawn_subagent":
                # spawn_subagent needs tool_call_id for parent context tracking
                result = self._execute_spawn_subagent(arguments, context, tool_call_id)
            elif tool_name in {
                "write_file", "edit_file", "read_file",
                "run_command", "batch_tool", "present_plan",
            }:
                # Handlers requiring context
                result = handler(arguments, context)
            elif tool_name == "list_processes":
                result = handler(arguments, context)
            elif tool_name in {"get_process_output", "kill_process"}:
                result = handler(arguments)
            else:
                # Remaining handlers ignore execution context
                result = handler(arguments)
        except Exception as exc:  # noqa: BLE001
            if isinstance(exc, InterruptedError):
                raise
            import traceback as _tb

            logger.error("Tool execution failed: %s\n%s", exc, _tb.format_exc())
            result = {"success": False, "error": str(exc), "output": None}

        # --- PostToolUse / PostToolUseFailure hook ---
        if self._hook_manager:
            from opendev.core.hooks.models import HookEvent

            is_success = result.get("success", False)
            post_event = HookEvent.POST_TOOL_USE if is_success else HookEvent.POST_TOOL_USE_FAILURE
            if self._hook_manager.has_hooks_for(post_event):
                self._hook_manager.run_hooks_async(
                    post_event,
                    match_value=tool_name,
                    event_data={
                        "tool_input": arguments,
                        "tool_response": result,
                    },
                )

        return result

    def set_mcp_manager(self, mcp_manager: Union[Any, None]) -> None:
        """Update the MCP manager and refresh the handlers."""
        self.mcp_manager = mcp_manager
        self._mcp_handler = McpToolHandler(mcp_manager)
        self._search_tools_handler.set_mcp_manager(mcp_manager)

    def _open_browser(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Execute the open_browser tool."""
        if not self.open_browser_tool:
            return {
                "success": False,
                "error": "open_browser tool not available",
                "output": None,
            }
        return self.open_browser_tool.execute(**arguments)

    def _analyze_image(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Execute the analyze_image tool (VLM)."""
        if not self.vlm_tool:
            return {
                "success": False,
                "error": "VLM tool not available",
                "output": None,
            }
        # Handle max_completion_tokens -> max_tokens conversion (OpenAI models use different param)
        if "max_completion_tokens" in arguments:
            arguments["max_tokens"] = arguments.pop("max_completion_tokens")
        result = self.vlm_tool.analyze_image(**arguments)
        # Format output for consistency with other tools
        if result.get("success"):
            return {
                "success": True,
                "output": result.get("content", ""),
                "model": result.get("model"),
                "provider": result.get("provider"),
            }
        else:
            return {
                "success": False,
                "error": result.get("error", "Unknown error"),
                "output": None,
            }

    def _capture_web_screenshot(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Execute the capture_web_screenshot tool."""
        if not self.web_screenshot_tool:
            return {
                "success": False,
                "error": "Web screenshot tool not available",
                "output": None,
            }
        result = self.web_screenshot_tool.capture_web_screenshot(**arguments)
        # Format output for consistency
        if result.get("success"):
            output_lines = [
                f"Screenshot captured: {result.get('screenshot_path')}",
                f"URL: {result.get('url')}",
            ]
            if result.get("pdf_path"):
                output_lines.append(f"PDF captured: {result.get('pdf_path')}")
            if result.get("warning"):
                output_lines.append(f"Warning: {result['warning']}")
            if result.get("pdf_warning"):
                output_lines.append(f"PDF Warning: {result['pdf_warning']}")

            response = {
                "success": True,
                "output": "\n".join(output_lines),
                "screenshot_path": result.get("screenshot_path"),
            }
            if result.get("pdf_path"):
                response["pdf_path"] = result.get("pdf_path")
            return response
        else:
            return {
                "success": False,
                "error": result.get("error", "Unknown error"),
                "output": None,
            }

    def _write_todos(self, arguments: dict[str, Any], context: Any = None) -> dict[str, Any]:
        """Execute the write_todos tool."""
        return self.todo_handler.write_todos(arguments.get("todos", []))

    def _update_todo(self, arguments: dict[str, Any], context: Any = None) -> dict[str, Any]:
        """Execute the update_todo tool."""
        return self.todo_handler.update_todo(
            id=arguments.get("id"),
            status=arguments.get("status"),
            title=arguments.get("title"),
        )

    def _complete_todo(self, arguments: dict[str, Any], context: Any = None) -> dict[str, Any]:
        """Execute the complete_todo tool."""
        return self.todo_handler.complete_todo(id=arguments.get("id"))

    def _read_pdf(self, arguments: dict[str, Any]) -> dict[str, Any]:
        """Execute the read_pdf tool to extract text from a PDF file.

        Args:
            arguments: Dict with 'file_path' key

        Returns:
            Result with extracted text content and metadata
        """
        file_path = arguments.get("file_path", "")
        if not file_path:
            return {
                "success": False,
                "error": "file_path is required for read_pdf",
                "output": None,
            }

        result = self._pdf_tool.extract_text(file_path)

        if result.get("success"):
            # Format output for display
            content = result.get("content", "")
            metadata = result.get("metadata", {})
            page_count = result.get("page_count", 0)
            sections = result.get("sections", [])

            output_parts = []
            if metadata:
                if metadata.get("title"):
                    output_parts.append(f"Title: {metadata['title']}")
                if metadata.get("author"):
                    output_parts.append(f"Author: {metadata['author']}")
            output_parts.append(f"Pages: {page_count}")
            if sections:
                output_parts.append(f"Detected sections: {len(sections)}")
                section_titles = [s.get("title", "") for s in sections[:10]]
                output_parts.append(f"  {', '.join(section_titles)}")

            output_parts.append("\n--- Content ---\n")
            output_parts.append(content)

            return {
                "success": True,
                "output": "\n".join(output_parts),
                "metadata": metadata,
                "page_count": page_count,
                "sections": sections,
            }
        else:
            return {
                "success": False,
                "error": result.get("error", "Unknown error"),
                "output": None,
            }

    def _execute_task_complete(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Execute the task_complete tool to signal explicit task completion.

        Args:
            arguments: Dict with 'summary' (required) and 'status' keys
            context: Tool execution context (unused)

        Returns:
            Result with _completion flag for loop termination
        """
        summary = arguments.get("summary", "")
        status = arguments.get("status", "success")

        return self._task_complete_tool.execute(summary=summary, status=status)

    def _execute_present_plan(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Execute the present_plan tool to present plan for approval.

        Args:
            arguments: Dict with 'plan_file_path' key.
            context: Tool execution context.

        Returns:
            Result indicating approval status.
        """
        kwargs: dict[str, Any] = {
            "plan_file_path": arguments.get("plan_file_path", ""),
        }
        if context:
            kwargs["ui_callback"] = getattr(context, "ui_callback", None)
            kwargs["session_manager"] = getattr(context, "session_manager", None)

        result = self._present_plan_tool.execute(**kwargs)

        # Set autonomy to AUTO if user chose "Start implementation (auto-approve)"
        if result.get("auto_approve") and context:
            approval_manager = getattr(context, "approval_manager", None)
            if approval_manager and hasattr(approval_manager, "set_autonomy_level"):
                approval_manager.set_autonomy_level("Auto")

        # Auto-create todos from plan steps
        if result.get("plan_approved"):
            plan_content = result.get("plan_content", "")
            if plan_content:
                self._create_todos_from_plan(plan_content, result)

        return result

    def _create_todos_from_plan(self, plan_content: str, result: dict[str, Any]) -> None:
        """Parse plan and create todos from implementation steps.

        Args:
            plan_content: Raw plan text.
            result: The present_plan result dict to augment with todo count.
        """
        from opendev.core.agents.components.response.plan_parser import parse_plan

        # Ensure content has delimiters (safety net)
        if "---BEGIN PLAN---" not in plan_content:
            plan_content = f"---BEGIN PLAN---\n{plan_content}\n---END PLAN---"

        parsed = parse_plan(plan_content)
        if parsed and parsed.steps:
            todos = parsed.get_todo_items()
            todo_result = self.todo_handler.write_todos(todos)
            if todo_result.get("success"):
                count = todo_result.get("created_count", len(todos))
                result["todos_created"] = count
                result["output"] += f"\n\nCreated {count} implementation todos."

    def _execute_batch_tool(self, arguments: dict[str, Any], context: Any = None) -> dict[str, Any]:
        """Execute the batch_tool for parallel/serial multi-tool invocations.

        Args:
            arguments: Dict with 'invocations' list and optional 'mode'
            context: Tool execution context

        Returns:
            Result with list of tool outputs
        """
        if not self._batch_handler:
            return {"success": False, "error": "Batch handler not initialized", "results": []}

        # Pass context-related kwargs for tool execution
        kwargs: dict[str, Any] = {}
        if context:
            kwargs["mode_manager"] = getattr(context, "mode_manager", None)
            kwargs["approval_manager"] = getattr(context, "approval_manager", None)
            kwargs["undo_manager"] = getattr(context, "undo_manager", None)
            kwargs["task_monitor"] = getattr(context, "task_monitor", None)
            kwargs["session_manager"] = getattr(context, "session_manager", None)
            kwargs["ui_callback"] = getattr(context, "ui_callback", None)

        return self._batch_handler.handle(arguments, **kwargs)

    def _handle_invoke_skill(
        self, arguments: dict[str, Any], context: Any = None
    ) -> dict[str, Any]:
        """Execute the invoke_skill tool to load skill content into context.

        Args:
            arguments: Dict with 'skill_name' key
            context: Tool execution context (unused)

        Returns:
            Result with skill content or error
        """
        if not self._skill_loader:
            return {
                "success": False,
                "error": "Skills system not configured. invoke_skill tool unavailable.",
                "output": None,
            }

        skill_name = arguments.get("skill_name", "")
        if not skill_name:
            # List available skills if no name provided
            available = self._skill_loader.get_skill_names()
            return {
                "success": True,
                "output": f"Available skills: {', '.join(available) if available else 'None'}",
            }

        skill = self._skill_loader.load_skill(skill_name)
        if not skill:
            available = self._skill_loader.get_skill_names()
            return {
                "success": False,
                "error": f"Skill not found: '{skill_name}'. Available: {', '.join(available) if available else 'None'}",
                "output": None,
            }

        # Dedup: if already invoked this session, return a short reminder
        if skill_name in self._invoked_skills:
            return {
                "success": True,
                "output": (
                    f"Skill '{skill.metadata.name}' is already loaded in this conversation. "
                    "Refer to the skill content above and proceed with the next action step — "
                    "do not invoke this skill again."
                ),
                "skill_name": skill.metadata.name,
                "skill_namespace": skill.metadata.namespace,
            }

        self._invoked_skills.add(skill_name)
        return {
            "success": True,
            "output": f"Loaded skill: {skill.metadata.name}\n\n{skill.content}",
            "skill_name": skill.metadata.name,
            "skill_namespace": skill.metadata.namespace,
        }
