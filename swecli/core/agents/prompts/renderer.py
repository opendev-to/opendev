"""Template rendering with variable substitution."""

import re
from pathlib import Path
from typing import Any

from .variables import PromptVariables


class PromptRenderer:
    """Renders prompt templates with variable substitution."""

    def __init__(self):
        self.variables = PromptVariables()

    def render(self, template_path: Path, **runtime_vars: Any) -> str:
        """Render template file with variable substitution.

        Args:
            template_path: Path to .md template file
            **runtime_vars: Runtime variables to inject

        Returns:
            Rendered prompt text with all variables substituted

        Example:
            renderer = PromptRenderer()
            content = renderer.render(
                Path("tool-enter-plan-mode.md"),
                SYSTEM_REMINDER=variables.get_system_reminder("/path/to/plan")
            )
        """
        content = template_path.read_text()

        # Strip YAML frontmatter (<!-- ... -->)
        content = re.sub(r"<!--.*?-->", "", content, flags=re.DOTALL).strip()

        # Get all variables
        var_dict = self.variables.to_dict(**runtime_vars)

        # Simple ${VAR} substitution
        def replace_var(match: re.Match) -> str:
            var_name = match.group(1)
            # Handle ${TOOL.name} -> tool name
            if "." in var_name:
                parts = var_name.split(".")
                obj = var_dict.get(parts[0])
                for part in parts[1:]:
                    obj = getattr(obj, part, var_name)
                return str(obj)
            # Handle ${VAR} -> direct value
            value = var_dict.get(var_name, match.group(0))
            # For boolean values, return empty string if False
            if isinstance(value, bool):
                return "" if not value else str(value)
            return str(value)

        content = re.sub(r"\$\{([^}]+)\}", replace_var, content)

        return content
