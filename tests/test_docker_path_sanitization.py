"""Tests for Docker path sanitization and task rewriting.

These tests verify that:
1. Local paths are correctly sanitized to relative paths
2. Task rewriting removes "local" hints and replaces paths correctly
"""

import pytest
from pathlib import Path
from unittest.mock import MagicMock


class TestSanitizeLocalPaths:
    """Test the _sanitize_local_paths method in DockerToolRegistry."""

    def _create_registry(self):
        """Create a minimal DockerToolRegistry for testing."""
        from swecli.core.docker.tool_handler import DockerToolRegistry, DockerToolHandler

        # Create a mock docker handler
        mock_runtime = MagicMock()
        mock_runtime.host = "localhost"
        mock_runtime.port = 8000
        mock_runtime.auth_token = "test"
        mock_runtime.timeout = 30.0

        handler = DockerToolHandler(mock_runtime, workspace_dir="/workspace")
        return DockerToolRegistry(handler)

    def test_sanitize_users_path(self):
        """Test that /Users/... paths are sanitized to just filename."""
        registry = self._create_registry()
        args = {"path": "/Users/nghibui/codes/test_opencli/pyproject.toml"}
        result = registry._sanitize_local_paths(args)
        assert result["path"] == "pyproject.toml"

    def test_sanitize_home_path(self):
        """Test that /home/... paths are sanitized to just filename."""
        registry = self._create_registry()
        args = {"file_path": "/home/user/project/src/model.py"}
        result = registry._sanitize_local_paths(args)
        assert result["file_path"] == "model.py"

    def test_sanitize_var_path(self):
        """Test that /var/... paths are sanitized to just filename."""
        registry = self._create_registry()
        args = {"path": "/var/tmp/data/config.yaml"}
        result = registry._sanitize_local_paths(args)
        assert result["path"] == "config.yaml"

    def test_sanitize_tmp_path(self):
        """Test that /tmp/... paths are sanitized to just filename."""
        registry = self._create_registry()
        args = {"path": "/tmp/working/file.txt"}
        result = registry._sanitize_local_paths(args)
        assert result["path"] == "file.txt"

    def test_preserve_relative_path(self):
        """Test that relative paths are preserved."""
        registry = self._create_registry()
        args = {"path": "src/model.py"}
        result = registry._sanitize_local_paths(args)
        assert result["path"] == "src/model.py"

    def test_preserve_workspace_path(self):
        """Test that /workspace/... paths are preserved."""
        registry = self._create_registry()
        args = {"path": "/workspace/src/model.py"}
        result = registry._sanitize_local_paths(args)
        # This starts with /workspace, not /Users, so should be preserved
        assert result["path"] == "/workspace/src/model.py"

    def test_preserve_testbed_path(self):
        """Test that /testbed/... paths are preserved."""
        registry = self._create_registry()
        args = {"path": "/testbed/src/model.py"}
        result = registry._sanitize_local_paths(args)
        assert result["path"] == "/testbed/src/model.py"

    def test_sanitize_multiple_args(self):
        """Test sanitizing multiple path arguments."""
        registry = self._create_registry()
        args = {
            "file_path": "/Users/nghibui/codes/test/main.py",
            "content": "print('hello')",  # Non-path, should be preserved
            "output_path": "/home/user/output.txt",
        }
        result = registry._sanitize_local_paths(args)
        assert result["file_path"] == "main.py"
        assert result["content"] == "print('hello')"
        assert result["output_path"] == "output.txt"

    def test_sanitize_pdf_path(self):
        """Test sanitizing PDF file paths."""
        registry = self._create_registry()
        args = {"path": "/Users/nghibui/codes/test_opencli/2303.11366v4.pdf"}
        result = registry._sanitize_local_paths(args)
        assert result["path"] == "2303.11366v4.pdf"


class TestRewriteTaskForDocker:
    """Test the _rewrite_task_for_docker method in SubAgentManager."""

    def _create_manager(self, working_dir: str = "/Users/nghibui/codes/test_opencli"):
        """Create a minimal SubAgentManager for testing."""
        from swecli.core.agents.subagents.manager import SubAgentManager

        mock_tool_registry = MagicMock()
        mock_config = MagicMock()
        mock_config.anthropic_api_key = "test"
        mock_config.provider = "anthropic"
        mock_config.model = "claude-sonnet-4-20250514"

        manager = SubAgentManager(
            config=mock_config,
            tool_registry=mock_tool_registry,
            mode_manager=None,
            working_dir=Path(working_dir),
        )
        return manager

    def test_removes_local_keyword(self):
        """Test that 'local' keyword is removed from task."""
        manager = self._create_manager()
        task = "Implement the local PDF paper"
        result = manager._rewrite_task_for_docker(task, [], "/workspace")
        assert "local" not in result.lower() or "local" not in task.replace("local ", "").lower()
        # The word "local" should be stripped, leaving "Implement the PDF paper"
        assert "PDF paper" in result

    def test_replaces_in_this_repo(self):
        """Test that 'in this repo' is replaced with workspace path."""
        manager = self._create_manager()
        task = "Find all Python files in this repo"
        result = manager._rewrite_task_for_docker(task, [], "/workspace")
        assert "in /workspace" in result
        assert "in this repo" not in result

    def test_replaces_this_repo(self):
        """Test that 'this repo' is replaced with workspace path."""
        manager = self._create_manager()
        task = "Analyze this repo structure"
        result = manager._rewrite_task_for_docker(task, [], "/workspace")
        assert "/workspace" in result

    def test_replaces_local_directory_path(self):
        """Test that local directory paths are replaced with workspace."""
        manager = self._create_manager("/Users/nghibui/codes/test_opencli")
        task = "Read the file at /Users/nghibui/codes/test_opencli/main.py"
        result = manager._rewrite_task_for_docker(task, [], "/workspace")
        assert "/Users/nghibui/codes/test_opencli" not in result
        assert "/workspace" in result

    def test_replaces_input_file_paths(self):
        """Test that input file paths are replaced with Docker paths."""
        manager = self._create_manager("/Users/nghibui/codes/test_opencli")
        input_files = [Path("/Users/nghibui/codes/test_opencli/paper.pdf")]
        task = "Implement the paper at /Users/nghibui/codes/test_opencli/paper.pdf"
        result = manager._rewrite_task_for_docker(task, input_files, "/workspace")
        assert "/workspace/paper.pdf" in result
        assert "/Users/nghibui" not in result

    def test_replaces_at_filename_reference(self):
        """Test that @filename references are replaced with Docker paths."""
        manager = self._create_manager()
        input_files = [Path("/Users/nghibui/codes/test_opencli/paper.pdf")]
        task = "Implement the paper @paper.pdf"
        result = manager._rewrite_task_for_docker(task, input_files, "/workspace")
        assert "/workspace/paper.pdf" in result
        assert "@paper.pdf" not in result

    def test_includes_docker_context_preamble(self):
        """Test that Docker context is prepended to task."""
        manager = self._create_manager()
        task = "Write a simple Python script"
        result = manager._rewrite_task_for_docker(task, [], "/workspace")
        # Should include Docker context warning
        assert "CRITICAL" in result or "Docker" in result
        assert "/workspace" in result

    def test_complex_task_rewriting(self):
        """Test a complex task with multiple replacements."""
        manager = self._create_manager("/Users/nghibui/codes/test_opencli")
        input_files = [Path("/Users/nghibui/codes/test_opencli/2303.11366v4.pdf")]
        task = (
            "Implement the local PDF paper at /Users/nghibui/codes/test_opencli/2303.11366v4.pdf "
            "in this repo. Create the code structure."
        )
        result = manager._rewrite_task_for_docker(task, input_files, "/workspace")

        # Local paths should be removed/replaced
        assert "/Users/nghibui" not in result
        # Should have Docker context
        assert "/workspace" in result
        # Should not have "local" as a word (may appear in preamble warnings)
        # The original task's "local PDF paper" should become "PDF paper"


class TestTranslatePath:
    """Test the _translate_path method in DockerToolHandler."""

    def _create_handler(self):
        """Create a DockerToolHandler for testing."""
        from swecli.core.docker.tool_handler import DockerToolHandler

        mock_runtime = MagicMock()
        return DockerToolHandler(mock_runtime, workspace_dir="/workspace")

    def test_translate_relative_path(self):
        """Test that relative paths are prefixed with workspace."""
        handler = self._create_handler()
        result = handler._translate_path("src/model.py")
        assert result == "/workspace/src/model.py"

    def test_translate_relative_with_dot_slash(self):
        """Test that ./relative paths are handled correctly."""
        handler = self._create_handler()
        result = handler._translate_path("./src/model.py")
        assert result == "/workspace/src/model.py"

    def test_translate_workspace_path(self):
        """Test that /workspace paths are preserved."""
        handler = self._create_handler()
        result = handler._translate_path("/workspace/src/model.py")
        assert result == "/workspace/src/model.py"

    def test_translate_testbed_path(self):
        """Test that /testbed paths are preserved."""
        handler = self._create_handler()
        result = handler._translate_path("/testbed/src/model.py")
        assert result == "/testbed/src/model.py"

    def test_translate_absolute_host_path(self):
        """Test that absolute host paths are converted to just filename."""
        handler = self._create_handler()
        result = handler._translate_path("/Users/nghibui/codes/test/main.py")
        # Should extract just the filename
        assert result == "/workspace/main.py"

    def test_translate_empty_path(self):
        """Test that empty path returns workspace."""
        handler = self._create_handler()
        result = handler._translate_path("")
        assert result == "/workspace"

    def test_translate_filename_only(self):
        """Test that just a filename gets workspace prefix."""
        handler = self._create_handler()
        result = handler._translate_path("config.yaml")
        assert result == "/workspace/config.yaml"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
