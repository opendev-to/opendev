"""Approval rules system for pattern-based command approval.

Rules can be session-only (ephemeral) or persistent across sessions.
Persistent rules are stored in:
  - User-global: ~/.opendev/permissions.json
  - Project-scoped: .opendev/permissions.json
"""

from __future__ import annotations

import json
import logging
import re
from dataclasses import asdict, dataclass
from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Any, Dict, List, Optional

logger = logging.getLogger(__name__)


class RuleAction(Enum):
    AUTO_APPROVE = "auto_approve"
    AUTO_DENY = "auto_deny"
    REQUIRE_APPROVAL = "require_approval"
    REQUIRE_EDIT = "require_edit"


class RuleType(Enum):
    PATTERN = "pattern"
    COMMAND = "command"
    PREFIX = "prefix"
    DANGER = "danger"


@dataclass
class ApprovalRule:
    id: str
    name: str
    description: str
    rule_type: RuleType
    pattern: str
    action: RuleAction
    enabled: bool = True
    priority: int = 0
    created_at: Optional[str] = None
    modified_at: Optional[str] = None

    def matches(self, command: str) -> bool:
        if not self.enabled:
            return False

        if self.rule_type == RuleType.PATTERN:
            try:
                return bool(re.search(self.pattern, command))
            except re.error:
                return False
        if self.rule_type == RuleType.COMMAND:
            return command == self.pattern
        if self.rule_type == RuleType.PREFIX:
            # Match exact command OR command with additional args
            return command == self.pattern or command.startswith(self.pattern + " ")
        if self.rule_type == RuleType.DANGER:
            try:
                return bool(re.search(self.pattern, command))
            except re.error:
                return False
        return False

    def to_dict(self) -> Dict[str, Any]:
        data = asdict(self)
        data["rule_type"] = self.rule_type.value
        data["action"] = self.action.value
        return data

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "ApprovalRule":
        data["rule_type"] = RuleType(data["rule_type"])
        data["action"] = RuleAction(data["action"])
        return cls(**data)


@dataclass
class CommandHistory:
    command: str
    approved: bool
    edited_command: Optional[str] = None
    timestamp: Optional[str] = None
    rule_matched: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "CommandHistory":
        return cls(**data)


class ApprovalRulesManager:
    """Manager for approval rules and command history.

    Supports both session-only (ephemeral) and persistent rules.
    Persistent rules are loaded from disk on init and survive across sessions.
    """

    # Default persistence paths
    USER_PERMISSIONS_PATH = Path.home() / ".opendev" / "permissions.json"

    def __init__(self, project_dir: Optional[str] = None) -> None:
        self.rules: List[ApprovalRule] = []
        self.history: List[CommandHistory] = []
        self._project_dir = project_dir
        self._initialize_default_rules()
        self._load_persistent_rules()

    def _initialize_default_rules(self) -> None:
        """Initialize default danger rules for the session."""
        default_rules = [
            ApprovalRule(
                id="default_danger_rm",
                name="Dangerous rm commands",
                description="Require approval for dangerous rm commands",
                rule_type=RuleType.DANGER,
                pattern=r"rm\s+(-rf?|-fr?)\s+(/|\*|~)",
                action=RuleAction.REQUIRE_APPROVAL,
                priority=100,
                created_at=datetime.now().isoformat(),
            ),
            ApprovalRule(
                id="default_danger_chmod",
                name="Dangerous chmod 777",
                description="Require approval for chmod 777",
                rule_type=RuleType.DANGER,
                pattern=r"chmod\s+777",
                action=RuleAction.REQUIRE_APPROVAL,
                priority=100,
                created_at=datetime.now().isoformat(),
            ),
        ]

        self.rules.extend(default_rules)

    def evaluate_command(self, command: str) -> Optional[ApprovalRule]:
        enabled_rules = [r for r in self.rules if r.enabled]
        for rule in sorted(enabled_rules, key=lambda r: r.priority, reverse=True):
            if rule.matches(command):
                return rule
        return None

    def add_rule(self, rule: ApprovalRule) -> None:
        self.rules.append(rule)

    def update_rule(self, rule_id: str, **updates: Any) -> bool:
        for rule in self.rules:
            if rule.id == rule_id:
                for key, value in updates.items():
                    setattr(rule, key, value)
                rule.modified_at = datetime.now().isoformat()
                return True
        return False

    def remove_rule(self, rule_id: str) -> bool:
        before = len(self.rules)
        self.rules = [r for r in self.rules if r.id != rule_id]
        return len(self.rules) != before

    def add_history(self, command: str, approved: bool, *, edited_command: Optional[str] = None, rule_matched: Optional[str] = None) -> None:
        entry = CommandHistory(
            command=command,
            approved=approved,
            edited_command=edited_command,
            timestamp=datetime.now().isoformat(),
            rule_matched=rule_matched,
        )
        self.history.append(entry)

    # ------------------------------------------------------------------
    # Persistent rules
    # ------------------------------------------------------------------
    def add_persistent_rule(self, rule: ApprovalRule, *, scope: str = "user") -> None:
        """Add a rule and persist it to disk.

        Args:
            rule: The approval rule to add and persist.
            scope: "user" for ~/.opendev/permissions.json,
                   "project" for .opendev/permissions.json in the project dir.
        """
        self.add_rule(rule)
        self._save_persistent_rules(scope=scope)

    def remove_persistent_rule(self, rule_id: str) -> bool:
        """Remove a rule and update persistent storage."""
        removed = self.remove_rule(rule_id)
        if removed:
            # Save to both scopes (the rule may have been in either)
            self._save_persistent_rules(scope="user")
            if self._project_dir:
                self._save_persistent_rules(scope="project")
        return removed

    def clear_persistent_rules(self, *, scope: str = "user") -> int:
        """Remove all persistent (non-default) rules.

        Args:
            scope: Which scope to clear ("user", "project", or "all").

        Returns:
            Number of rules removed.
        """
        default_ids = {r.id for r in self.rules if r.id.startswith("default_")}
        before = len(self.rules)
        self.rules = [r for r in self.rules if r.id in default_ids]
        removed = before - len(self.rules)

        if scope in ("user", "all"):
            self._delete_permissions_file(self.USER_PERMISSIONS_PATH)
        if scope in ("project", "all") and self._project_dir:
            project_path = Path(self._project_dir) / ".opendev" / "permissions.json"
            self._delete_permissions_file(project_path)

        return removed

    def list_persistent_rules(self) -> List[Dict[str, Any]]:
        """List all non-default rules in a display-friendly format."""
        return [
            {
                "id": r.id,
                "name": r.name,
                "pattern": r.pattern,
                "action": r.action.value,
                "type": r.rule_type.value,
                "enabled": r.enabled,
            }
            for r in self.rules
            if not r.id.startswith("default_")
        ]

    def _load_persistent_rules(self) -> None:
        """Load persistent rules from disk (user-global + project-scoped)."""
        # User-global rules
        self._load_rules_from_file(self.USER_PERMISSIONS_PATH)

        # Project-scoped rules (higher priority)
        if self._project_dir:
            project_path = Path(self._project_dir) / ".opendev" / "permissions.json"
            self._load_rules_from_file(project_path)

    def _load_rules_from_file(self, path: Path) -> None:
        """Load rules from a single permissions file."""
        if not path.exists():
            return
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            for rule_data in data.get("rules", []):
                # Skip if rule ID already exists (avoid duplicates)
                if any(r.id == rule_data.get("id") for r in self.rules):
                    continue
                rule = ApprovalRule.from_dict(rule_data)
                self.rules.append(rule)
            logger.debug("Loaded %d rules from %s", len(data.get("rules", [])), path)
        except Exception:
            logger.warning("Failed to load persistent rules from %s", path, exc_info=True)

    def _save_persistent_rules(self, *, scope: str = "user") -> None:
        """Save non-default rules to disk."""
        persistent_rules = [r.to_dict() for r in self.rules if not r.id.startswith("default_")]
        data = {"version": 1, "rules": persistent_rules}

        if scope == "user":
            path = self.USER_PERMISSIONS_PATH
        elif scope == "project" and self._project_dir:
            path = Path(self._project_dir) / ".opendev" / "permissions.json"
        else:
            return

        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(data, indent=2), encoding="utf-8")
            logger.debug("Saved %d rules to %s", len(persistent_rules), path)
        except OSError:
            logger.warning("Failed to save persistent rules to %s", path, exc_info=True)

    @staticmethod
    def _delete_permissions_file(path: Path) -> None:
        """Delete a permissions file if it exists."""
        try:
            if path.exists():
                path.unlink()
        except OSError:
            pass
