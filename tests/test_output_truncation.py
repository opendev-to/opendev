"""Tests for output truncation across file reads, search, bash, and list_files."""

from pathlib import Path
from unittest.mock import MagicMock


from swecli.core.context_engineering.tools.implementations.bash_tool import truncate_output
from swecli.core.context_engineering.tools.implementations.file_ops import FileOperations
from swecli.models.config import AppConfig


def _make_config() -> AppConfig:
    """Create a permissive AppConfig for tests."""
    config = AppConfig()
    config.permissions.file_read.enabled = True
    config.permissions.file_read.always_allow = True
    return config


class TestReadFileTruncation:
    """Tests for read_file output truncation."""

    def test_default_2000_line_limit(self, tmp_path: Path) -> None:
        """File with 5000 lines should return only first 2000."""
        p = tmp_path / "big.txt"
        p.write_text("\n".join(f"line {i}" for i in range(1, 5001)))

        ops = FileOperations(_make_config(), tmp_path)
        result = ops.read_file(str(p))

        # Should truncate and mention total line count
        assert "truncated" in result
        assert "of 5000" in result

    def test_offset_parameter(self, tmp_path: Path) -> None:
        """offset=100 should start reading from line 100."""
        p = tmp_path / "lines.txt"
        p.write_text("\n".join(f"content-{i}" for i in range(1, 301)))

        ops = FileOperations(_make_config(), tmp_path)
        result = ops.read_file(str(p), offset=100)

        first_line = result.split("\n")[0]
        assert "100\t" in first_line
        assert "content-100" in first_line

    def test_cat_n_line_number_format(self, tmp_path: Path) -> None:
        """Output should have 'N\\t<content>' format."""
        p = tmp_path / "fmt.txt"
        p.write_text("hello\nworld\n")

        ops = FileOperations(_make_config(), tmp_path)
        result = ops.read_file(str(p))

        lines = result.strip().split("\n")
        assert "\t" in lines[0]
        assert "1\t" in lines[0]
        assert "hello" in lines[0]

    def test_long_line_truncation(self, tmp_path: Path) -> None:
        """Lines >2000 chars should be truncated."""
        p = tmp_path / "long.txt"
        p.write_text("x" * 5000 + "\n")

        ops = FileOperations(_make_config(), tmp_path)
        result = ops.read_file(str(p))

        # The line should be truncated with indicator
        assert "line truncated" in result

    def test_truncation_message(self, tmp_path: Path) -> None:
        """Should append truncation indicator when limit hit."""
        p = tmp_path / "trunc.txt"
        p.write_text("\n".join(f"line {i}" for i in range(1, 3001)))

        ops = FileOperations(_make_config(), tmp_path)
        result = ops.read_file(str(p))

        assert "truncated" in result
        assert "2000" in result  # showing lines up to 2000

    def test_binary_detection(self, tmp_path: Path) -> None:
        """Should detect binary files and return error."""
        p = tmp_path / "binary.bin"
        p.write_bytes(b"\x00\x01\x02\x03binary data")

        ops = FileOperations(_make_config(), tmp_path)
        result = ops.read_file(str(p))

        assert "binary" in result.lower()

    def test_explicit_max_lines_override(self, tmp_path: Path) -> None:
        """Caller can set max_lines higher or lower."""
        p = tmp_path / "small.txt"
        p.write_text("\n".join(f"line {i}" for i in range(1, 51)))

        ops = FileOperations(_make_config(), tmp_path)
        result = ops.read_file(str(p), max_lines=10)

        content_lines = [ln for ln in result.split("\n") if ln.strip() and "truncated" not in ln]
        assert len(content_lines) == 10


class TestSearchOutputTruncation:
    """Tests for search output character cap."""

    def test_output_cap_at_30k_chars(self) -> None:
        """Total output should not exceed ~30,000 chars."""
        from swecli.core.context_engineering.tools.handlers.file_handlers import FileToolHandler

        file_ops = MagicMock()
        # Generate many long matches
        file_ops.grep_files.return_value = [
            {"file": f"/path/to/file_{i}.py", "line": i, "content": "x" * 800} for i in range(100)
        ]

        handler = FileToolHandler(file_ops, None, None)
        result = handler.search({"pattern": "test", "path": "."})

        assert result["success"]
        # Output should be truncated below a reasonable limit
        assert len(result["output"]) < 35_000

    def test_truncation_message_appended(self) -> None:
        """Should indicate when results are truncated."""
        from swecli.core.context_engineering.tools.handlers.file_handlers import FileToolHandler

        file_ops = MagicMock()
        file_ops.grep_files.return_value = [
            {"file": f"/path/file_{i}.py", "line": i, "content": "y" * 1000} for i in range(50)
        ]

        handler = FileToolHandler(file_ops, None, None)
        result = handler.search({"pattern": "test", "path": "."})

        assert result["success"]
        assert "truncated" in result["output"]


class TestBashOutputTruncation:
    """Tests for bash output middle-truncation."""

    def test_middle_truncation(self) -> None:
        """Large output should keep first 10K + last 10K."""
        big = "A" * 50_000
        result = truncate_output(big)

        assert len(result) < 50_000
        assert "truncated" in result
        assert result.startswith("A" * 100)
        assert result.endswith("A" * 100)

    def test_short_output_not_truncated(self) -> None:
        """Output under limit should pass through unchanged."""
        small = "hello world"
        assert truncate_output(small) == small


class TestListFilesTruncation:
    """Tests for list_files entry cap."""

    def test_500_entry_cap(self) -> None:
        """Should cap at 500 entries with count message."""
        from swecli.core.context_engineering.tools.handlers.file_handlers import FileToolHandler

        file_ops = MagicMock()
        file_ops.working_dir = Path("/mock")
        file_ops.glob_files.return_value = [f"file_{i}.py" for i in range(800)]

        handler = FileToolHandler(file_ops, None, None)
        result = handler.list_files({"path": ".", "pattern": "*.py"})

        assert result["success"]
        assert "500 of 800" in result["output"]
        assert len(result["entries"]) == 500
