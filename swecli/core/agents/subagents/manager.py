"""SubAgent manager for creating and executing subagents."""

from __future__ import annotations

import asyncio
import logging
import re
import shutil
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any

from swecli.core.agents.prompts import get_reminder
from swecli.models.config import AppConfig

from .specs import CompiledSubAgent, SubAgentSpec

logger = logging.getLogger(__name__)


class AgentSource(str, Enum):
    """Source of an agent definition."""

    BUILTIN = "builtin"
    USER_GLOBAL = "user-global"
    PROJECT = "project"


@dataclass
class AgentConfig:
    """Configuration for an agent (builtin or custom).

    Used for building Task tool descriptions and on-demand prompt assembly.
    """

    name: str
    description: str
    tools: list[str] | str | dict[str, list[str]] = field(default_factory=list)
    system_prompt: str | None = None
    skill_path: str | None = None  # For custom agents
    source: AgentSource = AgentSource.BUILTIN
    model: str | None = None

    def get_tool_list(self, all_tools: list[str]) -> list[str]:
        """Resolve tool specification to concrete list.

        Args:
            all_tools: List of all available tool names

        Returns:
            Resolved list of tool names for this agent
        """
        if self.tools == "*":
            return all_tools
        if isinstance(self.tools, list):
            return self.tools if self.tools else all_tools
        if isinstance(self.tools, dict) and "exclude" in self.tools:
            excluded = set(self.tools["exclude"])
            return [t for t in all_tools if t not in excluded]
        return all_tools


@dataclass
class SubAgentDeps:
    """Dependencies for subagent execution."""

    mode_manager: Any
    approval_manager: Any
    undo_manager: Any


class SubAgentManager:
    """Manages subagent creation and execution.

    SubAgents are ephemeral agents that handle isolated tasks.
    They receive a task description, execute with their own context,
    and return a single result.
    """

    def __init__(
        self,
        config: AppConfig,
        tool_registry: Any,
        mode_manager: Any,
        working_dir: Any = None,
        env_context: Any = None,
    ) -> None:
        """Initialize the SubAgentManager.

        Args:
            config: Application configuration
            tool_registry: The tool registry for tool execution
            mode_manager: Mode manager for operation mode
            working_dir: Working directory for file operations
            env_context: Optional EnvironmentContext for rich system prompt
        """
        self._config = config
        self._tool_registry = tool_registry
        self._mode_manager = mode_manager
        self._working_dir = working_dir
        self._env_context = env_context
        self._hook_manager = None
        self._agents: dict[str, CompiledSubAgent] = {}
        self._all_tool_names: list[str] = self._get_all_tool_names()

    def set_hook_manager(self, hook_manager: Any) -> None:
        """Set the hook manager for SubagentStart/SubagentStop hooks.

        Args:
            hook_manager: HookManager instance
        """
        self._hook_manager = hook_manager

    def _get_all_tool_names(self) -> list[str]:
        """Get list of all available tool names from registry.

        Note: Todo tools (write_todos, update_todo, etc.) are intentionally
        excluded. Only the main agent manages task tracking - subagents
        focus purely on execution.
        """
        return [
            "read_file",
            "write_file",
            "edit_file",
            "list_files",
            "search",
            "run_command",
            "list_processes",
            "get_process_output",
            "kill_process",
            "fetch_url",
            "analyze_image",
            "capture_screenshot",
            "list_screenshots",
            "capture_web_screenshot",
            "read_pdf",
        ]

    def register_subagent(self, spec: SubAgentSpec) -> None:
        """Register a subagent from specification.

        Args:
            spec: The subagent specification
        """
        from swecli.core.agents import MainAgent

        # Create a filtered tool registry if tools are specified
        tool_names = spec.get("tools", self._all_tool_names)

        # Create the subagent instance with tool filtering
        agent = MainAgent(
            config=self._get_subagent_config(spec),
            tool_registry=self._tool_registry,
            mode_manager=self._mode_manager,
            working_dir=self._working_dir,
            allowed_tools=tool_names,  # Pass tool filtering to agent
            env_context=self._env_context,
        )

        # Override system prompt for subagent
        agent._subagent_system_prompt = spec["system_prompt"]

        self._agents[spec["name"]] = CompiledSubAgent(
            name=spec["name"],
            description=spec["description"],
            agent=agent,
            tool_names=tool_names,
        )

    def _get_subagent_config(self, spec: SubAgentSpec) -> AppConfig:
        """Create config for subagent, potentially with model override."""
        if "model" in spec and spec["model"]:
            # Create a copy with model override
            return AppConfig(
                model=spec["model"],
                temperature=self._config.temperature,
                max_tokens=self._config.max_tokens,
                api_key=self._config.api_key,
                api_base_url=self._config.api_base_url,
            )
        return self._config

    def register_defaults(self) -> None:
        """Register all default subagents."""
        from .agents import ALL_SUBAGENTS

        for spec in ALL_SUBAGENTS:
            self.register_subagent(spec)

    def get_agent_configs(self) -> list[AgentConfig]:
        """Get all agent configurations for Task tool description.

        Returns:
            List of AgentConfig for all registered agents (builtin and custom)
        """
        from .agents import ALL_SUBAGENTS

        configs = []
        for spec in ALL_SUBAGENTS:
            config = AgentConfig(
                name=spec["name"],
                description=spec["description"],
                tools=spec.get("tools", []),
                system_prompt=spec.get("system_prompt"),
                source=AgentSource.BUILTIN,
                model=spec.get("model"),
            )
            configs.append(config)

        # Include custom agents (added via register_custom_agents)
        for name, compiled in self._agents.items():
            # Skip if already in configs (builtin)
            if any(c.name == name for c in configs):
                continue
            # This is a custom agent
            config = AgentConfig(
                name=name,
                description=compiled["description"],
                tools=compiled.get("tool_names", []),
                source=AgentSource.USER_GLOBAL,  # Will be updated by register_custom_agents
            )
            configs.append(config)

        return configs

    def build_task_tool_description(self) -> str:
        """Build spawn_subagent tool description from registered agents.

        Returns:
            Formatted description string for the spawn_subagent tool
        """
        lines = [
            "Spawn a specialized subagent to handle a specific task.",
            "",
            "Available agent types:",
        ]
        for config in self.get_agent_configs():
            lines.append(f"- **{config.name}**: {config.description}")
        lines.append("")
        lines.append("Use this tool when you need specialized capabilities or ")
        lines.append("want to delegate complex tasks to a focused agent.")
        return "\n".join(lines)

    def get_subagent(self, name: str) -> CompiledSubAgent | None:
        """Get a registered subagent by name.

        Args:
            name: The subagent name

        Returns:
            The compiled subagent or None if not found
        """
        return self._agents.get(name)

    def get_available_types(self) -> list[str]:
        """Get list of available subagent type names.

        Returns:
            List of registered subagent names
        """
        return list(self._agents.keys())

    def get_descriptions(self) -> dict[str, str]:
        """Get descriptions for all registered subagents.

        Returns:
            Dict mapping subagent name to description
        """
        return {name: agent["description"] for name, agent in self._agents.items()}

    def register_custom_agents(self, custom_agents: list[dict]) -> None:
        """Register custom agents from config files.

        Custom agents can be defined in:
        - ~/.opendev/agents.json or <project>/.opendev/agents.json (JSON format)
        - ~/.opendev/agents/*.md or <project>/.opendev/agents/*.md (Claude Code markdown format)

        Each agent definition can specify:
        - name: Unique agent name (required)
        - description: Human-readable description (optional)
        - tools: List of tool names, "*" for all, or {"exclude": [...]} (optional)
        - skillPath: Path to skill file to use as system prompt (optional, JSON format)
        - _system_prompt: Direct system prompt content (markdown format)
        - model: Model override for this agent (optional)

        Args:
            custom_agents: List of agent definitions from config files
        """
        for agent_def in custom_agents:
            name = agent_def.get("name")
            if not name:
                logger.warning("Skipping custom agent without name")
                continue

            # Skip if already registered (builtin takes priority)
            if name in self._agents:
                logger.debug(f"Custom agent '{name}' shadows builtin agent, skipping")
                continue

            # Build AgentConfig from definition
            config = AgentConfig(
                name=name,
                description=agent_def.get("description", f"Custom agent: {name}"),
                tools=agent_def.get("tools", "*"),
                skill_path=agent_def.get("skillPath"),
                source=(
                    AgentSource.USER_GLOBAL
                    if agent_def.get("_source") == "user-global"
                    else AgentSource.PROJECT
                ),
                model=agent_def.get("model"),
            )

            # Check for direct system prompt (from markdown agent files)
            # or build from skill file
            if "_system_prompt" in agent_def:
                system_prompt = agent_def["_system_prompt"]
            else:
                system_prompt = self._build_custom_agent_prompt(config)

            # Create SubAgentSpec for registration
            spec: SubAgentSpec = {
                "name": name,
                "description": config.description,
                "system_prompt": system_prompt,
                "tools": config.get_tool_list(self._all_tool_names),
            }

            if config.model:
                spec["model"] = config.model

            # Register the agent
            self.register_subagent(spec)
            logger.info(f"Registered custom agent: {name} (source: {config.source.value})")

    def _build_custom_agent_prompt(self, config: AgentConfig) -> str:
        """Build system prompt for a custom agent.

        Args:
            config: AgentConfig with skill_path or other config

        Returns:
            System prompt string
        """
        if config.skill_path:
            # Load skill content from file
            from pathlib import Path

            skill_path = Path(config.skill_path).expanduser()
            if skill_path.exists():
                try:
                    content = skill_path.read_text(encoding="utf-8")
                    # Strip YAML frontmatter if present
                    if content.startswith("---"):
                        import re

                        content = re.sub(r"^---\n.*?\n---\n*", "", content, flags=re.DOTALL)
                    return content
                except Exception as e:
                    logger.warning(f"Failed to load skill file {skill_path}: {e}")

        # Default prompt for custom agents
        return get_reminder(
            "generators/custom_agent_default", name=config.name, description=config.description
        )

    def _is_docker_available(self) -> bool:
        """Check if Docker is available on the system."""
        return shutil.which("docker") is not None

    def _get_spec_for_subagent(self, name: str) -> SubAgentSpec | None:
        """Get the SubAgentSpec for a registered subagent."""
        from .agents import ALL_SUBAGENTS

        return next((s for s in ALL_SUBAGENTS if s["name"] == name), None)

    def _extract_input_files(self, task: str, local_working_dir: Path) -> list[Path]:
        """Extract DOCUMENT file paths referenced in the task.

        Only extracts PDF, DOC, DOCX - formats that can contain research papers.
        Images (PNG, JPEG, SVG) and data files (CSV) are NOT extracted.

        Looks for:
        - @filename patterns (e.g., @paper.pdf)
        - Quoted file paths (e.g., "paper.pdf")
        - Unquoted document filenames (e.g., PDF paper.pdf)

        Args:
            task: The task description string
            local_working_dir: Local working directory to resolve relative paths

        Returns:
            List of existing document file paths to copy into Docker
        """
        import re

        files: list[Path] = []
        seen: set[str] = set()  # Track by resolved filename to avoid duplicates

        # Only document formats (PDF, DOC, DOCX)
        doc_pattern = r"pdf|docx?"

        # Pattern 1: @filename (e.g., @paper.pdf)
        at_mentions = re.findall(rf"@([\w\-\.]+\.(?:{doc_pattern}))\b", task, re.I)
        for filename in at_mentions:
            path = local_working_dir / filename
            if path.exists() and path.is_file() and path.name not in seen:
                files.append(path)
                seen.add(path.name)

        # Pattern 2: Quoted file paths (e.g., "paper.pdf", 'paper.pdf', `paper.pdf`)
        quoted_paths = re.findall(rf'["\'\`]([^"\'\`]+\.(?:{doc_pattern}))["\'\`]', task, re.I)
        for p in quoted_paths:
            path = Path(p) if Path(p).is_absolute() else local_working_dir / p
            if path.exists() and path.is_file() and path.name not in seen:
                files.append(path)
                seen.add(path.name)

        # Pattern 3: Unquoted document filenames (e.g., "PDF paper.pdf")
        unquoted_docs = re.findall(rf'(?:^|[\s(,])([^\s"\'()<>]+\.(?:{doc_pattern}))\b', task, re.I)
        for filename in unquoted_docs:
            path = Path(filename) if Path(filename).is_absolute() else local_working_dir / filename
            if path.exists() and path.is_file() and path.name not in seen:
                files.append(path)
                seen.add(path.name)

        # Pattern 4: Stems without extension (e.g., "paper 2303.11366v4" without .pdf)
        # Match alphanumeric+dots patterns that could be paper IDs (e.g., arXiv IDs)
        # Then check if a corresponding .pdf/.doc/.docx exists
        stem_pattern = r"(?:^|[\s(,])(\d[\w\.\-]+v\d+|\d{4}\.\d+(?:v\d+)?)\b"
        stems = re.findall(stem_pattern, task, re.I)
        for stem in stems:
            # Try adding document extensions
            for ext in ["pdf", "PDF", "docx", "DOCX", "doc", "DOC"]:
                candidate = local_working_dir / f"{stem}.{ext}"
                if candidate.exists() and candidate.is_file():
                    # Check if this file was already found (use resolved name)
                    if candidate.name not in seen:
                        files.append(candidate)
                        seen.add(candidate.name)
                    break

        return files

    def _extract_github_info(self, task: str) -> tuple[str, str, str] | None:
        """Extract GitHub repo URL and issue number from task.

        Looks for GitHub issue URLs in the format:
        https://github.com/owner/repo/issues/123

        Args:
            task: The task description string

        Returns:
            Tuple of (repo_url, owner_repo, issue_number) or None if not found
        """
        import re

        match = re.search(r"https://github\.com/([^/]+/[^/]+)/issues/(\d+)", task)
        if match:
            owner_repo = match.group(1)
            issue_number = match.group(2)
            repo_url = f"https://github.com/{owner_repo}.git"
            return (repo_url, owner_repo, issue_number)
        return None

    def _copy_files_to_docker(
        self,
        container_name: str,
        files: list[Path],
        workspace_dir: str,
        ui_callback: Any = None,
    ) -> dict[str, str]:
        """Copy local files into Docker container using docker cp.

        Args:
            container_name: Docker container name or ID
            files: List of local file paths to copy
            workspace_dir: Target directory in Docker container
            ui_callback: Optional UI callback for progress display

        Returns:
            Mapping of Docker paths to local paths (for local-only tool remapping)
        """
        import subprocess

        path_mapping: dict[str, str] = {}

        for local_file in files:
            # Show copy progress - use on_nested_tool_call for proper display
            if ui_callback:
                if hasattr(ui_callback, "on_nested_tool_call"):
                    # Direct nested callback - use proper method
                    ui_callback.on_nested_tool_call(
                        "docker_copy",
                        {"file": local_file.name},
                        depth=getattr(ui_callback, "_depth", 1),
                        parent=getattr(ui_callback, "_context", "Docker"),
                    )
                elif hasattr(ui_callback, "on_tool_call"):
                    ui_callback.on_tool_call("docker_copy", {"file": local_file.name})

            try:
                docker_target = f"{workspace_dir}/{local_file.name}"
                docker_path = f"{container_name}:{docker_target}"

                # Use docker cp for reliable file transfer (handles any size/binary)
                result = subprocess.run(
                    ["docker", "cp", str(local_file), docker_path],
                    capture_output=True,
                    text=True,
                    timeout=60.0,
                )

                if result.returncode != 0:
                    raise RuntimeError(f"docker cp failed: {result.stderr}")

                # Store mapping: Docker path → local path
                path_mapping[docker_target] = str(local_file)

                # Show completion - use on_nested_tool_result for proper display
                if ui_callback:
                    result_data = {"success": True, "output": f"Copied to {docker_target}"}
                    if hasattr(ui_callback, "on_nested_tool_result"):
                        ui_callback.on_nested_tool_result(
                            "docker_copy",
                            {"file": local_file.name},
                            result_data,
                            depth=getattr(ui_callback, "_depth", 1),
                            parent=getattr(ui_callback, "_context", "Docker"),
                        )
                    elif hasattr(ui_callback, "on_tool_result"):
                        ui_callback.on_tool_result(
                            "docker_copy", {"file": local_file.name}, result_data
                        )

            except Exception as e:
                if ui_callback:
                    result_data = {"success": False, "error": str(e)}
                    if hasattr(ui_callback, "on_nested_tool_result"):
                        ui_callback.on_nested_tool_result(
                            "docker_copy",
                            {"file": local_file.name},
                            result_data,
                            depth=getattr(ui_callback, "_depth", 1),
                            parent=getattr(ui_callback, "_context", "Docker"),
                        )
                    elif hasattr(ui_callback, "on_tool_result"):
                        ui_callback.on_tool_result(
                            "docker_copy", {"file": local_file.name}, result_data
                        )

        return path_mapping

    def _rewrite_task_for_docker(
        self,
        task: str,
        input_files: list[Path],
        workspace_dir: str,
    ) -> str:
        """Rewrite task to reference Docker paths instead of local paths.

        Args:
            task: Original task description
            input_files: List of files that were copied to Docker
            workspace_dir: Docker workspace directory

        Returns:
            Task with paths rewritten to Docker paths, including Docker context
        """
        new_task = task

        # Remove phrases that hint at local filesystem
        new_task = re.sub(r"\blocal\s+", "", new_task, flags=re.IGNORECASE)
        new_task = re.sub(r"\bin this repo\b", f"in {workspace_dir}", new_task, flags=re.IGNORECASE)
        new_task = re.sub(r"\bthis repo\b", workspace_dir, new_task, flags=re.IGNORECASE)

        # Replace any reference to the local working directory with workspace
        local_working_dir = self._working_dir
        if local_working_dir:
            local_dir_str = str(local_working_dir)
            # Replace the local directory path with workspace
            new_task = new_task.replace(local_dir_str, workspace_dir)
            # Also try without trailing slash
            new_task = new_task.replace(local_dir_str.rstrip("/"), workspace_dir)

        for local_file in input_files:
            docker_path = f"{workspace_dir}/{local_file.name}"
            # Replace @filename with Docker path
            new_task = new_task.replace(f"@{local_file.name}", docker_path)
            # Also replace any absolute local paths
            new_task = new_task.replace(str(local_file), docker_path)
            # Replace plain filename references (be careful to avoid partial matches)
            # Use word boundary matching by checking surrounding chars
            new_task = re.sub(rf"\b{re.escape(local_file.name)}\b", docker_path, new_task)

        # Prepend Docker context with strong emphasis
        docker_context = get_reminder("docker/docker_context", workspace_dir=workspace_dir)
        return docker_context + "\n\n" + new_task

    def _create_docker_path_sanitizer(
        self,
        workspace_dir: str,
        local_dir: str,
        image_name: str,
        container_id: str,
    ):
        """Create a path sanitizer for Docker mode UI display.

        Converts local paths to Docker workspace paths with container prefix:
        - /Users/.../test_opencli/src/file.py → [uv:a1b2c3d4]:/workspace/src/file.py
        - README.md → [uv:a1b2c3d4]:/workspace/README.md

        Args:
            workspace_dir: The Docker workspace directory (e.g., /workspace)
            local_dir: The local working directory (e.g., /Users/.../test_opencli)
            image_name: Full Docker image name (e.g., ghcr.io/astral-sh/uv:python3.11)
            container_id: Short container ID (e.g., a1b2c3d4)

        Returns:
            A callable that sanitizes paths for display
        """
        # Extract short image name: "ghcr.io/astral-sh/uv:python3.11-bookworm" → "uv"
        short_image = image_name.split("/")[-1].split(":")[0]
        prefix = f"[{short_image}:{container_id}]:"

        def sanitize(path: str) -> str:
            # If path starts with local_dir, replace with workspace_dir
            if path.startswith(local_dir):
                relative = path[len(local_dir) :].lstrip("/")
                docker_path = f"{workspace_dir}/{relative}" if relative else workspace_dir
                return f"{prefix}{docker_path}"

            # Handle Docker-internal absolute paths (e.g., /workspace/..., /testbed/...)
            # These are paths the LLM outputs when running inside Docker
            if path.startswith(workspace_dir):
                return f"{prefix}{path}"

            # Fallback: extract filename from other absolute paths
            match = re.match(r"^(/Users/|/home/|/var/|/tmp/).+/([^/]+)$", path)
            if match:
                return f"{prefix}{workspace_dir}/{match.group(2)}"

            # Convert relative paths to full Docker paths
            # e.g., "README.md" → "[uv:a1b2c3d4]:/workspace/README.md"
            # e.g., "." → "[uv:a1b2c3d4]:/workspace"
            # e.g., "src/model.py" → "[uv:a1b2c3d4]:/workspace/src/model.py"
            if not path.startswith("/"):
                clean_path = path.lstrip("./")
                if not clean_path:
                    return f"{prefix}{workspace_dir}"
                return f"{prefix}{workspace_dir}/{clean_path}"

            return path

        return sanitize

    def create_docker_nested_callback(
        self,
        ui_callback: Any,
        subagent_name: str,
        workspace_dir: str,
        image_name: str,
        container_id: str,
        local_dir: str | None = None,
    ) -> Any:
        """Create NestedUICallback with Docker path sanitizer for consistent display.

        This is the STANDARD INTERFACE for Docker subagent UI context.
        Use this method whenever executing a subagent inside Docker.

        Args:
            ui_callback: Parent UI callback to wrap
            subagent_name: Name of the subagent (e.g., "Code-Explorer", "Web-clone")
            workspace_dir: Docker workspace path (e.g., "/workspace", "/testbed")
            image_name: Full Docker image name (e.g., "ghcr.io/astral-sh/uv:python3.11")
            container_id: Short container ID (e.g., "a1b2c3d4")
            local_dir: Local directory for path remapping (optional)

        Returns:
            NestedUICallback wrapped with Docker path sanitizer, or None if ui_callback is None

        Usage:
            nested_callback = manager.create_docker_nested_callback(
                ui_callback=self.ui_callback,
                subagent_name="Web-clone",
                workspace_dir="/workspace",
                image_name=docker_image,
                container_id=container_id,
            )
            result = manager.execute_subagent(..., ui_callback=nested_callback)
        """
        if ui_callback is None:
            return None

        from swecli.ui_textual.nested_callback import NestedUICallback

        # Use existing _create_docker_path_sanitizer
        path_sanitizer = self._create_docker_path_sanitizer(
            workspace_dir=workspace_dir,
            local_dir=local_dir or str(self._working_dir or Path.cwd()),
            image_name=image_name,
            container_id=container_id,
        )

        return NestedUICallback(
            parent_callback=ui_callback,
            parent_context=subagent_name,
            depth=1,
            path_sanitizer=path_sanitizer,
        )

    def execute_with_docker_handler(
        self,
        name: str,
        task: str,
        deps: SubAgentDeps,
        docker_handler: Any,
        ui_callback: Any = None,
        container_id: str = "",
        image_name: str = "",
        workspace_dir: str = "/workspace",
        description: str | None = None,
    ) -> dict[str, Any]:
        """Execute subagent with pre-configured Docker handler.

        Use this when you need custom Docker setup (e.g., clone repo, install deps)
        before subagent execution, but still want standardized UI display.

        This provides:
        - Spawn header: Spawn[name](description)
        - Nested callback with Docker path prefix: [image:containerid]:/workspace/...
        - Consistent result display

        Args:
            name: Subagent name (e.g., "Code-Explorer", "Web-clone")
            task: Task prompt for subagent
            deps: SubAgentDeps with mode_manager, approval_manager, undo_manager
            docker_handler: Pre-configured DockerToolHandler
            ui_callback: UI callback for display
            container_id: Docker container ID (last 8 chars) for path prefix
            image_name: Docker image name for path prefix
            workspace_dir: Workspace directory inside container
            description: Description for Spawn header (defaults to task excerpt)

        Returns:
            Result dict with success, content, etc.
        """
        compiled = self._agents.get(name)
        if not compiled:
            return {"success": False, "error": f"Unknown subagent: {name}"}

        # Extract description from task if not provided
        if description is None:
            description = self._extract_task_description(task)

        # Show Spawn header
        spawn_args = {
            "subagent_type": name,
            "description": description,
        }
        if ui_callback and hasattr(ui_callback, "on_tool_call"):
            ui_callback.on_tool_call("spawn_subagent", spawn_args)

        # Create nested callback with Docker context
        nested_callback = self.create_docker_nested_callback(
            ui_callback=ui_callback,
            subagent_name=name,
            workspace_dir=workspace_dir,
            image_name=image_name,
            container_id=container_id,
        )

        try:
            # Execute subagent with nested callback and docker handler
            result = self.execute_subagent(
                name=name,
                task=task,
                deps=deps,
                ui_callback=nested_callback,
                docker_handler=docker_handler,
                show_spawn_header=False,  # Already shown
            )

            # Show Spawn result
            if ui_callback and hasattr(ui_callback, "on_tool_result"):
                success = isinstance(result, str) or result.get("success", True)
                ui_callback.on_tool_result(
                    "spawn_subagent",
                    spawn_args,
                    {
                        "success": success,
                        "output": (
                            result.get("content", "") if isinstance(result, dict) else str(result)
                        ),
                    },
                )

            return result

        except Exception as e:
            if ui_callback and hasattr(ui_callback, "on_tool_result"):
                ui_callback.on_tool_result(
                    "spawn_subagent",
                    spawn_args,
                    {
                        "success": False,
                        "error": str(e),
                    },
                )
            return {"success": False, "error": str(e)}

    def _extract_task_description(self, task: str) -> str:
        """Extract a short description from the task for Spawn header display.

        Args:
            task: The full task description

        Returns:
            A short description suitable for display
        """
        # Look for PDF filename in task
        if ".pdf" in task.lower():
            match = re.search(r"([^\s/]+\.pdf)", task, re.IGNORECASE)
            if match:
                return f"Implement {match.group(1)}"
        # Default: first line, truncated
        first_line = task.split("\n")[0][:50]
        if len(task.split("\n")[0]) > 50:
            return first_line + "..."
        return first_line

    def _get_agent_display_type(self, name: str) -> str:
        """Get the display type for an agent.

        Args:
            name: The subagent name

        Returns:
            The display type (e.g., "Explore" for "Explore" agent)
        """
        # Map internal agent names to display types
        # For now, just return the name as-is
        # Could add special handling for specific agents
        return name

    def _execute_with_docker(
        self,
        name: str,
        task: str,
        deps: SubAgentDeps,
        spec: SubAgentSpec,
        ui_callback: Any = None,
        task_monitor: Any = None,
        show_spawn_header: bool = True,
        local_output_dir: Path | None = None,
    ) -> dict[str, Any]:
        """Execute a subagent inside Docker with automatic container lifecycle.

        This method:
        1. Starts a Docker container with the spec's docker_config
        2. Executes the subagent with all tools routed through Docker
        3. Copies generated files from container to local working directory
        4. Stops the container

        Args:
            name: The subagent type name
            task: The task description
            deps: Dependencies for tool execution
            spec: The subagent specification with docker_config
            ui_callback: Optional UI callback
            task_monitor: Optional task monitor
            show_spawn_header: Whether to show the Spawn[] header. Set to False when
                called via tool_registry (react_executor already showed it).
            local_output_dir: Local directory where files should be copied after Docker
                execution. If None, uses self._working_dir or cwd.

        Returns:
            Result dict with content, success, and messages
        """
        import asyncio
        from swecli.core.docker.deployment import DockerDeployment
        from swecli.core.docker.tool_handler import DockerToolHandler

        docker_config = spec.get("docker_config")
        if docker_config is None:
            return {
                "success": False,
                "error": "No docker_config in subagent spec",
                "content": "",
            }

        # Workspace inside Docker container
        workspace_dir = "/workspace"
        local_working_dir = local_output_dir or (
            Path(self._working_dir) if self._working_dir else Path.cwd()
        )

        deployment = None
        loop = None
        nested_callback = None

        # Show Spawn header only for direct invocations (e.g., /paper2code)
        # When called via tool_registry, react_executor already showed the header
        spawn_args = None
        if show_spawn_header:
            spawn_args = {
                "subagent_type": name,
                "description": self._extract_task_description(task),
            }
            if ui_callback and hasattr(ui_callback, "on_tool_call"):
                ui_callback.on_tool_call("spawn_subagent", spawn_args)

        try:
            # Create Docker deployment first to get container name
            # (container name is generated in __init__, before start())
            deployment = DockerDeployment(config=docker_config)

            # Extract container ID (last 8 chars of container name)
            # Container name format: "swecli-runtime-a1b2c3d4"
            container_id = deployment._container_name.split("-")[-1]

            # Create nested callback wrapper with container info using standardized interface
            # This ensures docker_start, docker_copy, and all subagent tool calls
            # appear properly nested under the Spawn[subagent_name] parent
            nested_callback = self.create_docker_nested_callback(
                ui_callback=ui_callback,
                subagent_name=name,
                workspace_dir=workspace_dir,
                image_name=docker_config.image,
                container_id=container_id,
                local_dir=str(local_working_dir),
            )

            # Show Docker start as a tool call with spinner (using nested callback)
            if nested_callback and hasattr(nested_callback, "on_tool_call"):
                nested_callback.on_tool_call("docker_start", {"image": docker_config.image})

            # Run async start in sync context - use a single event loop for the whole operation
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
            loop.run_until_complete(deployment.start())

            # Show Docker start completion (using nested callback)
            if nested_callback and hasattr(nested_callback, "on_tool_result"):
                nested_callback.on_tool_result(
                    "docker_start",
                    {"image": docker_config.image},
                    {
                        "success": True,
                        "output": docker_config.image,
                    },
                )

            # Create workspace directory in Docker container
            # (some images like uv don't have /workspace by default)
            loop.run_until_complete(deployment.runtime.run(f"mkdir -p {workspace_dir}"))

            # Create Docker tool handler with local registry fallback for tools like read_pdf
            runtime = deployment.runtime
            shell_init = docker_config.shell_init if hasattr(docker_config, "shell_init") else ""
            docker_handler = DockerToolHandler(
                runtime,
                workspace_dir=workspace_dir,
                shell_init=shell_init,
            )

            # Extract input files from task (PDFs, images, etc.)
            input_files = self._extract_input_files(task, local_working_dir)

            # Copy input files into Docker container using docker cp
            # Returns mapping of Docker paths to local paths for local-only tools
            # Note: Individual docker_copy calls will show progress for each file
            path_mapping: dict[str, str] = {}
            if input_files:
                path_mapping = self._copy_files_to_docker(
                    deployment._container_name,
                    input_files,
                    workspace_dir,
                    nested_callback,  # Use nested callback for proper nesting
                )

            # Rewrite task to use Docker paths
            docker_task = self._rewrite_task_for_docker(task, input_files, workspace_dir)

            # Execute subagent with Docker tools (local_registry passed for fallback)
            # Pass nested_callback - execute_subagent will detect it's already nested
            result = self.execute_subagent(
                name=name,
                task=docker_task,  # Use rewritten task with Docker paths
                deps=deps,
                ui_callback=nested_callback,  # Already nested, will be used directly
                task_monitor=task_monitor,
                working_dir=workspace_dir,
                docker_handler=docker_handler,
                path_mapping=path_mapping,  # For local-only tool path remapping
            )

            # Copy generated files from Docker to local working directory
            if result.get("success"):
                self._copy_files_from_docker(
                    container_name=deployment._container_name,
                    workspace_dir=workspace_dir,
                    local_dir=local_working_dir,
                    spec=spec,
                    ui_callback=nested_callback,
                )

            # Show Spawn completion only if we showed the header
            if spawn_args and ui_callback and hasattr(ui_callback, "on_tool_result"):
                ui_callback.on_tool_result(
                    "spawn_subagent",
                    spawn_args,
                    {
                        "success": result.get("success", True),
                    },
                )

            return result

        except Exception as e:
            import traceback

            # Stop the docker_start spinner by reporting failure
            if nested_callback and hasattr(nested_callback, "on_tool_result"):
                nested_callback.on_tool_result(
                    "docker_start",
                    {"image": docker_config.image},
                    {
                        "success": False,
                        "error": str(e),
                    },
                )
            # Show Spawn failure only if we showed the header
            if spawn_args and ui_callback and hasattr(ui_callback, "on_tool_result"):
                ui_callback.on_tool_result(
                    "spawn_subagent",
                    spawn_args,
                    {
                        "success": False,
                        "error": str(e),
                    },
                )
            return {
                "success": False,
                "error": f"Docker execution failed: {str(e)}\n{traceback.format_exc()}",
                "content": "",
            }
        finally:
            # Show Docker stop as a tool call (matching docker_start pattern)
            if (
                deployment is not None
                and nested_callback
                and hasattr(nested_callback, "on_tool_call")
            ):
                nested_callback.on_tool_call(
                    "docker_stop", {"container": deployment._container_name[:12]}
                )

            # Always stop the container
            if deployment is not None and loop is not None:
                try:
                    loop.run_until_complete(deployment.stop())
                except Exception:
                    pass  # Ignore cleanup errors

                # Show Docker stop completion with container ID
                if nested_callback and hasattr(nested_callback, "on_tool_result"):
                    container_id = deployment._container_name
                    nested_callback.on_tool_result(
                        "docker_stop",
                        {"container": container_id},
                        {"success": True, "output": container_id},
                    )

            # Close the loop after all async operations
            if loop is not None:
                try:
                    loop.close()
                except Exception:
                    pass

    def _copy_files_from_docker(
        self,
        container_name: str,
        workspace_dir: str,
        local_dir: Path,
        spec: SubAgentSpec | None = None,
        ui_callback: Any = None,
    ) -> None:
        """Copy generated files from Docker container to local directory using docker cp.

        Uses docker cp for recursive directory copy, which is more reliable and
        handles nested directories properly (e.g., reflexion_minimal/*.py).

        Args:
            container_name: Docker container name/ID
            workspace_dir: Path inside container (e.g., /workspace)
            local_dir: Local directory to copy files to
            spec: SubAgentSpec for copy configuration
            ui_callback: UI callback for progress display
        """
        import subprocess

        recursive = spec.get("copy_back_recursive", True) if spec else True

        if not recursive:
            return  # Skip copy if not configured

        try:
            # Show copy operation in UI
            if ui_callback and hasattr(ui_callback, "on_tool_call"):
                ui_callback.on_tool_call(
                    "docker_copy_back",
                    {
                        "from": f"{container_name}:{workspace_dir}",
                        "to": str(local_dir),
                    },
                )

            # Use docker cp to copy entire workspace recursively
            # The "/." at the end copies contents without creating workspace folder
            result = subprocess.run(
                ["docker", "cp", f"{container_name}:{workspace_dir}/.", str(local_dir)],
                capture_output=True,
                text=True,
                timeout=120.0,
            )

            if result.returncode == 0:
                logger.info(f"Copied workspace from Docker to {local_dir}")
                if ui_callback and hasattr(ui_callback, "on_tool_result"):
                    ui_callback.on_tool_result(
                        "docker_copy_back",
                        {},
                        {
                            "success": True,
                            "output": f"Copied to {local_dir}",
                        },
                    )
            else:
                logger.warning(f"docker cp failed: {result.stderr}")
                if ui_callback and hasattr(ui_callback, "on_tool_result"):
                    ui_callback.on_tool_result(
                        "docker_copy_back",
                        {},
                        {
                            "success": False,
                            "error": result.stderr,
                        },
                    )

        except subprocess.TimeoutExpired:
            logger.error("docker cp timed out after 120 seconds")
        except Exception as e:
            logger.error(f"Failed to copy from Docker: {e}")

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
            from swecli.core.hooks.models import HookEvent

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
            from swecli.core.docker.tool_handler import DockerToolRegistry

            tool_registry = DockerToolRegistry(
                docker_handler,
                local_registry=self._tool_registry,
                path_mapping=path_mapping,
            )
        else:
            tool_registry = self._tool_registry

        # If working_dir or docker_handler requires a new agent instance
        if working_dir is not None or docker_handler is not None:
            from swecli.core.agents import MainAgent
            from .agents import ALL_SUBAGENTS

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
            from swecli.ui_textual.nested_callback import NestedUICallback

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
            from swecli.core.hooks.models import HookEvent

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
        from swecli.core.context_engineering.tools.implementations.ask_user_tool import (
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
