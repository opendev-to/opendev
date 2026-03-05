"""Data models for the hooks system."""

from __future__ import annotations

import re
from enum import Enum
from typing import Any, Optional

from pydantic import BaseModel, field_validator, model_validator


class HookEvent(str, Enum):
    """Lifecycle events that can trigger hooks."""

    SESSION_START = "SessionStart"
    USER_PROMPT_SUBMIT = "UserPromptSubmit"
    PRE_TOOL_USE = "PreToolUse"
    POST_TOOL_USE = "PostToolUse"
    POST_TOOL_USE_FAILURE = "PostToolUseFailure"
    SUBAGENT_START = "SubagentStart"
    SUBAGENT_STOP = "SubagentStop"
    STOP = "Stop"
    PRE_COMPACT = "PreCompact"
    SESSION_END = "SessionEnd"


# Valid event names for config validation
VALID_EVENT_NAMES = {e.value for e in HookEvent}


class HookCommand(BaseModel):
    """A single hook command to execute."""

    type: str = "command"
    command: str
    timeout: int = 60

    @field_validator("timeout")
    @classmethod
    def validate_timeout(cls, v: int) -> int:
        if v < 1:
            return 1
        if v > 600:
            return 600
        return v


class HookMatcher(BaseModel):
    """A matcher that filters when hooks fire, with associated commands."""

    matcher: Optional[str] = None
    hooks: list[HookCommand]

    # Compiled regex — not serialized
    _compiled_regex: Optional[re.Pattern[str]] = None

    model_config = {"arbitrary_types_allowed": True}

    @model_validator(mode="after")
    def compile_regex(self) -> "HookMatcher":
        if self.matcher:
            try:
                self._compiled_regex = re.compile(self.matcher)
            except re.error:
                self._compiled_regex = None
        return self

    def matches(self, value: Optional[str] = None) -> bool:
        """Check if this matcher matches the given value.

        Args:
            value: Value to match against (e.g., tool name, agent type).
                   If None and matcher is None, matches everything.

        Returns:
            True if the matcher matches.
        """
        if self.matcher is None:
            return True
        if value is None:
            return True
        if self._compiled_regex is None:
            return self.matcher == value
        return self._compiled_regex.search(value) is not None


class HookConfig(BaseModel):
    """Top-level hooks configuration."""

    hooks: dict[str, list[HookMatcher]] = {}

    @field_validator("hooks")
    @classmethod
    def validate_event_names(
        cls, v: dict[str, list[HookMatcher]]
    ) -> dict[str, list[HookMatcher]]:
        unknown = set(v.keys()) - VALID_EVENT_NAMES
        if unknown:
            # Silently drop unknown events (forward-compat)
            return {k: matchers for k, matchers in v.items() if k in VALID_EVENT_NAMES}
        return v

    def get_matchers(self, event: HookEvent) -> list[HookMatcher]:
        """Get matchers for a given event.

        Args:
            event: The hook event.

        Returns:
            List of HookMatcher for the event (may be empty).
        """
        return self.hooks.get(event.value, [])

    def has_hooks_for(self, event: HookEvent) -> bool:
        """Fast check: are there any matchers for this event?"""
        matchers = self.hooks.get(event.value, [])
        return len(matchers) > 0
