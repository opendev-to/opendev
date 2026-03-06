"""Execution mixin for SubAgentManager."""

from __future__ import annotations

import asyncio
import logging
from pathlib import Path
from typing import TYPE_CHECKING, Any

from opendev.core.agents.prompts import get_reminder

if TYPE_CHECKING:
    from opendev.models.config import AppConfig

    from opendev.core.agents.subagents.manager.manager import SubAgentDeps
    from opendev.core.agents.subagents.specs import CompiledSubAgent

logger = logging.getLogger(__name__)


class ExecutionMixin:
    """Mixin providing subagent execution, ask-user handling, and async/parallel support."""

    # Declared for type checking — set by SubAgentManager.__init__
    _config: AppConfig
    _tool_registry: Any
    _mode_manager: Any
    _working_dir: Any
    _env_context: Any
    _hook_manager: Any
    _agents: dict[str, CompiledSubAgent]
    _all_tool_names: list[str]

    def execute_subagent(
        self,
        name: str,
        task: str,
        deps: SubAgentDeps,
        ui_callback: Any = None,
        task_monitor: Any = None,
        working_dir: Any = None,
        docker_handler: Any = None,
        path_mapping: dict[str, str] | None = None,
        show_spawn_header: bool = True,
        tool_call_id: str | None = None,
    ) -> dict[str, Any]:
        """Execute a subagent synchronously with the given task.

        Args:
            name: The subagent type name
            task: The task description for the subagent
            deps: Dependencies for tool execution
            ui_callback: Optional UI callback for displaying tool calls
            task_monitor: Optional task monitor for interrupt support
            working_dir: Optional working directory override for the subagent
            docker_handler: Optional DockerToolHandler for Docker-based execution.
                           When provided, all tool calls are routed through Docker
                           instead of local execution.
            path_mapping: Mapping of Docker paths to local paths for local-only tools.
                         Used to remap paths when tools like read_pdf run locally.
            show_spawn_header: Whether to show the Spawn[] header. Set to False when
                              called via tool_registry (react_executor already showed it).
            tool_call_id: Optional unique tool call ID for parent context tracking.
                         When provided, used as parent_context in NestedUICallback
                         to enable individual agent tracking in parallel display.

        Returns:
            Result dict with content, success, and messages
        """
        # Fire SubagentStart hook
        if self._hook_manager:
            from opendev.core.hooks.models import HookEvent

            if self._hook_manager.has_hooks_for(HookEvent.SUBAGENT_START):
                outcome = self._hook_manager.run_hooks(
                    HookEvent.SUBAGENT_START,
                    match_value=name,
                    event_data={"agent_task": task},
                )
                if outcome.blocked:
                    return {
                        "success": False,
                        "error": f"Blocked by hook: {outcome.block_reason}",
                        "content": "",
                    }

        # SPECIAL CASE: ask-user subagent
        # This is a built-in that shows UI panel instead of running LLM
        if name == "ask-user":
            return self._execute_ask_user(task, ui_callback)

        # Auto-detect Docker execution for subagents with docker_config
        # Only trigger if docker_handler is not already provided (to avoid recursion)
        if docker_handler is None:
            spec = self._get_spec_for_subagent(name)
            if spec is not None and spec.get("docker_config") is not None:
                if self._is_docker_available():
                    # Execute with Docker lifecycle management
                    return self._execute_with_docker(
                        name=name,
                        task=task,
                        deps=deps,
                        spec=spec,
                        ui_callback=ui_callback,
                        task_monitor=task_monitor,
                        show_spawn_header=show_spawn_header,
                        local_output_dir=Path(working_dir) if working_dir else None,
                    )
                # If Docker not available, fall through to local execution

        if name not in self._agents:
            available = ", ".join(self._agents.keys())
            return {
                "success": False,
                "error": f"Unknown subagent type '{name}'. Available: {available}",
                "content": "",
            }

        compiled = self._agents[name]

        # Note: UI callback notifications for single agents are handled by
        # TextualUICallback.on_tool_call() and on_tool_result() for spawn_subagent

        # Determine which tool registry to use
        if docker_handler is not None:
            # Use Docker-based tool registry for Docker execution
            # Pass local registry for fallback on tools not supported in Docker (e.g., read_pdf)
            # Pass path_mapping to remap Docker paths to local paths for local-only tools
            from opendev.core.docker.tool_handler import DockerToolRegistry

            tool_registry = DockerToolRegistry(
                docker_handler,
                local_registry=self._tool_registry,
                path_mapping=path_mapping,
            )
        else:
            tool_registry = self._tool_registry

        # If working_dir or docker_handler requires a new agent instance
        if working_dir is not None or docker_handler is not None:
            from opendev.core.agents import MainAgent
            from opendev.core.agents.subagents.agents import ALL_SUBAGENTS

            # Find the spec for this subagent
            spec = next((s for s in ALL_SUBAGENTS if s["name"] == name), None)
            if spec is None:
                return {
                    "success": False,
                    "error": f"Spec not found for subagent '{name}'",
                    "content": "",
                }

            allowed_tools = spec.get("tools", self._all_tool_names)

            agent = MainAgent(
                config=self._get_subagent_config(spec),
                tool_registry=tool_registry,
                mode_manager=self._mode_manager,
                working_dir=working_dir if working_dir is not None else self._working_dir,
                allowed_tools=allowed_tools,
                env_context=self._env_context,
            )

            # Apply system prompt override
            if spec.get("system_prompt"):
                base_prompt = spec["system_prompt"]
                # When running in Docker, inject Docker context into system prompt
                if docker_handler is not None:
                    docker_preamble = get_reminder(
                        "docker/docker_preamble", working_dir=working_dir
                    )
                    agent.system_prompt = docker_preamble + "\n\n" + base_prompt
                else:
                    agent.system_prompt = base_prompt
        else:
            agent = compiled["agent"]
            allowed_tools = compiled["tool_names"]
            # Apply the subagent's specialized system prompt
            if hasattr(agent, "_subagent_system_prompt") and agent._subagent_system_prompt:
                agent.system_prompt = agent._subagent_system_prompt

        # Create nested callback wrapper if parent callback provided
        # If ui_callback is already a NestedUICallback, use it directly (avoids double-wrapping)
        # For Docker subagents, caller should use create_docker_nested_callback() first
        nested_callback = None
        if ui_callback is not None:
            from opendev.ui_textual.nested_callback import NestedUICallback

            if isinstance(ui_callback, NestedUICallback):
                # Already nested (e.g., from create_docker_nested_callback), use directly
                nested_callback = ui_callback
            else:
                # Wrap in NestedUICallback for proper nesting display
                # Use tool_call_id as parent_context for individual agent tracking
                # in parallel display (falls back to name for single agent calls)
                # No path_sanitizer for local subagents - Docker subagents should
                # use create_docker_nested_callback() before calling execute_subagent()
                import sys

                print(
                    f"[DEBUG MANAGER] Creating NestedUICallback: tool_call_id={tool_call_id!r}, name={name!r}, parent_context={tool_call_id or name!r}",
                    file=sys.stderr,
                )
                nested_callback = NestedUICallback(
                    parent_callback=ui_callback,
                    parent_context=tool_call_id or name,
                    depth=1,
                )

        # Execute with isolated context (fresh message history)
        # No iteration cap — subagent stops when its prompt tells it to
        result = agent.run_sync(
            message=task,
            deps=deps,
            message_history=None,  # Fresh context for subagent
            ui_callback=nested_callback,
            max_iterations=None,
            task_monitor=task_monitor,  # Pass task monitor for interrupt support
        )

        # Fire SubagentStop hook
        if self._hook_manager:
            from opendev.core.hooks.models import HookEvent

            if self._hook_manager.has_hooks_for(HookEvent.SUBAGENT_STOP):
                self._hook_manager.run_hooks_async(
                    HookEvent.SUBAGENT_STOP,
                    match_value=name,
                    event_data={
                        "agent_result": {
                            "success": result.get("success", False),
                        },
                    },
                )

        # Note: UI callback completion notification is handled by
        # TextualUICallback.on_tool_result() for spawn_subagent

        return result

    def _execute_ask_user(
        self,
        task: str,
        ui_callback: Any,
    ) -> dict[str, Any]:
        """Execute the ask-user built-in subagent.

        This is a special subagent that shows a UI panel for user input
        instead of running an LLM. It parses questions from the task JSON
        and displays them in an interactive panel.

        Args:
            task: JSON string containing questions (from spawn_subagent prompt)
            ui_callback: UI callback with access to app

        Returns:
            Result dict with user's answers
        """
        import json

        # Parse questions from task (JSON string)
        try:
            questions_data = json.loads(task)
            questions = self._parse_ask_user_questions(questions_data.get("questions", []))
        except json.JSONDecodeError:
            return {
                "success": False,
                "error": "Invalid questions format - expected JSON",
                "content": "",
            }

        if not questions:
            return {
                "success": False,
                "error": "No questions provided",
                "content": "",
            }

        # Get app reference from ui_callback
        app = getattr(ui_callback, "chat_app", None) or getattr(ui_callback, "_app", None)
        if app is None:
            # Try to get app from nested callback parent
            parent = getattr(ui_callback, "_parent_callback", None)
            if parent:
                app = getattr(parent, "chat_app", None) or getattr(parent, "_app", None)

        if app is None:
            return {
                "success": False,
                "error": "UI app not available for ask-user",
                "content": "",
            }

        # Show panel and wait for user response using call_from_thread pattern
        # (similar to approval_manager.py)
        import threading

        if not hasattr(app, "call_from_thread") or not getattr(app, "is_running", False):
            return {
                "success": False,
                "error": "UI app not available or not running for ask-user",
                "content": "",
            }

        done_event = threading.Event()
        result_holder: dict[str, Any] = {"answers": None, "error": None}

        def invoke_panel() -> None:
            async def run_panel() -> None:
                try:
                    result_holder["answers"] = await app._ask_user_controller.start(questions)
                except Exception as exc:
                    result_holder["error"] = exc
                finally:
                    done_event.set()

            app.run_worker(
                run_panel(),
                name="ask-user-panel",
                exclusive=True,
                exit_on_error=False,
            )

        try:
            app.call_from_thread(invoke_panel)

            # Wait for user response with timeout
            if not done_event.wait(timeout=600):  # 10 min timeout
                return {
                    "success": False,
                    "error": "Ask user timed out",
                    "content": "",
                }

            if result_holder["error"]:
                raise result_holder["error"]

            answers = result_holder["answers"]
        except Exception as e:
            logger.exception("Ask user failed")
            return {
                "success": False,
                "error": f"Ask user failed: {e}",
                "content": "",
            }

        if answers is None:
            return {
                "success": True,
                "content": "User cancelled/skipped the question(s).",
                "answers": {},
                "cancelled": True,
            }

        # Format answers for agent consumption (compact single line for clean UI display)
        # Get headers from original questions for better formatting
        answer_parts = []
        for idx, ans in answers.items():
            if isinstance(ans, list):
                ans_text = ", ".join(str(a) for a in ans)
            else:
                ans_text = str(ans)
            # Try to get header from question, fall back to Q#
            q_idx = int(idx) if idx.isdigit() else 0
            header = f"Q{q_idx + 1}"
            if q_idx < len(questions):
                q = questions[q_idx]
                if hasattr(q, "header") and q.header:
                    header = q.header
            answer_parts.append(f"[{header}]={ans_text}")

        total = len(questions)
        answered = len(answers)
        answer_summary = ", ".join(answer_parts) if answer_parts else "No answers"

        return {
            "success": True,
            "content": f"Received {answered}/{total} answers: {answer_summary}",
            "answers": answers,
            "cancelled": False,
        }

    def _parse_ask_user_questions(self, questions_data: list) -> list:
        """Parse question dicts into Question objects.

        Args:
            questions_data: List of question dictionaries from JSON

        Returns:
            List of Question objects
        """
        from opendev.core.context_engineering.tools.implementations.ask_user_tool import (
            Question,
            QuestionOption,
        )

        questions = []
        for q in questions_data:
            if not isinstance(q, dict):
                continue

            options = []
            for opt in q.get("options", []):
                if isinstance(opt, dict):
                    options.append(
                        QuestionOption(
                            label=opt.get("label", ""),
                            description=opt.get("description", ""),
                        )
                    )
                else:
                    options.append(QuestionOption(label=str(opt)))

            if options:
                questions.append(
                    Question(
                        question=q.get("question", ""),
                        header=q.get("header", "")[:12],
                        options=options,
                        multi_select=q.get("multiSelect", False),
                    )
                )
        return questions

    async def execute_subagent_async(
        self,
        name: str,
        task: str,
        deps: SubAgentDeps,
        ui_callback: Any = None,
    ) -> dict[str, Any]:
        """Execute a subagent asynchronously.

        Uses asyncio.to_thread to run the synchronous agent in a thread pool.

        Args:
            name: The subagent type name
            task: The task description for the subagent
            deps: Dependencies for tool execution
            ui_callback: Optional UI callback for displaying tool calls

        Returns:
            Result dict with content, success, and messages
        """
        return await asyncio.to_thread(self.execute_subagent, name, task, deps, ui_callback)

    async def execute_parallel(
        self,
        tasks: list[tuple[str, str]],
        deps: SubAgentDeps,
        ui_callback: Any = None,
    ) -> list[dict[str, Any]]:
        """Execute multiple subagents in parallel.

        Args:
            tasks: List of (subagent_name, task_description) tuples
            deps: Dependencies for tool execution
            ui_callback: Optional UI callback for displaying tool calls

        Returns:
            List of results from each subagent
        """
        # 1. Notify start of parallel execution
        agent_names = [name for name, _ in tasks]
        if ui_callback and hasattr(ui_callback, "on_parallel_agents_start"):
            ui_callback.on_parallel_agents_start(agent_names)

        # 2. Execute in parallel with completion tracking
        async def execute_with_tracking(name: str, task: str) -> dict[str, Any]:
            """Execute a single subagent and report completion."""
            result = await self.execute_subagent_async(name, task, deps, ui_callback)
            success = result.get("success", True) if isinstance(result, dict) else True
            if ui_callback and hasattr(ui_callback, "on_parallel_agent_complete"):
                ui_callback.on_parallel_agent_complete(name, success)
            return result

        coroutines = [execute_with_tracking(name, task) for name, task in tasks]
        results = await asyncio.gather(*coroutines)

        # 3. Notify completion of all parallel agents
        if ui_callback and hasattr(ui_callback, "on_parallel_agents_done"):
            ui_callback.on_parallel_agents_done()

        return results
