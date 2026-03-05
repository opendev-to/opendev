"""Configuration management with hierarchical loading."""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

from swecli.core.paths import get_paths
from swecli.models.config import AppConfig

logger = logging.getLogger(__name__)


class ConfigManager:
    """Manages hierarchical configuration loading and merging."""

    def __init__(self, working_dir: Path | None = None):
        """Initialize config manager.

        Args:
            working_dir: Current working directory (defaults to cwd)
        """
        self.working_dir = working_dir or Path.cwd()
        self._config: AppConfig | None = None

    def load_config(self) -> AppConfig:
        """Load and merge configuration from multiple sources.

        Priority (highest to lowest):
        1. Local project config (.opendev/settings.json)
        2. Global user config (~/.opendev/settings.json)
        3. Default values
        """
        # Start with defaults
        config_data: dict = {}
        global_data: dict = {}
        local_data: dict = {}

        # Load global config
        paths = get_paths(self.working_dir)
        global_config = paths.global_settings
        if global_config.exists():
            with open(global_config) as f:
                global_data = json.load(f)
                # Remove legacy api_key from config - keys should come from environment
                global_data.pop("api_key", None)
                _, global_changed = self._normalize_fireworks_models(global_data)
                if global_changed:
                    with open(global_config, "w") as target:
                        json.dump(global_data, target, indent=2)
                config_data.update(global_data)

        # Load local project config
        local_config = paths.project_settings
        if local_config.exists():
            with open(local_config) as f:
                local_data = json.load(f)
                # Remove legacy api_key from config - keys should come from environment
                local_data.pop("api_key", None)
                _, local_changed = self._normalize_fireworks_models(local_data)
                if local_changed:
                    with open(local_config, "w") as target:
                        json.dump(local_data, target, indent=2)
                config_data.update(local_data)

        self._normalize_fireworks_models(config_data)

        # Create AppConfig with merged data
        self._config = AppConfig(**config_data)

        # Auto-set max_context_tokens from model if:
        # 1. Not explicitly configured, OR
        # 2. Set to old defaults (100000 or 256000)
        current_max = config_data.get("max_context_tokens")
        if current_max is None or current_max in [100000, 256000]:
            model_info = self._config.get_model_info()
            if model_info and model_info.context_length:
                # Use 80% of context length to leave room for response
                self._config.max_context_tokens = int(model_info.context_length * 0.8)

        return self._config

    def get_config(self) -> AppConfig:
        """Get current config, loading if necessary."""
        if self._config is None:
            return self.load_config()
        return self._config

    def save_config(self, config: AppConfig, global_config: bool = False) -> None:
        """Save configuration to file.

        Args:
            config: Configuration to save
            global_config: If True, save to global config; otherwise save to local project
        """
        paths = get_paths(self.working_dir)
        if global_config:
            config_path = paths.global_settings
        else:
            config_path = paths.project_settings

        config_path.parent.mkdir(parents=True, exist_ok=True)

        # Only save user-facing settings, not internal defaults
        # Note: api_key is intentionally excluded - keys should come from
        # environment variables for security and to avoid cross-provider issues
        user_fields = {
            "model_provider",
            "model",
            "model_thinking_provider",
            "model_thinking",
            "model_vlm_provider",
            "model_vlm",
            "model_critique_provider",
            "model_critique",
            "model_compact_provider",
            "model_compact",
            "api_base_url",
            "debug_logging",
        }
        data = {k: v for k, v in config.model_dump().items() if k in user_fields and v is not None}
        with open(config_path, "w") as f:
            json.dump(data, f, indent=2)

    def ensure_directories(self) -> None:
        """Ensure all required directories exist."""
        import shutil

        config = self.get_config()

        # Expand paths
        opendev_dir = Path(config.opendev_dir).expanduser()
        log_dir = Path(config.log_dir).expanduser()

        # Create directories
        opendev_dir.mkdir(parents=True, exist_ok=True)
        log_dir.mkdir(parents=True, exist_ok=True)

        # Create projects directory for project-scoped sessions
        paths = get_paths(self.working_dir)
        paths.global_projects_dir.mkdir(parents=True, exist_ok=True)

        # Migrate: delete the old flat sessions directory if it has session files
        old_sessions_dir = Path(config.session_dir).expanduser()
        if old_sessions_dir.exists() and any(old_sessions_dir.glob("*.json")):
            shutil.rmtree(old_sessions_dir, ignore_errors=True)
        # Still ensure it exists (some code paths may reference it)
        old_sessions_dir.mkdir(parents=True, exist_ok=True)

        # Create user skills directory
        self.user_skills_dir.mkdir(parents=True, exist_ok=True)

        # Create local command directory if in a project
        local_cmd_dir = self.working_dir / config.command_dir
        if not local_cmd_dir.exists() and (self.working_dir / ".git").exists():
            local_cmd_dir.mkdir(parents=True, exist_ok=True)

    def load_context_files(self) -> list[str]:
        """Load OPENDEV.md context files hierarchically.

        Returns:
            List of context file contents, from global to local
        """
        contexts = []
        paths = get_paths(self.working_dir)

        # Global context
        global_context = paths.global_context_file
        if global_context.exists():
            contexts.append(global_context.read_text())

        # Project root context
        project_context = self.working_dir / "OPENDEV.md"
        if project_context.exists():
            contexts.append(project_context.read_text())

        # Subdirectory contexts (walk up from current dir to project root)
        current = self.working_dir
        while current != current.parent:
            subdir_context = current / "OPENDEV.md"
            if subdir_context.exists() and subdir_context != project_context:
                contexts.insert(1, subdir_context.read_text())  # Insert after global
            current = current.parent

        return contexts

    @staticmethod
    def _normalize_fireworks_models(data: dict) -> tuple[dict, bool]:
        """Normalize Fireworks model identifiers to full registry IDs."""
        changed = False
        mapping = [
            ("model_provider", "model"),
            ("model_thinking_provider", "model_thinking"),
            ("model_vlm_provider", "model_vlm"),
            ("model_critique_provider", "model_critique"),
        ]

        for provider_key, model_key in mapping:
            provider_id = data.get(provider_key)
            model_id = data.get(model_key)
            if provider_id != "fireworks":
                continue
            if not isinstance(model_id, str) or not model_id.strip():
                continue
            normalized = model_id.strip()
            if normalized.startswith("accounts/"):
                continue
            slug = normalized.split("/")[-1]
            corrected = f"accounts/fireworks/models/{slug}"
            if normalized != corrected:
                data[model_key] = corrected
                changed = True

        return data, changed

    # ===== Skills System Support =====

    @property
    def user_skills_dir(self) -> Path:
        """Get user-global skills directory (~/.opendev/skills/)."""
        return get_paths(self.working_dir).global_skills_dir

    @property
    def project_skills_dir(self) -> Path | None:
        """Get project-local skills directory (<project>/.opendev/skills/).

        Returns None if no working directory is set.
        """
        if self.working_dir:
            return get_paths(self.working_dir).project_skills_dir
        return None

    def get_skill_dirs(self) -> list[Path]:
        """Get all skill directories in priority order.

        Returns directories from highest to lowest priority:
        1. Project skills (.opendev/skills/)
        2. User global skills (~/.opendev/skills/)
        3. Project bundle skills (.opendev/plugins/bundles/*/skills/)
        4. User bundle skills (~/.opendev/plugins/bundles/*/skills/)
        5. Built-in skills (shipped with package)

        Returns:
            List of existing skill directories, highest priority first
        """
        dirs = []
        # Project skills take priority
        if self.project_skills_dir and self.project_skills_dir.exists():
            dirs.append(self.project_skills_dir)
        # User global skills
        if self.user_skills_dir.exists():
            dirs.append(self.user_skills_dir)
        # Bundle skills (from enabled bundles)
        dirs.extend(self._get_bundle_skill_dirs())
        # Built-in skills (lowest priority)
        builtin_dir = get_paths(self.working_dir).builtin_skills_dir
        if builtin_dir.exists():
            dirs.append(builtin_dir)
        return dirs

    def _get_bundle_skill_dirs(self) -> list[Path]:
        """Get skill directories from all enabled bundles.

        Returns:
            List of skill directories from bundles
        """
        bundle_dirs = []
        try:
            from swecli.core.plugins import PluginManager

            plugin_manager = PluginManager(self.working_dir)
            for bundle in plugin_manager.list_bundles():
                if bundle.enabled:
                    skills_dir = Path(bundle.path) / "skills"
                    if skills_dir.exists():
                        bundle_dirs.append(skills_dir)
        except Exception:
            pass  # Bundles not available
        return bundle_dirs

    def _load_markdown_agent(self, path: Path, source: str) -> dict[str, Any] | None:
        """Load a Claude Code-style markdown agent file.

        Markdown agents use YAML frontmatter for configuration:
        ```markdown
        ---
        name: agent-name
        description: "Agent description"
        model: sonnet
        tools: "*"
        ---

        System prompt content here...
        ```

        Args:
            path: Path to the markdown file
            source: Source identifier ("user-global" or "project")

        Returns:
            Agent definition dict or None if parsing fails
        """
        try:
            content = path.read_text(encoding="utf-8")

            # Parse YAML frontmatter
            if content.startswith("---"):
                parts = content.split("---", 2)
                if len(parts) >= 3:
                    import yaml

                    frontmatter = yaml.safe_load(parts[1])
                    if not isinstance(frontmatter, dict):
                        frontmatter = {}
                    system_prompt = parts[2].strip()

                    return {
                        "name": frontmatter.get("name", path.stem),
                        "description": frontmatter.get(
                            "description", f"Custom agent from {path.name}"
                        ),
                        "model": frontmatter.get("model"),
                        "tools": frontmatter.get("tools", "*"),
                        "_system_prompt": system_prompt,  # Direct prompt, not skillPath
                        "_source": source,
                    }

            # Fallback: use filename as name, content as prompt
            return {
                "name": path.stem,
                "description": f"Custom agent from {path.name}",
                "_system_prompt": content,
                "_source": source,
            }
        except Exception as e:
            logger.warning(f"Failed to load markdown agent {path}: {e}")
            return None

    def load_custom_agents(self) -> list[dict[str, Any]]:
        """Load custom agent definitions from config files and markdown agents.

        Loads from (in priority order, later sources override earlier):
        1. ~/.opendev/agents.json (user global JSON)
        2. ~/.opendev/agents/*.md (user global markdown)
        3. <project>/.opendev/agents.json (project local JSON)
        4. <project>/.opendev/agents/*.md (project local markdown)

        Returns:
            List of agent definitions merged from all sources
        """
        agents: list[dict[str, Any]] = []
        seen_names: set[str] = set()
        paths = get_paths(self.working_dir)

        def add_agent(agent: dict[str, Any]) -> None:
            """Add agent, removing any existing agent with the same name."""
            name = agent.get("name")
            if not name:
                return
            # Remove existing agent with same name (later sources override)
            nonlocal agents, seen_names
            agents = [a for a in agents if a.get("name") != name]
            seen_names.discard(name)
            agents.append(agent)
            seen_names.add(name)

        # 1. Load user global JSON agents
        global_agents_file = paths.global_agents_file
        if global_agents_file.exists():
            try:
                with open(global_agents_file) as f:
                    data = json.load(f)
                    for agent in data.get("agents", []):
                        if agent.get("name"):
                            agent["_source"] = "user-global"
                            add_agent(agent)
            except (json.JSONDecodeError, OSError) as e:
                logger.warning(f"Failed to load global agents.json: {e}")

        # 2. Load user global markdown agents
        global_agents_dir = paths.global_agents_dir
        if global_agents_dir.exists() and global_agents_dir.is_dir():
            for md_file in sorted(global_agents_dir.glob("*.md")):
                agent = self._load_markdown_agent(md_file, "user-global")
                if agent:
                    add_agent(agent)

        # 3. Load project JSON agents
        if self.working_dir:
            project_agents_file = paths.project_agents_file
            if project_agents_file.exists():
                try:
                    with open(project_agents_file) as f:
                        data = json.load(f)
                        for agent in data.get("agents", []):
                            if agent.get("name"):
                                agent["_source"] = "project"
                                add_agent(agent)
                except (json.JSONDecodeError, OSError) as e:
                    logger.warning(f"Failed to load project agents.json: {e}")

            # 4. Load project markdown agents
            project_agents_dir = paths.project_agents_dir
            if project_agents_dir.exists() and project_agents_dir.is_dir():
                for md_file in sorted(project_agents_dir.glob("*.md")):
                    agent = self._load_markdown_agent(md_file, "project")
                    if agent:
                        add_agent(agent)

        return agents
