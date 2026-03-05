"""Integration tests for QueryEnhancer multimodal support."""

from __future__ import annotations

import tempfile
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from swecli.repl.query_enhancer import QueryEnhancer


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


class MockSession:
    """Mock session for testing."""

    def __init__(self):
        self.session_id = "test-session"

    def to_api_messages(self, window_size=None):
        return [{"role": "user", "content": "previous message"}]

    def get_playbook(self):
        return MagicMock(as_context=MagicMock(return_value=None))


class MockSessionManager:
    """Mock session manager for testing."""

    def __init__(self, session=None):
        self.current_session = session or MockSession()


class MockAgent:
    """Mock agent for testing."""

    def __init__(self):
        self.system_prompt = "You are a helpful assistant."


@pytest.fixture
def temp_workspace():
    """Create a temporary workspace with test files."""
    with tempfile.TemporaryDirectory() as tmpdir:
        workspace = Path(tmpdir)

        # Create test files
        (workspace / "main.py").write_text("def main():\n    pass\n")
        (workspace / "README.md").write_text("# Test\n")
        (workspace / "test.png").write_bytes(b"fake png data")

        yield workspace


@pytest.fixture
def query_enhancer(temp_workspace):
    """Create QueryEnhancer with mock dependencies."""
    file_ops = MockFileOps(temp_workspace)
    session_manager = MockSessionManager()
    config = MagicMock()
    config.model_vlm = None
    config.model_vlm_provider = None
    config.playbook = None
    console = MagicMock()

    return QueryEnhancer(file_ops, session_manager, config, console)


class TestEnhanceQueryMultimodal:
    """Tests for enhance_query with multimodal support."""

    def test_returns_tuple(self, query_enhancer):
        """Test that enhance_query returns a tuple."""
        result = query_enhancer.enhance_query("test query")
        assert isinstance(result, tuple)
        assert len(result) == 2

    def test_text_only_returns_empty_image_blocks(self, query_enhancer, temp_workspace):
        """Test that text-only queries return empty image_blocks."""
        enhanced, image_blocks = query_enhancer.enhance_query("read @main.py")
        assert "def main():" in enhanced
        assert image_blocks == []

    def test_strips_at_from_query(self, query_enhancer, temp_workspace):
        """Test that @ is stripped from the query."""
        enhanced, _ = query_enhancer.enhance_query("read @main.py")
        # The query should have main.py without @
        assert "read main.py" in enhanced

    def test_multiple_files(self, query_enhancer, temp_workspace):
        """Test enhancing query with multiple file references."""
        enhanced, _ = query_enhancer.enhance_query("compare @main.py and @README.md")
        assert '<file_content path="main.py"' in enhanced
        assert '<file_content path="README.md"' in enhanced

    def test_image_without_vision_model(self, query_enhancer, temp_workspace):
        """Test image handling when vision model is not configured."""
        enhanced, image_blocks = query_enhancer.enhance_query("analyze @test.png")
        assert "Vision model not configured" in enhanced
        assert image_blocks == []


class TestPrepareMessagesMultimodal:
    """Tests for prepare_messages with multimodal content."""

    def test_text_only_messages(self, query_enhancer, temp_workspace):
        """Test message preparation with text only."""
        enhanced, image_blocks = query_enhancer.enhance_query("read @main.py")
        agent = MockAgent()

        messages = query_enhancer.prepare_messages(
            "read @main.py", enhanced, agent, image_blocks
        )

        # Should have system and user messages
        assert len(messages) >= 2
        assert messages[0]["role"] == "system"
        # Last user message should be string (not multimodal)
        user_msg = next(m for m in reversed(messages) if m["role"] == "user")
        assert isinstance(user_msg["content"], str)

    def test_multimodal_messages(self, query_enhancer, temp_workspace):
        """Test message preparation with image blocks."""
        enhanced = "analyze this image"
        image_blocks = [
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "base64data",
                },
            }
        ]
        agent = MockAgent()

        messages = query_enhancer.prepare_messages(
            "analyze @test.png", enhanced, agent, image_blocks
        )

        # Find the user message
        user_msg = next(m for m in reversed(messages) if m["role"] == "user")

        # Should be a list (multimodal format)
        assert isinstance(user_msg["content"], list)
        assert len(user_msg["content"]) == 2

        # First block should be text
        assert user_msg["content"][0]["type"] == "text"
        assert user_msg["content"][0]["text"] == enhanced

        # Second block should be image
        assert user_msg["content"][1]["type"] == "image"
        assert user_msg["content"][1]["source"]["data"] == "base64data"

    def test_no_image_blocks_keeps_string_content(self, query_enhancer):
        """Test that None or empty image_blocks keeps string content."""
        agent = MockAgent()

        # Test with None
        messages = query_enhancer.prepare_messages(
            "query", "enhanced query", agent, None
        )
        user_msg = next(m for m in reversed(messages) if m["role"] == "user")
        assert isinstance(user_msg["content"], str)

        # Test with empty list
        messages = query_enhancer.prepare_messages(
            "query", "enhanced query", agent, []
        )
        user_msg = next(m for m in reversed(messages) if m["role"] == "user")
        assert isinstance(user_msg["content"], str)


class TestImageIntegration:
    """Integration tests for image handling with mocked vision model."""

    def test_full_image_flow(self, temp_workspace):
        """Test full flow: query → enhance → prepare → multimodal messages."""
        file_ops = MockFileOps(temp_workspace)
        session_manager = MockSessionManager()
        config = MagicMock()
        config.model_vlm = "gpt-4-vision"
        config.model_vlm_provider = "openai"
        config.playbook = None
        console = MagicMock()

        # Mock VLMTool to simulate vision model being available
        with patch(
            "swecli.core.context_engineering.tools.implementations.vlm_tool.VLMTool"
        ) as mock_vlm_class:
            mock_vlm = MagicMock()
            mock_vlm.is_available.return_value = True
            mock_vlm.encode_image.return_value = "base64imagedata"
            mock_vlm_class.return_value = mock_vlm

            enhancer = QueryEnhancer(file_ops, session_manager, config, console)
            agent = MockAgent()

            # Enhance query
            enhanced, image_blocks = enhancer.enhance_query("analyze @test.png")

            # Verify image block was created
            assert len(image_blocks) == 1
            assert image_blocks[0]["type"] == "image"

            # Prepare messages
            messages = enhancer.prepare_messages(
                "analyze @test.png", enhanced, agent, image_blocks
            )

            # Verify multimodal format
            user_msg = next(m for m in reversed(messages) if m["role"] == "user")
            assert isinstance(user_msg["content"], list)
            assert user_msg["content"][1]["source"]["data"] == "base64imagedata"


class TestEmailExclusion:
    """Tests for email address exclusion."""

    def test_email_not_treated_as_file(self, query_enhancer):
        """Test that emails are not treated as file references."""
        enhanced, image_blocks = query_enhancer.enhance_query(
            "contact user@example.com for help"
        )
        # Email should remain unchanged
        assert "user@example.com" in enhanced
        # No file tags should be present
        assert "<file_content" not in enhanced
        assert "<file_error" not in enhanced


class TestDirectoryListing:
    """Tests for directory listing."""

    def test_directory_injection(self, query_enhancer, temp_workspace):
        """Test directory listing injection."""
        # Create a subdirectory
        (temp_workspace / "src").mkdir()
        (temp_workspace / "src" / "app.py").write_text("# app")

        enhanced, _ = query_enhancer.enhance_query("list @src/")
        assert "<directory_listing" in enhanced
        assert "app.py" in enhanced


class TestLargeFileHandling:
    """Tests for large file truncation."""

    def test_large_file_truncation(self, query_enhancer, temp_workspace):
        """Test that large files are truncated."""
        # Create a large file
        large_content = "\n".join([f"line {i}" for i in range(2000)])
        (temp_workspace / "large.log").write_text(large_content)

        enhanced, _ = query_enhancer.enhance_query("read @large.log")
        assert "<file_truncated" in enhanced
        assert "HEAD" in enhanced
        assert "TAIL" in enhanced
        assert "TRUNCATED" in enhanced
