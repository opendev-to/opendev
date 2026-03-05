"""Tests for the prompt reminders module."""

import re

import pytest

from swecli.core.agents.prompts.reminders import get_reminder, _parse_sections


class TestParseSections:
    """Tests for section parsing from reminders.md."""

    def test_all_expected_sections_present(self):
        """Every expected section name is parsed."""
        sections = _parse_sections()
        expected = [
            "thinking_analysis_prompt",
            "thinking_trace_reminder",
            "subagent_complete_signal",
            "failed_tool_nudge",
            "nudge_permission_error",
            "nudge_file_not_found",
            "nudge_syntax_error",
            "nudge_rate_limit",
            "nudge_timeout",
            "nudge_edit_mismatch",
            "consecutive_reads_nudge",
            "safety_limit_summary",
            "thinking_on_instruction",
            "thinking_off_instruction",
            "incomplete_todos_nudge",
            "init_complete_signal",
            "plan_approved_signal",
            "all_todos_complete_nudge",
            "docker_command_failed_nudge",
            "file_exists_warning",
            "json_retry_simple",
            "json_retry_with_fields",
            "file_read_nudge",
            "plan_subagent_request",
            "tool_denied_nudge",
            "plan_file_reference",
        ]
        for name in expected:
            assert name in sections, f"Missing section: {name}"

    def test_sections_are_non_empty(self):
        """No section should be empty after parsing."""
        sections = _parse_sections()
        for name, content in sections.items():
            assert content.strip(), f"Section {name!r} is empty"

    def test_section_names_are_valid(self):
        """Section names should be snake_case identifiers."""
        sections = _parse_sections()
        for name in sections:
            assert re.match(r'^[a-z][a-z0-9_]*$', name), f"Invalid section name: {name!r}"


class TestGetReminder:
    """Tests for get_reminder() accessor."""

    # --- Simple lookups (no placeholders) ---

    def test_thinking_analysis_prompt(self):
        result = get_reminder("thinking_analysis_prompt")
        assert "Analyze the context" in result

    def test_failed_tool_nudge(self):
        result = get_reminder("failed_tool_nudge")
        assert "failed" in result.lower()
        assert "task_complete" in result

    def test_consecutive_reads_nudge(self):
        result = get_reminder("consecutive_reads_nudge")
        assert "proceed with implementation" in result.lower()

    def test_safety_limit_summary(self):
        result = get_reminder("safety_limit_summary")
        assert "summary" in result.lower()

    def test_subagent_complete_signal(self):
        result = get_reminder("subagent_complete_signal")
        assert "<subagent_complete>" in result
        assert "Evaluate" in result
        assert "Do NOT re-spawn the same subagent" in result
        assert "asked a question" in result

    def test_thinking_on_instruction(self):
        result = get_reminder("thinking_on_instruction")
        assert "THINKING MODE IS ON" in result
        assert "think" in result.lower()

    def test_thinking_off_instruction(self):
        result = get_reminder("thinking_off_instruction")
        assert "simple tasks" in result.lower()

    # --- Placeholder substitution ---

    def test_thinking_trace_reminder(self):
        trace = "Step 1: analyze. Step 2: act."
        result = get_reminder("thinking_trace_reminder", thinking_trace=trace)
        assert trace in result
        assert "<thinking_trace>" in result

    def test_incomplete_todos_nudge(self):
        result = get_reminder(
            "incomplete_todos_nudge",
            count="3",
            todo_list="  \u2022 task A\n  \u2022 task B\n  \u2022 task C",
        )
        assert "3 incomplete todo(s)" in result
        assert "task A" in result
        assert "Please complete" in result

    # --- File fallback (standalone .txt files) ---

    def test_docker_preamble(self):
        result = get_reminder("docker/docker_preamble", working_dir="/workspace")
        assert "/workspace" in result
        assert "DOCKER CONTAINER" in result

    def test_docker_context(self):
        result = get_reminder("docker/docker_context", workspace_dir="/testbed")
        assert "/testbed" in result
        assert "DOCKER CONTAINER" in result

    def test_custom_agent_default(self):
        result = get_reminder("generators/custom_agent_default", name="MyAgent", description="Does things")
        assert "MyAgent" in result
        assert "Does things" in result

    # --- Error handling ---

    def test_unknown_reminder_raises_key_error(self):
        with pytest.raises(KeyError, match="Unknown reminder"):
            get_reminder("nonexistent_reminder_name")

    def test_no_kwargs_returns_raw_template(self):
        """Calling without kwargs returns the raw template with placeholders."""
        result = get_reminder("incomplete_todos_nudge")
        assert "{count}" in result

    # --- Reminder sections ---

    def test_docker_command_failed_nudge(self):
        result = get_reminder("docker_command_failed_nudge", exit_code="1")
        assert "exit code 1" in result
        assert "COMMAND FAILED" in result
        assert "fix" in result.lower()

    def test_file_exists_warning(self):
        result = get_reminder("file_exists_warning")
        assert "exists" in result.lower()

    def test_json_retry_simple(self):
        result = get_reminder("json_retry_simple")
        assert "JSON" in result

    def test_json_retry_with_fields(self):
        result = get_reminder("json_retry_with_fields")
        assert "JSON" in result
        assert "required fields" in result

    # --- Centralized reminder sections ---

    def test_plan_subagent_request(self):
        result = get_reminder("plan_subagent_request")
        assert "planner" in result.lower()
        assert "spawn" in result.lower()

    def test_tool_denied_nudge(self):
        result = get_reminder("tool_denied_nudge")
        assert "denied" in result.lower()
        assert "adjust your approach" in result.lower()

    def test_plan_file_reference(self):
        result = get_reminder("plan_file_reference", plan_file_path="/tmp/plan.md")
        assert "/tmp/plan.md" in result
        assert "plan file exists" in result.lower()
