"""Tests for ToolDisplayService - unified tool display formatting."""

from pathlib import Path

import pytest

from swecli.ui_textual.services import ToolDisplayService, ToolResultData, BashOutputData


class TestToolDisplayService:
    """Tests for ToolDisplayService."""

    @pytest.fixture
    def service(self, tmp_path: Path) -> ToolDisplayService:
        """Create a ToolDisplayService with a temp working directory."""
        return ToolDisplayService(tmp_path)

    def test_init_with_working_dir(self, tmp_path: Path) -> None:
        """Test initialization with explicit working directory."""
        service = ToolDisplayService(tmp_path)
        assert service._working_dir == tmp_path

    def test_init_default_working_dir(self) -> None:
        """Test initialization defaults to current working directory."""
        service = ToolDisplayService()
        assert service._working_dir == Path.cwd()


class TestResolvePathsUnified:
    """Tests for unified path resolution."""

    @pytest.fixture
    def service(self, tmp_path: Path) -> ToolDisplayService:
        return ToolDisplayService(tmp_path)

    def test_resolve_relative_file_path(self, service: ToolDisplayService, tmp_path: Path) -> None:
        """Test that relative file_path is resolved to absolute."""
        args = {"file_path": "src/main.py"}
        result = service.resolve_paths(args)
        assert result["file_path"] == str(tmp_path / "src/main.py")

    def test_resolve_dot_path(self, service: ToolDisplayService, tmp_path: Path) -> None:
        """Test that '.' is resolved to working directory."""
        args = {"path": "."}
        result = service.resolve_paths(args)
        assert result["path"] == str(tmp_path)

    def test_resolve_empty_path(self, service: ToolDisplayService, tmp_path: Path) -> None:
        """Test that empty string is resolved to working directory."""
        args = {"directory": ""}
        result = service.resolve_paths(args)
        assert result["directory"] == str(tmp_path)

    def test_preserve_absolute_path(self, service: ToolDisplayService) -> None:
        """Test that absolute paths are not modified."""
        args = {"file_path": "/absolute/path/file.txt"}
        result = service.resolve_paths(args)
        assert result["file_path"] == "/absolute/path/file.txt"

    def test_preserve_docker_prefix(self, service: ToolDisplayService) -> None:
        """Test that paths with docker prefix are not modified."""
        args = {"path": "[container]:/app/file.txt"}
        result = service.resolve_paths(args)
        assert result["path"] == "[container]:/app/file.txt"

    def test_strip_leading_dot_slash(self, service: ToolDisplayService, tmp_path: Path) -> None:
        """Test that leading ./ is stripped before resolution."""
        args = {"file_path": "./src/main.py"}
        result = service.resolve_paths(args)
        assert result["file_path"] == str(tmp_path / "src/main.py")

    def test_non_path_keys_unchanged(self, service: ToolDisplayService) -> None:
        """Test that non-path keys are not modified."""
        args = {"command": "ls -la", "timeout": 30}
        result = service.resolve_paths(args)
        assert result["command"] == "ls -la"
        assert result["timeout"] == 30


class TestNormalizeArguments:
    """Tests for argument normalization."""

    @pytest.fixture
    def service(self) -> ToolDisplayService:
        return ToolDisplayService()

    def test_dict_passthrough(self, service: ToolDisplayService) -> None:
        """Test that dict arguments pass through."""
        args = {"key": "value"}
        result = service.normalize_arguments(args)
        assert result == {"key": "value"}

    def test_json_string_parsing(self, service: ToolDisplayService) -> None:
        """Test that JSON string is parsed to dict."""
        args = '{"command": "ls"}'
        result = service.normalize_arguments(args)
        assert result == {"command": "ls"}

    def test_plain_string_wrapped(self, service: ToolDisplayService) -> None:
        """Test that plain string is wrapped in value key."""
        args = "plain text"
        result = service.normalize_arguments(args)
        assert result == {"value": "plain text"}

    def test_url_fix_missing_double_slash(self, service: ToolDisplayService) -> None:
        """Test that https:/domain is fixed to https://domain."""
        args = {"url": "https:/example.com"}
        result = service.normalize_arguments(args)
        assert result["url"] == "https://example.com"

    def test_url_add_protocol(self, service: ToolDisplayService) -> None:
        """Test that missing protocol is added."""
        args = {"url": "example.com/path"}
        result = service.normalize_arguments(args)
        assert result["url"] == "https://example.com/path"

    def test_url_preserve_valid(self, service: ToolDisplayService) -> None:
        """Test that valid URL is preserved."""
        args = {"url": "https://example.com"}
        result = service.normalize_arguments(args)
        assert result["url"] == "https://example.com"


class TestTruncateOutput:
    """Tests for unified truncation."""

    @pytest.fixture
    def service(self) -> ToolDisplayService:
        return ToolDisplayService()

    def test_bash_truncation_head_tail(self, service: ToolDisplayService) -> None:
        """Test bash mode uses head/tail truncation."""
        lines = "\n".join([f"line {i}" for i in range(20)])
        truncated, is_truncated, hidden = service.truncate_output(lines, mode="bash")

        assert is_truncated
        assert hidden == 10  # 20 - 5 - 5
        assert "line 0" in truncated
        assert "line 4" in truncated
        assert "line 19" in truncated
        assert "... (10 lines hidden) ..." in truncated

    def test_bash_no_truncation_small(self, service: ToolDisplayService) -> None:
        """Test bash mode doesn't truncate small output."""
        lines = "\n".join([f"line {i}" for i in range(5)])
        truncated, is_truncated, hidden = service.truncate_output(lines, mode="bash")

        assert not is_truncated
        assert hidden == 0
        assert truncated == lines

    def test_nested_truncation(self, service: ToolDisplayService) -> None:
        """Test nested mode uses smaller head/tail."""
        lines = "\n".join([f"line {i}" for i in range(10)])
        truncated, is_truncated, hidden = service.truncate_output(lines, mode="nested")

        assert is_truncated
        assert hidden == 4  # 10 - 3 - 3
        assert "... (4 lines hidden) ..." in truncated

    def test_generic_max_lines(self, service: ToolDisplayService) -> None:
        """Test generic mode truncates by max lines."""
        lines = "\n".join([f"line {i}" for i in range(10)])
        truncated, is_truncated, hidden = service.truncate_output(lines, mode="generic")

        assert is_truncated
        assert "... (truncated)" in truncated

    def test_generic_max_chars(self, service: ToolDisplayService) -> None:
        """Test generic mode truncates by max chars."""
        text = "x" * 500
        truncated, is_truncated, _ = service.truncate_output(text, mode="generic")

        assert is_truncated
        assert len(truncated) < 500
        assert "... (truncated)" in truncated

    def test_empty_input(self, service: ToolDisplayService) -> None:
        """Test empty input returns empty."""
        truncated, is_truncated, hidden = service.truncate_output("", mode="bash")
        assert truncated == ""
        assert not is_truncated
        assert hidden == 0


class TestFormatToolResult:
    """Tests for format_tool_result."""

    @pytest.fixture
    def service(self) -> ToolDisplayService:
        return ToolDisplayService()

    def test_interrupted_result(self, service: ToolDisplayService) -> None:
        """Test interrupted result is formatted correctly."""
        result = service.format_tool_result(
            "read_file", {"file_path": "test.py"}, {"interrupted": True}
        )

        assert isinstance(result, ToolResultData)
        assert result.is_interrupted
        assert not result.success
        # Interrupted results now have empty lines - message is shown by ui_callback.on_interrupt()
        assert result.lines == []

    def test_rejected_result(self, service: ToolDisplayService) -> None:
        """Test rejected result is formatted correctly."""
        result = service.format_tool_result(
            "write_file", {"file_path": "test.py"}, {"_approved": False}
        )

        assert isinstance(result, ToolResultData)
        assert result.is_rejected
        assert not result.success
        assert "Operation rejected by user" in result.lines

    def test_bash_result_with_output(self, service: ToolDisplayService) -> None:
        """Test bash result with output."""
        result = service.format_tool_result(
            "Bash",
            {"command": "ls"},
            {"success": True, "stdout": "file1\nfile2"},
        )

        assert isinstance(result, ToolResultData)
        assert result.special_type == "bash"
        assert result.bash_data is not None
        assert result.bash_data.command == "ls"
        assert not result.bash_data.is_error
        assert "OK: ls ran successfully" in result.bash_data.output

    def test_bash_error_result(self, service: ToolDisplayService) -> None:
        """Test bash error result."""
        result = service.format_tool_result(
            "bash_execute",
            {"command": "invalid_cmd"},
            {"success": False, "stderr": "command not found"},
        )

        assert isinstance(result, ToolResultData)
        assert result.bash_data is not None
        assert result.bash_data.is_error
        assert "command not found" in result.bash_data.output

    def test_bash_background_task(self, service: ToolDisplayService) -> None:
        """Test bash background task result."""
        result = service.format_tool_result(
            "Bash",
            {"command": "sleep 100"},
            {"success": True, "background_task_id": "bg_123"},
        )

        assert result.special_type == "bash"
        assert "Running in background" in result.lines[0]

    def test_string_result_conversion(self, service: ToolDisplayService) -> None:
        """Test that string result is converted to dict."""
        result = service.format_tool_result(
            "some_tool", {}, "simple output"
        )

        assert isinstance(result, ToolResultData)
        assert result.success


class TestFormatToolHeader:
    """Tests for format_tool_header."""

    @pytest.fixture
    def service(self, tmp_path: Path) -> ToolDisplayService:
        return ToolDisplayService(tmp_path)

    def test_header_includes_tool_name(self, service: ToolDisplayService) -> None:
        """Test header includes tool name."""
        from rich.text import Text

        header = service.format_tool_header("read_file", {"file_path": "test.py"})
        assert isinstance(header, Text)
        # The header should contain read_file
        plain = header.plain
        assert "read_file" in plain or "Read" in plain

    def test_header_resolves_paths(self, service: ToolDisplayService, tmp_path: Path) -> None:
        """Test header has resolved paths."""
        from rich.text import Text

        header = service.format_tool_header("read_file", {"file_path": "src/main.py"})
        plain = header.plain
        # Should contain the full resolved path
        assert str(tmp_path) in plain


class TestExtractResultLines:
    """Tests for extract_result_lines."""

    @pytest.fixture
    def service(self) -> ToolDisplayService:
        return ToolDisplayService()

    def test_extract_single_line(self, service: ToolDisplayService) -> None:
        """Test extracting single result line."""
        formatted = "⎿ Read 100 lines from file.py"
        lines = service.extract_result_lines(formatted)
        assert lines == ["Read 100 lines from file.py"]

    def test_extract_multiple_lines(self, service: ToolDisplayService) -> None:
        """Test extracting multiple result lines."""
        formatted = "⎿ Updated file.py\n  +5 lines\n  -2 lines"
        lines = service.extract_result_lines(formatted)
        assert len(lines) == 3
        assert lines[0] == "Updated file.py"

    def test_extract_empty_input(self, service: ToolDisplayService) -> None:
        """Test extracting from empty input."""
        lines = service.extract_result_lines("")
        assert lines == []

    def test_extract_non_string_input(self, service: ToolDisplayService) -> None:
        """Test extracting from non-string input."""
        lines = service.extract_result_lines(None)  # type: ignore
        assert lines == []


class TestTruncationConsistency:
    """Tests to verify truncation is consistent between live and replay."""

    def test_same_truncation_both_modes(self) -> None:
        """Test that the same output truncates identically."""
        service = ToolDisplayService()

        # Create long output
        long_output = "\n".join([f"line {i}: some content here" for i in range(100)])

        # Truncate with bash mode (used by both paths)
        truncated, is_truncated, hidden = service.truncate_output(long_output, mode="bash")

        # Verify consistent truncation
        assert is_truncated
        assert hidden == 90  # 100 - 5 - 5
        assert "line 0" in truncated
        assert "line 99" in truncated
        assert "... (90 lines hidden) ..." in truncated

    def test_bash_result_consistency(self) -> None:
        """Test that bash results format consistently."""
        service = ToolDisplayService()

        result = service.format_tool_result(
            "Bash",
            {"command": "echo test"},
            {"success": True, "stdout": "test output"},
        )

        # Verify structure is consistent
        assert result.special_type == "bash"
        assert result.bash_data is not None
        assert isinstance(result.bash_data, BashOutputData)
        assert result.bash_data.command == "echo test"
