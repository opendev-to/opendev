"""Load hooks configuration from settings files."""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

from swecli.core.hooks.models import HookConfig, HookMatcher, VALID_EVENT_NAMES

logger = logging.getLogger(__name__)


def _read_hooks_from_file(path: Path) -> dict[str, list[dict[str, Any]]]:
    """Read the 'hooks' key from a settings.json file.

    Args:
        path: Path to settings.json file.

    Returns:
        Dict mapping event name to list of raw matcher dicts.
        Empty dict if file doesn't exist or has no hooks key.
    """
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        hooks = data.get("hooks", {})
        if not isinstance(hooks, dict):
            return {}
        return hooks
    except (json.JSONDecodeError, OSError) as e:
        logger.warning("Failed to read hooks from %s: %s", path, e)
        return {}


def load_hooks_config(working_dir: Path | str | None = None) -> HookConfig:
    """Load hooks configuration from global and project settings.

    Merges hooks from:
    1. ~/.opendev/settings.json (global)
    2. <working_dir>/.opendev/settings.json (project)

    Project matchers are appended after global matchers for the same event,
    giving project hooks the ability to override or extend global hooks.

    Args:
        working_dir: Working directory for project-level settings.
                    Defaults to current directory.

    Returns:
        HookConfig with merged hooks from both sources.
    """
    from swecli.core.paths import get_paths

    wd = Path(working_dir) if working_dir else None
    paths = get_paths(wd)

    # Read from both config files
    global_hooks = _read_hooks_from_file(paths.global_settings)
    project_hooks = _read_hooks_from_file(paths.project_settings)

    # Merge: project matchers appended to global for the same event
    merged: dict[str, list[dict[str, Any]]] = {}

    all_events = set(global_hooks.keys()) | set(project_hooks.keys())
    for event_name in all_events:
        if event_name not in VALID_EVENT_NAMES:
            continue
        global_matchers = global_hooks.get(event_name, [])
        project_matchers = project_hooks.get(event_name, [])
        if not isinstance(global_matchers, list):
            global_matchers = []
        if not isinstance(project_matchers, list):
            project_matchers = []
        merged[event_name] = global_matchers + project_matchers

    try:
        return HookConfig(hooks=merged)
    except Exception as e:
        logger.warning("Failed to parse hooks config: %s", e)
        return HookConfig()
