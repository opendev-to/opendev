"""Unit tests for DockerToolHandler argument name handling."""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch


class TestDockerToolHandlerArgumentNames:
    """Test that DockerToolHandler accepts both standard and legacy argument names."""

    @pytest.fixture
    def mock_runtime(self):
        """Create a mock RemoteRuntime."""
        runtime = MagicMock()
        runtime.host = "localhost"
        runtime.port = 8080
        runtime.auth_token = "test-token"
        runtime.timeout = 30.0
        runtime.read_file = AsyncMock(return_value="file content")
        runtime.write_file = AsyncMock(return_value=None)
        return runtime

    @pytest.fixture
    def handler(self, mock_runtime):
        """Create a DockerToolHandler with mocked runtime."""
        from swecli.core.docker.tool_handler import DockerToolHandler
        return DockerToolHandler(mock_runtime, workspace_dir="/workspace/repo")

    # --- read_file tests ---

    @pytest.mark.asyncio
    async def test_read_file_with_file_path_arg(self, handler, mock_runtime):
        """Test read_file accepts 'file_path' (standard naming)."""
        result = await handler.read_file({"file_path": "/workspace/repo/test.py"})

        assert result["success"] is True
        assert result["content"] == "file content"
        mock_runtime.read_file.assert_called_once_with("/workspace/repo/test.py")

    @pytest.mark.asyncio
    async def test_read_file_with_path_arg(self, handler, mock_runtime):
        """Test read_file accepts 'path' (legacy naming)."""
        result = await handler.read_file({"path": "/workspace/repo/test.py"})

        assert result["success"] is True
        assert result["content"] == "file content"
        mock_runtime.read_file.assert_called_once_with("/workspace/repo/test.py")

    @pytest.mark.asyncio
    async def test_read_file_prefers_file_path_over_path(self, handler, mock_runtime):
        """Test that 'file_path' takes precedence over 'path'."""
        result = await handler.read_file({
            "file_path": "/workspace/repo/preferred.py",
            "path": "/workspace/repo/legacy.py",
        })

        assert result["success"] is True
        mock_runtime.read_file.assert_called_once_with("/workspace/repo/preferred.py")

    @pytest.mark.asyncio
    async def test_read_file_requires_path(self, handler):
        """Test read_file fails when no path provided."""
        result = await handler.read_file({})

        assert result["success"] is False
        assert "required" in result["error"]

    # --- write_file tests ---

    @pytest.mark.asyncio
    async def test_write_file_with_file_path_arg(self, handler, mock_runtime):
        """Test write_file accepts 'file_path' (standard naming)."""
        result = await handler.write_file({
            "file_path": "/workspace/repo/test.py",
            "content": "new content",
        })

        assert result["success"] is True
        mock_runtime.write_file.assert_called_once_with("/workspace/repo/test.py", "new content")

    @pytest.mark.asyncio
    async def test_write_file_with_path_arg(self, handler, mock_runtime):
        """Test write_file accepts 'path' (legacy naming)."""
        result = await handler.write_file({
            "path": "/workspace/repo/test.py",
            "content": "new content",
        })

        assert result["success"] is True
        mock_runtime.write_file.assert_called_once_with("/workspace/repo/test.py", "new content")

    # --- edit_file tests ---

    @pytest.mark.asyncio
    async def test_edit_file_with_standard_args(self, handler, mock_runtime):
        """Test edit_file accepts standard naming (file_path, old_content, new_content)."""
        mock_runtime.read_file = AsyncMock(return_value="old code here")

        result = await handler.edit_file({
            "file_path": "/workspace/repo/test.py",
            "old_content": "old code",
            "new_content": "new code",
        })

        assert result["success"] is True
        mock_runtime.read_file.assert_called_once_with("/workspace/repo/test.py")
        mock_runtime.write_file.assert_called_once_with(
            "/workspace/repo/test.py", "new code here"
        )

    @pytest.mark.asyncio
    async def test_edit_file_with_legacy_args(self, handler, mock_runtime):
        """Test edit_file accepts legacy naming (path, old_text, new_text)."""
        mock_runtime.read_file = AsyncMock(return_value="old code here")

        result = await handler.edit_file({
            "path": "/workspace/repo/test.py",
            "old_text": "old code",
            "new_text": "new code",
        })

        assert result["success"] is True
        mock_runtime.read_file.assert_called_once_with("/workspace/repo/test.py")
        mock_runtime.write_file.assert_called_once_with(
            "/workspace/repo/test.py", "new code here"
        )

    @pytest.mark.asyncio
    async def test_edit_file_prefers_standard_over_legacy(self, handler, mock_runtime):
        """Test that standard names take precedence over legacy names."""
        mock_runtime.read_file = AsyncMock(return_value="standard text here")

        result = await handler.edit_file({
            "file_path": "/workspace/repo/standard.py",
            "path": "/workspace/repo/legacy.py",
            "old_content": "standard text",
            "old_text": "legacy text",
            "new_content": "replaced",
            "new_text": "wrong",
        })

        assert result["success"] is True
        # Should use file_path, not path
        mock_runtime.read_file.assert_called_once_with("/workspace/repo/standard.py")
        # Should use old_content/new_content, not old_text/new_text
        mock_runtime.write_file.assert_called_once_with(
            "/workspace/repo/standard.py", "replaced here"
        )

    @pytest.mark.asyncio
    async def test_edit_file_requires_path(self, handler):
        """Test edit_file fails when no path provided."""
        result = await handler.edit_file({
            "old_content": "old",
            "new_content": "new",
        })

        assert result["success"] is False
        assert "required" in result["error"]

    @pytest.mark.asyncio
    async def test_edit_file_requires_old_content(self, handler):
        """Test edit_file fails when no old_content/old_text provided."""
        result = await handler.edit_file({
            "file_path": "/workspace/repo/test.py",
            "new_content": "new",
        })

        assert result["success"] is False
        assert "required" in result["error"]

    @pytest.mark.asyncio
    async def test_edit_file_old_text_not_found(self, handler, mock_runtime):
        """Test edit_file fails when old_text is not in the file."""
        mock_runtime.read_file = AsyncMock(return_value="different content")

        result = await handler.edit_file({
            "file_path": "/workspace/repo/test.py",
            "old_content": "not found text",
            "new_content": "new",
        })

        assert result["success"] is False
        assert "not found" in result["error"]

    # --- search tests ---

    @pytest.mark.asyncio
    async def test_search_with_pattern_arg(self, handler, mock_runtime):
        """Test search accepts 'pattern' (standard naming)."""
        mock_obs = MagicMock()
        mock_obs.output = "test.py:10:match"
        mock_obs.exit_code = 0
        mock_runtime.run = AsyncMock(return_value=mock_obs)

        result = await handler.search({
            "pattern": "def foo",
            "path": "/workspace/repo",
        })

        assert result["success"] is True
        assert "test.py:10:match" in result["output"]

    @pytest.mark.asyncio
    async def test_search_with_query_arg(self, handler, mock_runtime):
        """Test search accepts 'query' (legacy naming)."""
        mock_obs = MagicMock()
        mock_obs.output = "test.py:10:match"
        mock_obs.exit_code = 0
        mock_runtime.run = AsyncMock(return_value=mock_obs)

        result = await handler.search({
            "query": "def foo",
            "path": "/workspace/repo",
        })

        assert result["success"] is True
        assert "test.py:10:match" in result["output"]

    @pytest.mark.asyncio
    async def test_search_requires_pattern_or_query(self, handler):
        """Test search fails when no pattern/query provided."""
        result = await handler.search({
            "path": "/workspace/repo",
        })

        assert result["success"] is False
        assert "required" in result["error"]

    # --- list_files tests ---

    @pytest.mark.asyncio
    async def test_list_files_with_path_arg(self, handler, mock_runtime):
        """Test list_files accepts 'path' argument."""
        mock_obs = MagicMock()
        mock_obs.output = "file1.py\nfile2.py"
        mock_obs.exit_code = 0
        mock_runtime.run = AsyncMock(return_value=mock_obs)

        result = await handler.list_files({
            "path": "/workspace/repo/src",
        })

        assert result["success"] is True
        assert "file1.py" in result["output"]

    @pytest.mark.asyncio
    async def test_list_files_with_directory_arg(self, handler, mock_runtime):
        """Test list_files accepts 'directory' argument."""
        mock_obs = MagicMock()
        mock_obs.output = "file1.py\nfile2.py"
        mock_obs.exit_code = 0
        mock_runtime.run = AsyncMock(return_value=mock_obs)

        result = await handler.list_files({
            "directory": "/workspace/repo/src",
        })

        assert result["success"] is True
        assert "file1.py" in result["output"]

    @pytest.mark.asyncio
    async def test_list_files_with_dir_path_arg(self, handler, mock_runtime):
        """Test list_files accepts 'dir_path' argument."""
        mock_obs = MagicMock()
        mock_obs.output = "file1.py\nfile2.py"
        mock_obs.exit_code = 0
        mock_runtime.run = AsyncMock(return_value=mock_obs)

        result = await handler.list_files({
            "dir_path": "/workspace/repo/src",
        })

        assert result["success"] is True
        assert "file1.py" in result["output"]

    @pytest.mark.asyncio
    async def test_list_files_prefers_directory_over_path(self, handler, mock_runtime):
        """Test that 'directory' takes precedence over 'path'."""
        mock_obs = MagicMock()
        mock_obs.output = "preferred_dir"
        mock_obs.exit_code = 0
        mock_runtime.run = AsyncMock(return_value=mock_obs)

        result = await handler.list_files({
            "directory": "/workspace/repo/preferred",
            "path": "/workspace/repo/fallback",
        })

        assert result["success"] is True
        # Verify the command was called with the preferred path
        call_args = mock_runtime.run.call_args[0][0]
        assert "/workspace/repo/preferred" in call_args

    # --- run_command tests ---

    @pytest.mark.asyncio
    async def test_run_command_basic(self, handler, mock_runtime):
        """Test run_command executes commands."""
        from swecli.core.docker.models import BashObservation

        mock_obs = MagicMock()
        mock_obs.output = "hello world"
        mock_obs.exit_code = 0
        mock_obs.failure_reason = None
        mock_runtime.run_in_session = AsyncMock(return_value=mock_obs)

        result = await handler.run_command({
            "command": "echo hello world",
        })

        assert result["success"] is True
        assert result["output"] == "hello world"

    @pytest.mark.asyncio
    async def test_run_command_requires_command(self, handler):
        """Test run_command fails when no command provided."""
        result = await handler.run_command({})

        assert result["success"] is False
        assert "required" in result["error"]
