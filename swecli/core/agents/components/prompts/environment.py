"""Environment context collection and formatting for system prompts."""

from __future__ import annotations

import datetime
import logging
import os
import platform
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from swecli.models.config import AppConfig

logger = logging.getLogger(__name__)

# Directories to skip when building the directory tree
_TREE_SKIP_DIRS = frozenset(
    {
        ".git",
        "node_modules",
        "__pycache__",
        ".venv",
        "venv",
        ".tox",
        "dist",
        "build",
        ".mypy_cache",
        ".ruff_cache",
        ".pytest_cache",
        ".eggs",
        "*.egg-info",
    }
)

# Config files to detect at project root
_CONFIG_FILES = (
    "pyproject.toml",
    "package.json",
    "Cargo.toml",
    "go.mod",
    "Makefile",
    "Gemfile",
    "pom.xml",
    "build.gradle",
    "composer.json",
    "CMakeLists.txt",
    "tsconfig.json",
    "setup.py",
    "requirements.txt",
)

# Package managers to probe
_PACKAGE_MANAGERS = ("pip", "npm", "yarn", "pnpm", "cargo", "go", "gem", "composer")


@dataclass(frozen=True)
class EnvironmentContext:
    """Immutable snapshot of environment data collected at startup."""

    # Core runtime
    working_dir: str
    platform: str  # "macos", "linux", "windows"
    os_version: str  # "Darwin 25.2.0"
    current_date: str  # "2026-02-08"
    model: str  # "gpt-4o"
    model_provider: str  # "openai"
    is_git_repo: bool

    # Git (all None if not a git repo)
    git_branch: str | None = None
    git_default_branch: str | None = None
    git_status: str | None = None
    git_recent_commits: str | None = None
    git_remote_url: str | None = None

    # Shell & runtime
    shell: str | None = None
    python_version: str | None = None
    virtual_env: str | None = None
    node_version: str | None = None
    available_package_managers: tuple[str, ...] = ()

    # Project structure
    project_config_files: tuple[str, ...] = ()
    tech_stack: str | None = None
    directory_tree: str | None = None

    # Project instructions
    project_instructions: str | None = None


class EnvironmentCollector:
    """Collects environment data into an EnvironmentContext snapshot."""

    def __init__(
        self,
        working_dir: Path,
        config: "AppConfig",
        config_manager: Any = None,
    ) -> None:
        self._working_dir = working_dir
        self._config = config
        self._config_manager = config_manager

    def collect(self) -> EnvironmentContext:
        """Collect all environment data and return an immutable snapshot."""
        is_git = (self._working_dir / ".git").exists()

        # Git fields
        git_branch = None
        git_default_branch = None
        git_status = None
        git_recent_commits = None
        git_remote_url = None
        if is_git:
            git_branch = self._run_git("rev-parse", "--abbrev-ref", "HEAD")
            git_default_branch = self._detect_default_branch()
            git_status = self._collect_git_status()
            git_recent_commits = self._run_git("log", "--oneline", "-5")
            git_remote_url = self._run_git("config", "--get", "remote.origin.url")

        # Shell & runtime
        venv = os.environ.get("VIRTUAL_ENV")
        virtual_env = Path(venv).name if venv else None

        node_version = self._detect_node_version()
        pkg_managers = tuple(m for m in _PACKAGE_MANAGERS if shutil.which(m))

        # Project structure
        config_files = tuple(f for f in _CONFIG_FILES if (self._working_dir / f).exists())
        tech_stack = self._infer_tech_stack(config_files)
        tree = self._build_directory_tree()

        # Project instructions
        project_instructions = self._load_project_instructions()

        return EnvironmentContext(
            working_dir=str(self._working_dir),
            platform=self._map_platform(),
            os_version=f"{platform.system()} {platform.release()}",
            current_date=datetime.date.today().isoformat(),
            model=self._config.model or "",
            model_provider=self._config.model_provider or "",
            is_git_repo=is_git,
            git_branch=git_branch,
            git_default_branch=git_default_branch,
            git_status=git_status,
            git_recent_commits=git_recent_commits,
            git_remote_url=git_remote_url,
            shell=os.environ.get("SHELL"),
            python_version=platform.python_version(),
            virtual_env=virtual_env,
            node_version=node_version,
            available_package_managers=pkg_managers,
            project_config_files=config_files,
            tech_stack=tech_stack,
            directory_tree=tree,
            project_instructions=project_instructions,
        )

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _map_platform() -> str:
        sys_name = platform.system()
        return {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}.get(sys_name, sys_name)

    def _run_git(self, *args: str) -> str | None:
        try:
            result = subprocess.run(
                ["git", *args],
                capture_output=True,
                text=True,
                cwd=self._working_dir,
                timeout=5,
            )
            if result.returncode == 0:
                return result.stdout.strip() or None
        except Exception:
            pass
        return None

    def _detect_default_branch(self) -> str | None:
        # Try symbolic-ref first
        ref = self._run_git("symbolic-ref", "refs/remotes/origin/HEAD")
        if ref:
            return ref.rsplit("/", 1)[-1]
        # Fallback: check if main or master exists
        for branch in ("main", "master"):
            check = self._run_git("rev-parse", "--verify", branch)
            if check:
                return branch
        return None

    def _collect_git_status(self) -> str:
        raw = self._run_git("status", "--porcelain")
        if not raw:
            return "(clean)"
        return raw

    def _detect_node_version(self) -> str | None:
        if not shutil.which("node"):
            return None
        try:
            result = subprocess.run(
                ["node", "--version"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode == 0:
                return result.stdout.strip() or None
        except Exception:
            pass
        return None

    def _infer_tech_stack(self, config_files: tuple[str, ...]) -> str | None:
        if "Cargo.toml" in config_files:
            return "Rust"
        if "go.mod" in config_files:
            return "Go"
        if "pom.xml" in config_files or "build.gradle" in config_files:
            return "Java"
        if "Gemfile" in config_files:
            return "Ruby"
        if "composer.json" in config_files:
            return "PHP"
        if "CMakeLists.txt" in config_files:
            return "C/C++"
        if "package.json" in config_files:
            return self._detect_js_framework()
        if "pyproject.toml" in config_files or "setup.py" in config_files:
            return "Python"
        if "requirements.txt" in config_files:
            return "Python"
        return None

    def _detect_js_framework(self) -> str:
        pkg_json = self._working_dir / "package.json"
        if not pkg_json.exists():
            return "JavaScript / Node.js"
        try:
            import json

            data = json.loads(pkg_json.read_text())
            deps = {**data.get("dependencies", {}), **data.get("devDependencies", {})}
            if "next" in deps:
                return "TypeScript / Next.js"
            if "react" in deps:
                return "TypeScript / React" if "typescript" in deps else "JavaScript / React"
            if "vue" in deps:
                return "JavaScript / Vue"
            if "angular" in deps or "@angular/core" in deps:
                return "TypeScript / Angular"
            if "svelte" in deps:
                return "JavaScript / Svelte"
            if "typescript" in deps:
                return "TypeScript"
        except Exception:
            pass
        return "JavaScript / Node.js"

    def _build_directory_tree(self) -> str | None:
        try:
            return self._walk_tree(self._working_dir, depth=2)
        except Exception:
            return None

    def _walk_tree(self, root: Path, depth: int) -> str:
        lines: list[str] = ["."]
        self._tree_recurse(root, "", depth, lines)
        return "\n".join(lines)

    def _tree_recurse(self, path: Path, prefix: str, remaining: int, lines: list[str]) -> None:
        if remaining <= 0:
            return
        try:
            entries = sorted(path.iterdir(), key=lambda e: (not e.is_dir(), e.name.lower()))
        except PermissionError:
            return

        # Filter entries
        visible: list[Path] = []
        for entry in entries:
            name = entry.name
            if name.startswith(".") and name != ".opendev":
                continue
            if name in _TREE_SKIP_DIRS:
                continue
            # Skip egg-info dirs
            if name.endswith(".egg-info"):
                continue
            visible.append(entry)

        for i, entry in enumerate(visible):
            is_last = i == len(visible) - 1
            connector = "\u2514\u2500\u2500 " if is_last else "\u251c\u2500\u2500 "
            suffix = "/" if entry.is_dir() else ""
            lines.append(f"{prefix}{connector}{entry.name}{suffix}")
            if entry.is_dir():
                extension = "    " if is_last else "\u2502   "
                self._tree_recurse(entry, prefix + extension, remaining - 1, lines)

    def _load_project_instructions(self) -> str | None:
        if not self._config_manager:
            return None
        try:
            contexts = self._config_manager.load_context_files()
            if contexts:
                return "\n\n".join(contexts)
        except Exception:
            pass
        return None


# ======================================================================
# Formatting functions
# ======================================================================


def build_env_block(ctx: EnvironmentContext) -> str:
    """Build lean <env> block with core runtime facts."""
    lines = ["Here is useful information about the environment you are running in:", "<env>"]
    lines.append(f"Working directory: {ctx.working_dir}")
    lines.append(f"Is directory a git repo: {'Yes' if ctx.is_git_repo else 'No'}")
    lines.append(f"Platform: {ctx.platform}")
    lines.append(f"OS Version: {ctx.os_version}")
    lines.append(f"Today's date: {ctx.current_date}")
    lines.append(f"Model: {ctx.model_provider}/{ctx.model}")
    if ctx.shell:
        lines.append(f"Shell: {ctx.shell}")
    if ctx.python_version:
        lines.append(f"Python: {ctx.python_version}")
    if ctx.virtual_env:
        lines.append(f"Virtual env: {ctx.virtual_env}")
    if ctx.node_version:
        lines.append(f"Node: {ctx.node_version}")
    if ctx.available_package_managers:
        lines.append(f"Available package managers: {', '.join(ctx.available_package_managers)}")
    lines.append("</env>")
    return "\n".join(lines)


def build_git_status_block(ctx: EnvironmentContext) -> str:
    """Build git status snapshot block. Returns empty string if not a git repo."""
    if not ctx.is_git_repo:
        return ""

    lines = [
        "gitStatus: This is the git status at the start of the conversation. "
        "Note that this status is a snapshot in time, and will not update during the conversation."
    ]
    if ctx.git_branch:
        lines.append(f"Current branch: {ctx.git_branch}")
    if ctx.git_default_branch:
        lines.append(f"\nMain branch (you will usually use this for PRs): {ctx.git_default_branch}")
    if ctx.git_remote_url:
        lines.append(f"\nRemote: {ctx.git_remote_url}")
    if ctx.git_status:
        lines.append(f"\nStatus:\n{ctx.git_status}")
    if ctx.git_recent_commits:
        lines.append(f"\nRecent commits:\n{ctx.git_recent_commits}")
    return "\n".join(lines)


def build_project_structure_block(ctx: EnvironmentContext) -> str:
    """Build project structure block. Returns empty string if no data."""
    parts: list[str] = []
    if ctx.project_config_files:
        parts.append(f"Detected config files: {', '.join(ctx.project_config_files)}")
    if ctx.tech_stack:
        parts.append(f"Tech stack: {ctx.tech_stack}")
    if ctx.directory_tree:
        parts.append(f"\nDirectory tree (depth 2):\n{ctx.directory_tree}")

    if not parts:
        return ""

    return "Project structure:\n" + "\n".join(parts)


def build_project_instructions_block(ctx: EnvironmentContext) -> str:
    """Wrap OPENDEV.md content as project instructions block."""
    if not ctx.project_instructions:
        return ""
    return (
        "# Project Instructions (OPENDEV.md)\n\n"
        "Codebase and user instructions are shown below. "
        "Be sure to adhere to these instructions.\n\n"
        f"<opendev-md>\n{ctx.project_instructions}\n</opendev-md>"
    )
