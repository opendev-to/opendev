"""Tests for FileContentInjector - @ mention file content injection."""

from __future__ import annotations

import tempfile
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from swecli.repl.file_content_injector import FileContentInjector, InjectionResult


class MockFileOps:
    """Mock file operations for testing."""

    def __init__(self, working_dir: Path):
        self.working_dir = working_dir

    def read_file(self, path: str) -> str:
        """Read file content."""
        file_path = Path(path)
        if not file_path.is_absolute():
            file_path = self.working_dir / file_path
        return file_path.read_text()


@pytest.fixture
def temp_workspace():
    """Create a temporary workspace with test files."""
    with tempfile.TemporaryDirectory() as tmpdir:
        workspace = Path(tmpdir)

        # Create test files
        (workspace / "main.py").write_text("def main():\n    print('hello')\n")
        (workspace / "README.md").write_text("# Project\n\nThis is a test project.\n")
        (workspace / "config.yaml").write_text("key: value\ncount: 42\n")

        # Create subdirectory
        (workspace / "src").mkdir()
        (workspace / "src" / "utils.py").write_text("def helper():\n    pass\n")
        (workspace / "src" / "data.json").write_text('{"name": "test"}\n')

        # Create a large file
        large_content = "\n".join([f"line {i}" for i in range(1500)])
        (workspace / "large.log").write_text(large_content)

        # Create an empty file
        (workspace / "empty.txt").write_text("")

        # Create binary file (mock)
        (workspace / "binary.bin").write_bytes(b"\x00\x01\x02\x03")

        yield workspace


@pytest.fixture
def injector(temp_workspace):
    """Create FileContentInjector with mock config."""
    file_ops = MockFileOps(temp_workspace)
    config = MagicMock()
    config.model_vlm = None  # No vision model configured
    config.model_vlm_provider = None
    return FileContentInjector(file_ops, config, temp_workspace)


class TestInjectionResult:
    """Tests for InjectionResult dataclass."""

    def test_create_empty_result(self):
        """Test creating an empty result."""
        result = InjectionResult(text_content="")
        assert result.text_content == ""
        assert result.image_blocks == []
        assert result.errors == []

    def test_create_with_content(self):
        """Test creating result with content."""
        result = InjectionResult(
            text_content="<file_content>...</file_content>",
            image_blocks=[{"type": "image"}],
            errors=["Error 1"],
        )
        assert "<file_content>" in result.text_content
        assert len(result.image_blocks) == 1
        assert len(result.errors) == 1


class TestExtractRefs:
    """Tests for file reference extraction."""

    def test_extract_simple_ref(self, injector):
        """Test extracting a simple file reference."""
        refs = injector._extract_refs("analyze @main.py")
        assert len(refs) == 1
        assert refs[0][0] == "main.py"

    def test_extract_multiple_refs(self, injector):
        """Test extracting multiple file references."""
        refs = injector._extract_refs("compare @main.py and @README.md")
        assert len(refs) == 2
        assert refs[0][0] == "main.py"
        assert refs[1][0] == "README.md"

    def test_extract_path_with_directory(self, injector):
        """Test extracting path with directory."""
        refs = injector._extract_refs("check @src/utils.py")
        assert len(refs) == 1
        assert refs[0][0] == "src/utils.py"

    def test_extract_quoted_path(self, injector):
        """Test extracting quoted path with spaces."""
        refs = injector._extract_refs('read @"path with spaces/file.py"')
        assert len(refs) == 1
        assert refs[0][0] == "path with spaces/file.py"

    def test_exclude_email(self, injector):
        """Test that emails are not treated as file references."""
        refs = injector._extract_refs("contact user@example.com for help")
        # Should not extract example.com as a file reference
        assert len(refs) == 0

    def test_deduplicate_refs(self, injector):
        """Test that duplicate references are deduplicated."""
        refs = injector._extract_refs("@main.py and @main.py again")
        assert len(refs) == 1

    def test_no_refs(self, injector):
        """Test query with no file references."""
        refs = injector._extract_refs("just a normal query")
        assert len(refs) == 0


class TestTextFileProcessing:
    """Tests for text file processing."""

    def test_inject_python_file(self, injector, temp_workspace):
        """Test injecting a Python file."""
        result = injector.inject_content("read @main.py")
        assert '<file_content path="main.py"' in result.text_content
        assert 'language="python"' in result.text_content
        assert "def main():" in result.text_content
        assert "</file_content>" in result.text_content

    def test_inject_markdown_file(self, injector, temp_workspace):
        """Test injecting a Markdown file."""
        result = injector.inject_content("read @README.md")
        assert '<file_content path="README.md"' in result.text_content
        assert 'language="markdown"' in result.text_content
        assert "# Project" in result.text_content

    def test_inject_yaml_file(self, injector, temp_workspace):
        """Test injecting a YAML file."""
        result = injector.inject_content("read @config.yaml")
        assert '<file_content path="config.yaml"' in result.text_content
        assert 'language="yaml"' in result.text_content
        assert "key: value" in result.text_content

    def test_inject_nested_file(self, injector, temp_workspace):
        """Test injecting a file in subdirectory."""
        result = injector.inject_content("read @src/utils.py")
        assert '<file_content path="src/utils.py"' in result.text_content
        assert 'language="python"' in result.text_content
        assert "def helper():" in result.text_content

    def test_inject_empty_file(self, injector, temp_workspace):
        """Test injecting an empty file."""
        result = injector.inject_content("read @empty.txt")
        assert '<file_content path="empty.txt"' in result.text_content
        assert result.errors == []


class TestLargeFileProcessing:
    """Tests for large file truncation."""

    def test_truncate_large_file(self, injector, temp_workspace):
        """Test that large files are truncated with head/tail."""
        result = injector.inject_content("read @large.log")
        assert '<file_truncated' in result.text_content
        assert 'large.log' in result.text_content
        assert "=== HEAD (lines 1-100) ===" in result.text_content
        assert "=== TRUNCATED" in result.text_content
        assert "=== TAIL" in result.text_content
        assert "</file_truncated>" in result.text_content


class TestDirectoryProcessing:
    """Tests for directory listing."""

    def test_list_directory(self, injector, temp_workspace):
        """Test listing a directory."""
        result = injector.inject_content("list @src/")
        assert '<directory_listing path="src/"' in result.text_content
        assert "utils.py" in result.text_content
        assert "data.json" in result.text_content
        assert "</directory_listing>" in result.text_content

    def test_list_root_directory(self, injector, temp_workspace):
        """Test listing the root workspace directory."""
        # Create a simple subdirectory to list
        (temp_workspace / "subdir").mkdir()
        (temp_workspace / "subdir" / "file.txt").write_text("content")

        result = injector.inject_content("list @subdir/")
        assert "<directory_listing" in result.text_content


class TestErrorHandling:
    """Tests for error handling."""

    def test_nonexistent_file(self, injector):
        """Test handling of non-existent file."""
        result = injector.inject_content("read @nonexistent.py")
        assert '<file_error path="nonexistent.py" reason="File not found"' in result.text_content
        assert "File not found" in result.errors[0]

    def test_binary_file_skipped(self, injector, temp_workspace):
        """Test that known binary files are skipped."""
        result = injector.inject_content("read @binary.bin")
        # Binary files with .bin extension should be treated as unsupported
        assert "Unsupported file type" in result.text_content or "file_error" in result.text_content


class TestMultipleFiles:
    """Tests for multiple file injection."""

    def test_inject_multiple_files(self, injector, temp_workspace):
        """Test injecting multiple files in one query."""
        result = injector.inject_content("compare @main.py and @README.md")
        assert '<file_content path="main.py"' in result.text_content
        assert '<file_content path="README.md"' in result.text_content
        assert "def main():" in result.text_content
        assert "# Project" in result.text_content


class TestBinaryDetection:
    """Tests for binary file detection."""

    def test_detect_binary_file(self, injector, temp_workspace):
        """Test that binary content is detected."""
        # The binary.bin file has null bytes
        assert not injector._detect_text_file(temp_workspace / "binary.bin")

    def test_detect_text_file(self, injector, temp_workspace):
        """Test that text content is detected."""
        assert injector._detect_text_file(temp_workspace / "main.py")

    def test_unknown_extension_text(self, injector, temp_workspace):
        """Test that unknown extension with text content is detected as text."""
        (temp_workspace / "custom.xyz").write_text("This is plain text content.\n")
        assert injector._detect_text_file(temp_workspace / "custom.xyz")

    def test_unknown_extension_binary(self, injector, temp_workspace):
        """Test that unknown extension with binary content is detected as binary."""
        (temp_workspace / "custom.data").write_bytes(b"\x00\x01\x02\x03\x04")
        assert not injector._detect_text_file(temp_workspace / "custom.data")


class TestImageProcessing:
    """Tests for image file processing."""

    def test_image_without_vision_model(self, injector, temp_workspace):
        """Test image processing when vision model is not configured."""
        # Create a fake image file
        (temp_workspace / "test.png").write_bytes(b"fake png data")

        result = injector.inject_content("analyze @test.png")
        assert "Vision model not configured" in result.text_content
        assert result.image_blocks == []

    def test_image_with_vision_model(self, temp_workspace):
        """Test image processing when vision model is configured."""
        # Create a fake image file
        (temp_workspace / "test.png").write_bytes(b"fake png data")

        file_ops = MockFileOps(temp_workspace)
        config = MagicMock()
        config.model_vlm = "gpt-4-vision"
        config.model_vlm_provider = "openai"

        # Mock VLMTool at the source module
        with patch(
            "swecli.core.context_engineering.tools.implementations.vlm_tool.VLMTool"
        ) as mock_vlm_class:
            mock_vlm = MagicMock()
            mock_vlm.is_available.return_value = True
            mock_vlm.encode_image.return_value = "base64encodeddata"
            mock_vlm_class.return_value = mock_vlm

            injector = FileContentInjector(file_ops, config, temp_workspace)
            result = injector.inject_content("analyze @test.png")

            assert '<image path="test.png" type="image/png">' in result.text_content
            assert len(result.image_blocks) == 1
            assert result.image_blocks[0]["type"] == "image"
            assert result.image_blocks[0]["source"]["data"] == "base64encodeddata"


class TestPDFProcessing:
    """Tests for PDF file processing."""

    def test_pdf_extraction(self, temp_workspace):
        """Test PDF text extraction."""
        # Create a fake PDF file
        (temp_workspace / "test.pdf").write_bytes(b"%PDF-1.4 fake content")

        file_ops = MockFileOps(temp_workspace)
        config = MagicMock()

        # Mock PDFTool at the source module
        with patch(
            "swecli.core.context_engineering.tools.implementations.pdf_tool.PDFTool"
        ) as mock_pdf_class:
            mock_pdf = MagicMock()
            mock_pdf.extract_text.return_value = {
                "success": True,
                "content": "Extracted PDF text content",
                "page_count": 5,
            }
            mock_pdf_class.return_value = mock_pdf

            injector = FileContentInjector(file_ops, config, temp_workspace)
            result = injector.inject_content("read @test.pdf")

            assert '<pdf_content path="test.pdf" pages="5">' in result.text_content
            assert "Extracted PDF text content" in result.text_content

    def test_pdf_extraction_failure(self, temp_workspace):
        """Test PDF extraction when it fails."""
        (temp_workspace / "corrupt.pdf").write_bytes(b"not a real pdf")

        file_ops = MockFileOps(temp_workspace)
        config = MagicMock()

        with patch(
            "swecli.core.context_engineering.tools.implementations.pdf_tool.PDFTool"
        ) as mock_pdf_class:
            mock_pdf = MagicMock()
            mock_pdf.extract_text.return_value = {
                "success": False,
                "error": "Invalid PDF format",
            }
            mock_pdf_class.return_value = mock_pdf

            injector = FileContentInjector(file_ops, config, temp_workspace)
            result = injector.inject_content("read @corrupt.pdf")

            assert "Invalid PDF format" in result.text_content


class TestEdgeCases:
    """Tests for edge cases."""

    def test_at_in_middle_of_word(self, injector):
        """Test that @ in middle of word is ignored."""
        refs = injector._extract_refs("the email@domain pattern")
        # Should not extract anything
        assert len(refs) == 0

    def test_multiple_at_symbols(self, injector):
        """Test multiple @ in query."""
        refs = injector._extract_refs("@file1.py @file2.py @file3.py")
        assert len(refs) == 3

    def test_special_characters_in_path(self, injector, temp_workspace):
        """Test file with special characters in path."""
        # Create file with hyphens and underscores
        (temp_workspace / "my-file_v2.py").write_text("content")

        result = injector.inject_content("read @my-file_v2.py")
        assert '<file_content path="my-file_v2.py"' in result.text_content

    def test_empty_query(self, injector):
        """Test empty query."""
        result = injector.inject_content("")
        assert result.text_content == ""
        assert result.image_blocks == []
        assert result.errors == []

    def test_query_without_refs(self, injector):
        """Test query with no @ references."""
        result = injector.inject_content("just a normal question")
        assert result.text_content == ""
        assert result.image_blocks == []


class TestLanguageDetection:
    """Tests for language detection."""

    def test_python_language(self, injector):
        """Test Python language detection."""
        assert injector._get_language(Path("test.py")) == "python"

    def test_javascript_language(self, injector):
        """Test JavaScript language detection."""
        assert injector._get_language(Path("test.js")) == "javascript"

    def test_typescript_language(self, injector):
        """Test TypeScript language detection."""
        assert injector._get_language(Path("test.ts")) == "typescript"

    def test_unknown_language(self, injector):
        """Test unknown extension."""
        assert injector._get_language(Path("test.xyz")) == ""


class TestSizeFormatting:
    """Tests for file size formatting."""

    def test_format_bytes(self, injector):
        """Test byte formatting."""
        assert injector._format_size(500) == "500B"

    def test_format_kilobytes(self, injector):
        """Test kilobyte formatting."""
        assert injector._format_size(2048) == "2.0KB"

    def test_format_megabytes(self, injector):
        """Test megabyte formatting."""
        assert injector._format_size(2 * 1024 * 1024) == "2.0MB"
