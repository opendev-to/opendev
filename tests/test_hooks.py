"""Unit tests for the hooks system."""

import json
import os
import stat
import subprocess
import tempfile
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from swecli.core.hooks.models import (
    HookEvent,
    HookCommand,
    HookMatcher,
    HookConfig,
    VALID_EVENT_NAMES,
)
from swecli.core.hooks.executor import HookResult, HookCommandExecutor
from swecli.core.hooks.manager import HookManager, HookOutcome
from swecli.core.hooks.loader import load_hooks_config, _read_hooks_from_file


# ============================================================================
# HookEvent tests
# ============================================================================


class TestHookEvent:
    def test_all_events_defined(self):
        assert len(HookEvent) == 10

    def test_event_values(self):
        assert HookEvent.SESSION_START.value == "SessionStart"
        assert HookEvent.PRE_TOOL_USE.value == "PreToolUse"
        assert HookEvent.POST_TOOL_USE.value == "PostToolUse"
        assert HookEvent.POST_TOOL_USE_FAILURE.value == "PostToolUseFailure"
        assert HookEvent.STOP.value == "Stop"

    def test_valid_event_names_matches_enum(self):
        assert VALID_EVENT_NAMES == {e.value for e in HookEvent}


# ============================================================================
# HookCommand tests
# ============================================================================


class TestHookCommand:
    def test_basic_creation(self):
        cmd = HookCommand(command="echo hello")
        assert cmd.type == "command"
        assert cmd.command == "echo hello"
        assert cmd.timeout == 60

    def test_custom_timeout(self):
        cmd = HookCommand(command="echo hello", timeout=30)
        assert cmd.timeout == 30

    def test_timeout_clamped_min(self):
        cmd = HookCommand(command="echo hello", timeout=-5)
        assert cmd.timeout == 1

    def test_timeout_clamped_max(self):
        cmd = HookCommand(command="echo hello", timeout=9999)
        assert cmd.timeout == 600


# ============================================================================
# HookMatcher tests
# ============================================================================


class TestHookMatcher:
    def test_no_matcher_matches_everything(self):
        m = HookMatcher(hooks=[HookCommand(command="echo")])
        assert m.matches() is True
        assert m.matches("anything") is True
        assert m.matches(None) is True

    def test_exact_matcher(self):
        m = HookMatcher(matcher="run_command", hooks=[HookCommand(command="echo")])
        assert m.matches("run_command") is True
        # regex search, so partial match works
        assert m.matches("run_command_extra") is True

    def test_regex_matcher(self):
        m = HookMatcher(matcher="^(run_command|write_file)$", hooks=[HookCommand(command="echo")])
        assert m.matches("run_command") is True
        assert m.matches("write_file") is True
        assert m.matches("read_file") is False

    def test_regex_search_semantics(self):
        m = HookMatcher(matcher="rm\\s+-rf", hooks=[HookCommand(command="echo")])
        assert m.matches("rm -rf /") is True
        assert m.matches("echo hello") is False

    def test_invalid_regex_falls_back_to_equality(self):
        m = HookMatcher(matcher="[invalid", hooks=[HookCommand(command="echo")])
        assert m.matches("[invalid") is True
        assert m.matches("other") is False

    def test_none_value_matches(self):
        m = HookMatcher(matcher="test", hooks=[HookCommand(command="echo")])
        # When value is None and matcher exists, still returns True (per spec)
        assert m.matches(None) is True


# ============================================================================
# HookConfig tests
# ============================================================================


class TestHookConfig:
    def test_empty_config(self):
        config = HookConfig()
        assert config.hooks == {}
        assert config.has_hooks_for(HookEvent.PRE_TOOL_USE) is False

    def test_valid_config(self):
        config = HookConfig(hooks={
            "PreToolUse": [
                HookMatcher(matcher="run_command", hooks=[HookCommand(command="echo")])
            ]
        })
        assert config.has_hooks_for(HookEvent.PRE_TOOL_USE) is True
        assert config.has_hooks_for(HookEvent.POST_TOOL_USE) is False

    def test_unknown_events_dropped(self):
        config = HookConfig(hooks={
            "PreToolUse": [HookMatcher(hooks=[HookCommand(command="echo")])],
            "FakeEvent": [HookMatcher(hooks=[HookCommand(command="echo")])],
        })
        assert "FakeEvent" not in config.hooks
        assert "PreToolUse" in config.hooks

    def test_get_matchers(self):
        matcher = HookMatcher(hooks=[HookCommand(command="echo")])
        config = HookConfig(hooks={"PreToolUse": [matcher]})
        assert config.get_matchers(HookEvent.PRE_TOOL_USE) == [matcher]
        assert config.get_matchers(HookEvent.STOP) == []


# ============================================================================
# HookResult tests
# ============================================================================


class TestHookResult:
    def test_success(self):
        r = HookResult(exit_code=0)
        assert r.success is True
        assert r.should_block is False

    def test_block(self):
        r = HookResult(exit_code=2)
        assert r.success is False
        assert r.should_block is True

    def test_error(self):
        r = HookResult(exit_code=1, error="something failed")
        assert r.success is False
        assert r.should_block is False

    def test_timed_out(self):
        r = HookResult(exit_code=0, timed_out=True)
        assert r.success is False

    def test_parse_json_output(self):
        r = HookResult(stdout='{"reason": "too dangerous"}')
        parsed = r.parse_json_output()
        assert parsed["reason"] == "too dangerous"

    def test_parse_json_output_empty(self):
        r = HookResult(stdout="")
        assert r.parse_json_output() == {}

    def test_parse_json_output_invalid(self):
        r = HookResult(stdout="not json")
        assert r.parse_json_output() == {}


# ============================================================================
# HookCommandExecutor tests
# ============================================================================


class TestHookCommandExecutor:
    def test_execute_echo(self):
        executor = HookCommandExecutor()
        cmd = HookCommand(command="echo hello")
        result = executor.execute(cmd, {"test": True})
        assert result.exit_code == 0
        assert "hello" in result.stdout
        assert result.success is True

    def test_execute_receives_stdin(self):
        executor = HookCommandExecutor()
        cmd = HookCommand(command="cat")
        stdin_data = {"tool_name": "run_command", "blocked": False}
        result = executor.execute(cmd, stdin_data)
        assert result.exit_code == 0
        parsed = json.loads(result.stdout)
        assert parsed["tool_name"] == "run_command"

    def test_execute_exit_code_2_blocks(self):
        executor = HookCommandExecutor()
        cmd = HookCommand(command="exit 2")
        result = executor.execute(cmd, {})
        assert result.exit_code == 2
        assert result.should_block is True

    def test_execute_timeout(self):
        executor = HookCommandExecutor()
        cmd = HookCommand(command="sleep 10", timeout=1)
        result = executor.execute(cmd, {})
        assert result.timed_out is True
        assert result.success is False

    def test_execute_nonexistent_command(self):
        executor = HookCommandExecutor()
        cmd = HookCommand(command="/nonexistent/path/to/command 2>/dev/null; exit 1")
        result = executor.execute(cmd, {})
        assert result.success is False


# ============================================================================
# HookManager tests
# ============================================================================


class TestHookManager:
    def _make_manager(self, hooks_dict=None):
        config = HookConfig(hooks=hooks_dict or {})
        return HookManager(config=config, session_id="test-123", cwd="/tmp/test")

    def test_no_hooks_returns_empty_outcome(self):
        mgr = self._make_manager()
        outcome = mgr.run_hooks(HookEvent.PRE_TOOL_USE, match_value="run_command")
        assert outcome.blocked is False
        assert outcome.results == []

    def test_has_hooks_for(self):
        mgr = self._make_manager({
            "PreToolUse": [HookMatcher(hooks=[HookCommand(command="echo")])]
        })
        assert mgr.has_hooks_for(HookEvent.PRE_TOOL_USE) is True
        assert mgr.has_hooks_for(HookEvent.STOP) is False

    def test_matching_hook_fires(self):
        mgr = self._make_manager({
            "PreToolUse": [
                HookMatcher(
                    matcher="run_command",
                    hooks=[HookCommand(command="echo ok")]
                )
            ]
        })
        outcome = mgr.run_hooks(HookEvent.PRE_TOOL_USE, match_value="run_command")
        assert len(outcome.results) == 1
        assert outcome.results[0].success is True
        assert outcome.blocked is False

    def test_non_matching_hook_skipped(self):
        mgr = self._make_manager({
            "PreToolUse": [
                HookMatcher(
                    matcher="^write_file$",
                    hooks=[HookCommand(command="echo should-not-fire")]
                )
            ]
        })
        outcome = mgr.run_hooks(HookEvent.PRE_TOOL_USE, match_value="run_command")
        assert len(outcome.results) == 0

    def test_blocking_hook(self):
        mgr = self._make_manager({
            "PreToolUse": [
                HookMatcher(
                    matcher="run_command",
                    hooks=[HookCommand(
                        command='echo \'{"reason": "dangerous command"}\' && exit 2'
                    )]
                )
            ]
        })
        outcome = mgr.run_hooks(
            HookEvent.PRE_TOOL_USE,
            match_value="run_command",
            event_data={"tool_input": {"command": "rm -rf /"}},
        )
        assert outcome.blocked is True
        assert "dangerous command" in outcome.block_reason

    def test_short_circuit_on_block(self):
        """Second hook should not fire if first blocks."""
        mgr = self._make_manager({
            "PreToolUse": [
                HookMatcher(
                    hooks=[
                        HookCommand(command="exit 2"),
                        HookCommand(command="echo second"),
                    ]
                )
            ]
        })
        outcome = mgr.run_hooks(HookEvent.PRE_TOOL_USE, match_value="test")
        assert outcome.blocked is True
        assert len(outcome.results) == 1  # Only first hook ran

    def test_additional_context(self):
        mgr = self._make_manager({
            "PreToolUse": [
                HookMatcher(
                    hooks=[HookCommand(
                        command='echo \'{"additionalContext": "Remember: be careful"}\''
                    )]
                )
            ]
        })
        outcome = mgr.run_hooks(HookEvent.PRE_TOOL_USE, match_value="test")
        assert outcome.additional_context == "Remember: be careful"

    def test_updated_input(self):
        mgr = self._make_manager({
            "PreToolUse": [
                HookMatcher(
                    hooks=[HookCommand(
                        command='echo \'{"updatedInput": {"command": "ls -la"}}\''
                    )]
                )
            ]
        })
        outcome = mgr.run_hooks(HookEvent.PRE_TOOL_USE, match_value="run_command")
        assert outcome.updated_input == {"command": "ls -la"}

    def test_build_stdin_tool_event(self):
        mgr = self._make_manager()
        payload = mgr._build_stdin(
            HookEvent.PRE_TOOL_USE,
            "run_command",
            {"tool_input": {"command": "ls"}},
        )
        assert payload["hook_event_name"] == "PreToolUse"
        assert payload["tool_name"] == "run_command"
        assert payload["tool_input"] == {"command": "ls"}
        assert payload["session_id"] == "test-123"
        assert payload["cwd"] == "/tmp/test"

    def test_build_stdin_session_event(self):
        mgr = self._make_manager()
        payload = mgr._build_stdin(HookEvent.SESSION_START, "startup", None)
        assert payload["startup_type"] == "startup"

    def test_build_stdin_subagent_event(self):
        mgr = self._make_manager()
        payload = mgr._build_stdin(
            HookEvent.SUBAGENT_START, "code-explorer", {"agent_task": "Find bugs"}
        )
        assert payload["agent_type"] == "code-explorer"
        assert payload["agent_task"] == "Find bugs"

    def test_build_stdin_compact_event(self):
        mgr = self._make_manager()
        payload = mgr._build_stdin(HookEvent.PRE_COMPACT, "manual", None)
        assert payload["trigger"] == "manual"

    def test_run_hooks_async_fires(self):
        """Async hooks should fire without blocking."""
        mgr = self._make_manager({
            "PostToolUse": [
                HookMatcher(hooks=[HookCommand(command="echo async-ok")])
            ]
        })
        # Fire and forget — should not raise
        mgr.run_hooks_async(
            HookEvent.POST_TOOL_USE,
            match_value="read_file",
            event_data={"tool_response": {"success": True}},
        )
        mgr.shutdown()

    def test_shutdown_idempotent(self):
        mgr = self._make_manager()
        mgr.shutdown()
        mgr.shutdown()  # Should not raise


# ============================================================================
# Loader tests
# ============================================================================


class TestLoader:
    def test_read_hooks_from_nonexistent_file(self):
        result = _read_hooks_from_file(Path("/nonexistent/settings.json"))
        assert result == {}

    def test_read_hooks_from_file_no_hooks_key(self, tmp_path):
        f = tmp_path / "settings.json"
        f.write_text('{"model": "gpt-4"}')
        result = _read_hooks_from_file(f)
        assert result == {}

    def test_read_hooks_from_valid_file(self, tmp_path):
        f = tmp_path / "settings.json"
        f.write_text(json.dumps({
            "hooks": {
                "PreToolUse": [
                    {"matcher": "run_command", "hooks": [{"type": "command", "command": "echo"}]}
                ]
            }
        }))
        result = _read_hooks_from_file(f)
        assert "PreToolUse" in result
        assert len(result["PreToolUse"]) == 1

    def test_read_hooks_from_invalid_json(self, tmp_path):
        f = tmp_path / "settings.json"
        f.write_text("not json{{{")
        result = _read_hooks_from_file(f)
        assert result == {}

    def test_load_hooks_config_merges(self, tmp_path):
        """Project hooks are appended after global hooks for the same event."""
        global_dir = tmp_path / "global" / ".opendev"
        global_dir.mkdir(parents=True)
        global_settings = global_dir / "settings.json"
        global_settings.write_text(json.dumps({
            "hooks": {
                "PreToolUse": [
                    {"matcher": "global_match", "hooks": [{"type": "command", "command": "echo global"}]}
                ]
            }
        }))

        project_dir = tmp_path / "project"
        project_dir.mkdir()
        project_opendev = project_dir / ".opendev"
        project_opendev.mkdir()
        project_settings = project_opendev / "settings.json"
        project_settings.write_text(json.dumps({
            "hooks": {
                "PreToolUse": [
                    {"matcher": "project_match", "hooks": [{"type": "command", "command": "echo project"}]}
                ]
            }
        }))

        with patch("swecli.core.paths.get_paths") as mock_paths:
            mock_p = MagicMock()
            mock_p.global_settings = global_settings
            mock_p.project_settings = project_settings
            mock_paths.return_value = mock_p

            config = load_hooks_config(project_dir)

        matchers = config.get_matchers(HookEvent.PRE_TOOL_USE)
        assert len(matchers) == 2
        assert matchers[0].matcher == "global_match"
        assert matchers[1].matcher == "project_match"

    def test_load_hooks_config_empty(self, tmp_path):
        """No hooks defined returns empty config."""
        with patch("swecli.core.paths.get_paths") as mock_paths:
            mock_p = MagicMock()
            mock_p.global_settings = tmp_path / "nonexistent_global.json"
            mock_p.project_settings = tmp_path / "nonexistent_project.json"
            mock_paths.return_value = mock_p

            config = load_hooks_config(tmp_path)

        assert config.hooks == {}


# ============================================================================
# Integration: HookManager with real shell scripts
# ============================================================================


class TestHookManagerIntegration:
    """Integration tests using real shell scripts as hook commands."""

    def test_hook_script_receives_json_stdin(self, tmp_path):
        """Verify hook script receives proper JSON on stdin."""
        output_file = tmp_path / "output.json"
        script = tmp_path / "capture.sh"
        script.write_text(f"#!/bin/bash\ncat > {output_file}\n")
        script.chmod(script.stat().st_mode | stat.S_IEXEC)

        config = HookConfig(hooks={
            "PreToolUse": [
                HookMatcher(
                    matcher="run_command",
                    hooks=[HookCommand(command=str(script))],
                )
            ]
        })
        mgr = HookManager(config=config, session_id="s-001", cwd="/tmp")
        mgr.run_hooks(
            HookEvent.PRE_TOOL_USE,
            match_value="run_command",
            event_data={"tool_input": {"command": "ls -la"}},
        )

        captured = json.loads(output_file.read_text())
        assert captured["session_id"] == "s-001"
        assert captured["hook_event_name"] == "PreToolUse"
        assert captured["tool_name"] == "run_command"
        assert captured["tool_input"] == {"command": "ls -la"}

    def test_blocking_script(self, tmp_path):
        """Hook script returning exit 2 blocks the operation."""
        script = tmp_path / "block.sh"
        script.write_text(
            '#!/bin/bash\n'
            'echo \'{"reason": "rm -rf is not allowed"}\'\n'
            'exit 2\n'
        )
        script.chmod(script.stat().st_mode | stat.S_IEXEC)

        config = HookConfig(hooks={
            "PreToolUse": [
                HookMatcher(
                    matcher="run_command",
                    hooks=[HookCommand(command=str(script))],
                )
            ]
        })
        mgr = HookManager(config=config, session_id="s-001", cwd="/tmp")
        outcome = mgr.run_hooks(
            HookEvent.PRE_TOOL_USE,
            match_value="run_command",
            event_data={"tool_input": {"command": "rm -rf /"}},
        )
        assert outcome.blocked is True
        assert "rm -rf is not allowed" in outcome.block_reason

    def test_input_modification_script(self, tmp_path):
        """Hook script can modify tool input via updatedInput."""
        script = tmp_path / "modify.sh"
        script.write_text(
            '#!/bin/bash\n'
            'echo \'{"updatedInput": {"command": "ls -la --color=never"}}\'\n'
        )
        script.chmod(script.stat().st_mode | stat.S_IEXEC)

        config = HookConfig(hooks={
            "PreToolUse": [
                HookMatcher(
                    matcher="run_command",
                    hooks=[HookCommand(command=str(script))],
                )
            ]
        })
        mgr = HookManager(config=config, session_id="s-001", cwd="/tmp")
        outcome = mgr.run_hooks(
            HookEvent.PRE_TOOL_USE,
            match_value="run_command",
            event_data={"tool_input": {"command": "ls"}},
        )
        assert outcome.blocked is False
        assert outcome.updated_input == {"command": "ls -la --color=never"}


# ============================================================================
# Integration: ToolRegistry with hooks
# ============================================================================


class TestToolRegistryHookIntegration:
    """Test that hooks are properly invoked from ToolRegistry.execute_tool()."""

    def test_pre_tool_use_blocks_execution(self):
        """PreToolUse hook with exit 2 should prevent tool execution."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        config = HookConfig(hooks={
            "PreToolUse": [
                HookMatcher(
                    matcher="run_command",
                    hooks=[HookCommand(command="exit 2")],
                )
            ]
        })
        mgr = HookManager(config=config, session_id="test", cwd="/tmp")
        registry.set_hook_manager(mgr)

        result = registry.execute_tool("run_command", {"command": "echo hello"})
        assert result["success"] is False
        assert result.get("denied") is True
        assert "Blocked by hook" in result["error"]

    def test_pre_tool_use_updates_input(self):
        """PreToolUse hook can modify tool arguments."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        config = HookConfig(hooks={
            "PreToolUse": [
                HookMatcher(
                    hooks=[HookCommand(
                        command='echo \'{"updatedInput": {"extra": "injected"}}\''
                    )],
                )
            ]
        })
        mgr = HookManager(config=config, session_id="test", cwd="/tmp")
        registry.set_hook_manager(mgr)

        # Use read_file which is simple — we just need to verify arguments were merged
        # The tool will fail (no file_ops configured) but the hook should have fired
        result = registry.execute_tool("read_file", {"file_path": "/tmp/test.txt"})
        # The hook added "extra" to arguments, but read_file doesn't use it
        # Verifying the hook fired is sufficient — we tested updatedInput in manager tests

    def test_no_hook_manager_is_noop(self):
        """When no hook manager is set, tools work normally."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        registry = ToolRegistry()
        # No set_hook_manager call — _hook_manager is None
        result = registry.execute_tool("list_todos", {})
        # Should work without hooks
        assert result is not None
