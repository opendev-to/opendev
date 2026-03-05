"""Tests for centralized paths module."""

import os
from pathlib import Path

import pytest

from swecli.core.paths import (
    APP_DIR_NAME,
    SETTINGS_FILE_NAME,
    SESSIONS_DIR_NAME,
    LOGS_DIR_NAME,
    CACHE_DIR_NAME,
    SKILLS_DIR_NAME,
    AGENTS_DIR_NAME,
    COMMANDS_DIR_NAME,
    MCP_CONFIG_NAME,
    MCP_PROJECT_CONFIG_NAME,
    ENV_OPENDEV_DIR,
    ENV_OPENDEV_SESSION_DIR,
    ENV_OPENDEV_LOG_DIR,
    ENV_OPENDEV_CACHE_DIR,
    Paths,
    get_paths,
    set_paths,
    reset_paths,
)


class TestPathsConstants:
    """Test path constants."""

    def test_app_dir_name(self):
        assert APP_DIR_NAME == ".opendev"

    def test_settings_file_name(self):
        assert SETTINGS_FILE_NAME == "settings.json"

    def test_sessions_dir_name(self):
        assert SESSIONS_DIR_NAME == "sessions"

    def test_logs_dir_name(self):
        assert LOGS_DIR_NAME == "logs"

    def test_cache_dir_name(self):
        assert CACHE_DIR_NAME == "cache"

    def test_skills_dir_name(self):
        assert SKILLS_DIR_NAME == "skills"

    def test_agents_dir_name(self):
        assert AGENTS_DIR_NAME == "agents"

    def test_commands_dir_name(self):
        assert COMMANDS_DIR_NAME == "commands"

    def test_mcp_config_name(self):
        assert MCP_CONFIG_NAME == "mcp.json"

    def test_mcp_project_config_name(self):
        assert MCP_PROJECT_CONFIG_NAME == ".mcp.json"


class TestPathsGlobal:
    """Test global paths."""

    def test_global_dir_default(self, tmp_path, monkeypatch):
        """Test global dir defaults to ~/.opendev/."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_dir == tmp_path / ".opendev"

    def test_global_dir_env_override(self, tmp_path, monkeypatch):
        """Test OPENDEV_DIR environment variable overrides default."""
        custom_dir = tmp_path / "custom-swecli"
        monkeypatch.setenv(ENV_OPENDEV_DIR, str(custom_dir))
        reset_paths()
        paths = Paths()
        assert paths.global_dir == custom_dir

    def test_global_settings(self, tmp_path, monkeypatch):
        """Test global settings path."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_settings == tmp_path / ".opendev" / "settings.json"

    def test_global_sessions_dir_default(self, tmp_path, monkeypatch):
        """Test global sessions directory defaults to ~/.opendev/sessions/."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        monkeypatch.delenv(ENV_OPENDEV_SESSION_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_sessions_dir == tmp_path / ".opendev" / "sessions"

    def test_global_sessions_dir_env_override(self, tmp_path, monkeypatch):
        """Test OPENDEV_SESSION_DIR environment variable overrides default."""
        custom_sessions = tmp_path / "custom-sessions"
        monkeypatch.setenv(ENV_OPENDEV_SESSION_DIR, str(custom_sessions))
        reset_paths()
        paths = Paths()
        assert paths.global_sessions_dir == custom_sessions

    def test_global_logs_dir_default(self, tmp_path, monkeypatch):
        """Test global logs directory defaults to ~/.opendev/logs/."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        monkeypatch.delenv(ENV_OPENDEV_LOG_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_logs_dir == tmp_path / ".opendev" / "logs"

    def test_global_logs_dir_env_override(self, tmp_path, monkeypatch):
        """Test OPENDEV_LOG_DIR environment variable overrides default."""
        custom_logs = tmp_path / "custom-logs"
        monkeypatch.setenv(ENV_OPENDEV_LOG_DIR, str(custom_logs))
        reset_paths()
        paths = Paths()
        assert paths.global_logs_dir == custom_logs

    def test_global_cache_dir_default(self, tmp_path, monkeypatch):
        """Test global cache directory defaults to ~/.opendev/cache/."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        monkeypatch.delenv(ENV_OPENDEV_CACHE_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_cache_dir == tmp_path / ".opendev" / "cache"

    def test_global_cache_dir_env_override(self, tmp_path, monkeypatch):
        """Test OPENDEV_CACHE_DIR environment variable overrides default."""
        custom_cache = tmp_path / "custom-cache"
        monkeypatch.setenv(ENV_OPENDEV_CACHE_DIR, str(custom_cache))
        reset_paths()
        paths = Paths()
        assert paths.global_cache_dir == custom_cache

    def test_global_skills_dir(self, tmp_path, monkeypatch):
        """Test global skills directory."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_skills_dir == tmp_path / ".opendev" / "skills"

    def test_global_agents_dir(self, tmp_path, monkeypatch):
        """Test global agents directory."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_agents_dir == tmp_path / ".opendev" / "agents"

    def test_global_agents_file(self, tmp_path, monkeypatch):
        """Test global agents.json file path."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_agents_file == tmp_path / ".opendev" / "agents.json"

    def test_global_mcp_config(self, tmp_path, monkeypatch):
        """Test global MCP config path."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_mcp_config == tmp_path / ".opendev" / "mcp.json"

    def test_global_repos_dir(self, tmp_path, monkeypatch):
        """Test global repos directory."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_repos_dir == tmp_path / ".opendev" / "repos"

    def test_global_history_file(self, tmp_path, monkeypatch):
        """Test global history file path."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths()
        assert paths.global_history_file == tmp_path / ".opendev" / "history.txt"


class TestPathsProject:
    """Test project paths."""

    def test_project_dir(self, tmp_path):
        """Test project directory path."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_dir == tmp_path / ".opendev"

    def test_project_settings(self, tmp_path):
        """Test project settings path."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_settings == tmp_path / ".opendev" / "settings.json"

    def test_project_skills_dir(self, tmp_path):
        """Test project skills directory."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_skills_dir == tmp_path / ".opendev" / "skills"

    def test_project_agents_dir(self, tmp_path):
        """Test project agents directory."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_agents_dir == tmp_path / ".opendev" / "agents"

    def test_project_agents_file(self, tmp_path):
        """Test project agents.json file path."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_agents_file == tmp_path / ".opendev" / "agents.json"

    def test_project_commands_dir(self, tmp_path):
        """Test project commands directory."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_commands_dir == tmp_path / ".opendev" / "commands"

    def test_project_context_file(self, tmp_path):
        """Test project context file (SWECLI.md) at project root."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_context_file == tmp_path / "OPENDEV.md"

    def test_project_mcp_config(self, tmp_path):
        """Test project MCP config uses .mcp.json at project root."""
        paths = Paths(working_dir=tmp_path)
        assert paths.project_mcp_config == tmp_path / ".mcp.json"


class TestPathsDirectoryCreation:
    """Test directory creation methods."""

    def test_ensure_global_dirs(self, tmp_path, monkeypatch):
        """Test ensure_global_dirs creates required directories."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        monkeypatch.delenv(ENV_OPENDEV_SESSION_DIR, raising=False)
        monkeypatch.delenv(ENV_OPENDEV_LOG_DIR, raising=False)
        monkeypatch.delenv(ENV_OPENDEV_CACHE_DIR, raising=False)
        reset_paths()
        paths = Paths()
        paths.ensure_global_dirs()

        assert paths.global_dir.exists()
        assert paths.global_sessions_dir.exists()
        assert paths.global_logs_dir.exists()
        assert paths.global_cache_dir.exists()
        assert paths.global_skills_dir.exists()
        assert paths.global_agents_dir.exists()

    def test_ensure_project_dirs_with_git(self, tmp_path):
        """Test ensure_project_dirs creates commands dir when .git exists."""
        (tmp_path / ".git").mkdir()
        paths = Paths(working_dir=tmp_path)
        paths.ensure_project_dirs()

        assert paths.project_commands_dir.exists()

    def test_ensure_project_dirs_without_git(self, tmp_path):
        """Test ensure_project_dirs skips commands dir without .git."""
        paths = Paths(working_dir=tmp_path)
        paths.ensure_project_dirs()

        # Should NOT create commands dir without .git
        assert not paths.project_commands_dir.exists()


class TestPathsHelpers:
    """Test helper methods."""

    def test_get_skill_dirs_empty(self, tmp_path, monkeypatch):
        """Test get_skill_dirs returns only builtin when no user dirs exist."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths(working_dir=tmp_path / "project")
        dirs = paths.get_skill_dirs()
        # Only builtin skills dir (always exists as part of the package)
        assert len(dirs) == 1
        assert dirs[0] == paths.builtin_skills_dir

    def test_get_skill_dirs_with_global(self, tmp_path, monkeypatch):
        """Test get_skill_dirs returns global dir before builtin."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()

        # Create global skills dir
        global_skills = tmp_path / ".opendev" / "skills"
        global_skills.mkdir(parents=True)

        paths = Paths(working_dir=tmp_path / "project")
        dirs = paths.get_skill_dirs()

        assert len(dirs) == 2
        assert dirs[0] == global_skills
        assert dirs[1] == paths.builtin_skills_dir  # Builtin last

    def test_get_skill_dirs_project_priority(self, tmp_path, monkeypatch):
        """Test get_skill_dirs returns project dir first, builtin last."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()

        project_dir = tmp_path / "project"

        # Create both global and project skills dirs
        global_skills = tmp_path / ".opendev" / "skills"
        global_skills.mkdir(parents=True)
        project_skills = project_dir / ".opendev" / "skills"
        project_skills.mkdir(parents=True)

        paths = Paths(working_dir=project_dir)
        dirs = paths.get_skill_dirs()

        assert len(dirs) == 3
        assert dirs[0] == project_skills  # Project first
        assert dirs[1] == global_skills   # Global second
        assert dirs[2] == paths.builtin_skills_dir  # Builtin last

    def test_get_agents_dirs_empty(self, tmp_path, monkeypatch):
        """Test get_agents_dirs returns empty list when no dirs exist."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()
        paths = Paths(working_dir=tmp_path / "project")
        assert paths.get_agents_dirs() == []

    def test_get_agents_dirs_project_priority(self, tmp_path, monkeypatch):
        """Test get_agents_dirs returns project dir first."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        reset_paths()

        project_dir = tmp_path / "project"

        # Create both global and project agents dirs
        global_agents = tmp_path / ".opendev" / "agents"
        global_agents.mkdir(parents=True)
        project_agents = project_dir / ".opendev" / "agents"
        project_agents.mkdir(parents=True)

        paths = Paths(working_dir=project_dir)
        dirs = paths.get_agents_dirs()

        assert len(dirs) == 2
        assert dirs[0] == project_agents  # Project first
        assert dirs[1] == global_agents   # Global second

    def test_session_file(self, tmp_path, monkeypatch):
        """Test session_file returns correct path."""
        monkeypatch.setenv("HOME", str(tmp_path))
        monkeypatch.delenv(ENV_OPENDEV_DIR, raising=False)
        monkeypatch.delenv(ENV_OPENDEV_SESSION_DIR, raising=False)
        reset_paths()

        paths = Paths()
        session_path = paths.session_file("abc12345")

        assert session_path == tmp_path / ".opendev" / "sessions" / "abc12345.json"


class TestPathsSingleton:
    """Test singleton access functions."""

    def test_get_paths_singleton(self, monkeypatch):
        """Test get_paths returns same instance."""
        reset_paths()
        paths1 = get_paths()
        paths2 = get_paths()
        assert paths1 is paths2

    def test_get_paths_with_working_dir(self, tmp_path):
        """Test get_paths with working_dir returns new instance."""
        reset_paths()
        paths1 = get_paths()
        paths2 = get_paths(working_dir=tmp_path)

        # Should be different instances
        assert paths1 is not paths2
        assert paths2.working_dir == tmp_path

    def test_set_paths(self, tmp_path):
        """Test set_paths replaces singleton."""
        reset_paths()
        custom_paths = Paths(working_dir=tmp_path)
        set_paths(custom_paths)

        assert get_paths() is custom_paths

    def test_reset_paths(self):
        """Test reset_paths clears singleton."""
        # First get a singleton
        paths1 = get_paths()

        # Reset
        reset_paths()

        # Should get new instance
        paths2 = get_paths()
        assert paths1 is not paths2


class TestPathsWorkingDir:
    """Test working directory handling."""

    def test_working_dir_property(self, tmp_path):
        """Test working_dir property returns correct value."""
        paths = Paths(working_dir=tmp_path)
        assert paths.working_dir == tmp_path

    def test_working_dir_defaults_to_cwd(self, tmp_path, monkeypatch):
        """Test working_dir defaults to current working directory."""
        monkeypatch.chdir(tmp_path)
        paths = Paths()
        assert paths.working_dir == tmp_path
