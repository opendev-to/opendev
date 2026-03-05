"""Tests for tool schema and system prompt quality."""

from pathlib import Path

import pytest

from swecli.core.agents.components.schemas.definitions import _BUILTIN_TOOL_SCHEMAS


def _get_schema_by_name(name: str) -> dict | None:
    """Find a tool schema by its function name."""
    for schema in _BUILTIN_TOOL_SCHEMAS:
        if schema.get("function", {}).get("name") == name:
            return schema
    return None


def _get_description(name: str) -> str:
    """Get the description string for a tool by name."""
    schema = _get_schema_by_name(name)
    assert schema is not None, f"Schema '{name}' not found"
    return schema["function"]["description"]


SYSTEM_PROMPT_PATH = (
    Path(__file__).parent.parent
    / "swecli"
    / "core"
    / "agents"
    / "prompts"
    / "templates"
    / "system"
    / "main_system_prompt.txt"
)


class TestToolSchemaQuality:
    """Tests for enriched tool schema descriptions."""

    @pytest.mark.parametrize(
        "tool_name",
        [
            "read_file",
            "write_file",
            "edit_file",
            "search",
            "run_command",
            "list_files",
            "fetch_url",
        ],
    )
    def test_schema_descriptions_exceed_min_length(self, tool_name: str) -> None:
        """No schema description should be shorter than 100 chars."""
        desc = _get_description(tool_name)
        assert len(desc) >= 100, f"{tool_name} description is only {len(desc)} chars"

    @pytest.mark.parametrize(
        "tool_name",
        [
            "read_file",
            "write_file",
            "edit_file",
            "search",
            "run_command",
            "list_files",
            "fetch_url",
        ],
    )
    def test_all_schemas_have_usage_notes(self, tool_name: str) -> None:
        """Every enriched tool schema description should contain usage guidance."""
        desc = _get_description(tool_name)
        assert (
            "usage" in desc.lower() or "important" in desc.lower()
        ), f"{tool_name} description lacks usage notes"

    def test_read_file_warns_about_edit(self) -> None:
        """read_file schema should mention reading before editing."""
        desc = _get_description("read_file")
        assert "edit" in desc.lower()

    def test_edit_file_warns_about_reading_first(self) -> None:
        """edit_file schema should mention reading the file first."""
        desc = _get_description("edit_file")
        assert "read" in desc.lower()

    def test_run_command_warns_about_alternatives(self) -> None:
        """run_command should mention preferring dedicated tools."""
        desc = _get_description("run_command")
        assert "prefer" in desc.lower() or "dedicated" in desc.lower()

    def test_fetch_url_warns_about_guessing(self) -> None:
        """fetch_url should warn against generating or guessing URLs."""
        desc = _get_description("fetch_url")
        assert "never" in desc.lower() or "guess" in desc.lower()


class TestSystemPromptQuality:
    """Tests for enriched system prompt content."""

    @pytest.fixture()
    def prompt(self) -> str:
        return SYSTEM_PROMPT_PATH.read_text()

    def test_prompt_contains_git_safety(self, prompt: str) -> None:
        """System prompt should include git safety protocol."""
        assert "Git Safety Protocol" in prompt

    def test_prompt_contains_read_before_edit(self, prompt: str) -> None:
        """System prompt should include read-before-edit pattern."""
        assert "Read-Before-Edit" in prompt

    def test_prompt_contains_tool_guidance(self, prompt: str) -> None:
        """System prompt should include tool selection guidance."""
        assert "Tool Selection Guide" in prompt

    def test_prompt_contains_output_awareness(self, prompt: str) -> None:
        """System prompt should include output awareness section."""
        assert "Output Awareness" in prompt

    def test_prompt_contains_error_recovery(self, prompt: str) -> None:
        """System prompt should include error recovery guidance."""
        assert "Error Recovery" in prompt

    def test_prompt_mentions_truncation_limits(self, prompt: str) -> None:
        """System prompt should mention specific truncation limits."""
        assert "2000" in prompt  # read_file line limit
        assert "30,000" in prompt  # search/bash char cap
