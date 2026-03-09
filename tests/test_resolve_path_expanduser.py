"""Tests that _resolve_path expands ~ in all file tool implementations."""

from pathlib import Path
from unittest.mock import MagicMock

import pytest

from opendev.core.context_engineering.tools.implementations.write_tool import WriteTool
from opendev.core.context_engineering.tools.implementations.edit_tool.tool import EditTool
from opendev.core.context_engineering.tools.implementations.file_ops import FileOperations


@pytest.fixture
def mock_config():
    return MagicMock()


@pytest.fixture
def working_dir(tmp_path):
    return tmp_path


class TestWriteToolResolvePath:
    def test_tilde_path_expands_to_home(self, mock_config, working_dir):
        tool = WriteTool(config=mock_config, working_dir=working_dir)
        result = tool._resolve_path("~/.opendev/plans/test.md")
        assert result == Path.home() / ".opendev/plans/test.md"
        assert "~" not in str(result)

    def test_absolute_path_unchanged(self, mock_config, working_dir):
        tool = WriteTool(config=mock_config, working_dir=working_dir)
        result = tool._resolve_path("/tmp/test.md")
        assert result == Path("/tmp/test.md")

    def test_relative_path_resolved_to_working_dir(self, mock_config, working_dir):
        tool = WriteTool(config=mock_config, working_dir=working_dir)
        result = tool._resolve_path("foo/bar.md")
        assert result == (working_dir / "foo/bar.md").resolve()


class TestEditToolResolvePath:
    def test_tilde_path_expands_to_home(self, mock_config, working_dir):
        tool = EditTool(config=mock_config, working_dir=working_dir)
        result = tool._resolve_path("~/.opendev/plans/test.md")
        assert result == Path.home() / ".opendev/plans/test.md"
        assert "~" not in str(result)


class TestFileOpsResolvePath:
    def test_tilde_path_expands_to_home(self, mock_config, working_dir):
        tool = FileOperations(config=mock_config, working_dir=working_dir)
        result = tool._resolve_path("~/.opendev/plans/test.md")
        assert result == Path.home() / ".opendev/plans/test.md"
        assert "~" not in str(result)
