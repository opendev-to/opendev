"""Skills system for lazy-loaded knowledge modules.

Skills are markdown files with frontmatter that inject knowledge and instructions
into the main agent context on demand. Unlike subagents (separate sessions),
skills extend the current conversation's capabilities.

## Directory Structure
Skills are loaded from:
- ~/.opendev/skills/ (user global)
- <project>/.opendev/skills/ (project local, takes priority)

## Skill File Format
```markdown
---
name: commit
description: Git commit best practices and message formatting
namespace: default
---

# Git Commit Skill

When making commits:
1. Use conventional commit format...
```

## Usage
The main agent sees available skills in its system prompt and can invoke
them via the `invoke_skill` tool to load full content into context.
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from swecli.core.paths import get_paths

try:
    import yaml

    YAML_AVAILABLE = True
except ImportError:
    YAML_AVAILABLE = False

logger = logging.getLogger(__name__)


@dataclass
class SkillMetadata:
    """Metadata extracted from skill file frontmatter."""

    name: str
    description: str
    namespace: str = "default"
    path: Path | None = None
    source: str = "unknown"  # "user-global" or "project"


@dataclass
class LoadedSkill:
    """A fully loaded skill with content."""

    metadata: SkillMetadata
    content: str


class SkillLoader:
    """Discovers and loads skills from configured directories.

    Skills are discovered lazily - only metadata is read at startup.
    Full content is loaded on-demand when the skill is invoked.
    """

    def __init__(self, skill_dirs: list[Path]) -> None:
        """Initialize the skill loader.

        Args:
            skill_dirs: List of directories to search for skills, in priority order.
                        First directory has highest priority (typically project local).
        """
        self._dirs = skill_dirs
        self._cache: dict[str, LoadedSkill] = {}
        self._metadata_cache: dict[str, SkillMetadata] = {}

    def discover_skills(self) -> list[SkillMetadata]:
        """Scan skill directories for .md files and extract metadata.

        Returns:
            List of SkillMetadata for all discovered skills.
            Project-local skills override user-global skills with the same name.
        """
        skills: dict[str, SkillMetadata] = {}  # name -> metadata, for deduplication

        # Process directories in reverse order so higher priority dirs override
        for skill_dir in reversed(self._dirs):
            if not skill_dir.exists():
                continue

            source = self._detect_source(skill_dir)

            for md_file in skill_dir.glob("**/*.md"):
                metadata = self._parse_frontmatter(md_file)
                if metadata:
                    metadata.path = md_file
                    metadata.source = source

                    # Build full name with namespace
                    full_name = (
                        f"{metadata.namespace}:{metadata.name}"
                        if metadata.namespace != "default"
                        else metadata.name
                    )
                    skills[full_name] = metadata

        # Cache metadata for later lookup
        self._metadata_cache = skills

        return list(skills.values())

    def _detect_source(self, skill_dir: Path) -> str:
        """Detect if skill directory is user-global, project-local, or builtin."""
        global_skills_dir = get_paths().global_skills_dir
        if skill_dir == global_skills_dir or str(skill_dir).startswith(str(global_skills_dir)):
            return "user-global"
        builtin_dir = get_paths().builtin_skills_dir
        if skill_dir == builtin_dir or str(skill_dir).startswith(str(builtin_dir)):
            return "builtin"
        return "project"

    def load_skill(self, name: str) -> LoadedSkill | None:
        """Load full skill content by name.

        Args:
            name: Skill name, optionally with namespace (e.g., "commit" or "git:commit")

        Returns:
            LoadedSkill with full content, or None if not found
        """
        # Check cache first
        if name in self._cache:
            return self._cache[name]

        # Ensure metadata is loaded
        if not self._metadata_cache:
            self.discover_skills()

        # Look up metadata
        metadata = self._metadata_cache.get(name)
        if not metadata or not metadata.path:
            # Try without namespace prefix
            for full_name, meta in self._metadata_cache.items():
                if meta.name == name:
                    metadata = meta
                    break

        if not metadata or not metadata.path:
            logger.warning(f"Skill not found: {name}")
            return None

        # Load full content
        try:
            full_content = metadata.path.read_text(encoding="utf-8")

            # Remove frontmatter from content
            content = self._strip_frontmatter(full_content)

            skill = LoadedSkill(metadata=metadata, content=content)
            self._cache[name] = skill
            return skill

        except OSError as e:
            logger.error(f"Failed to load skill {name}: {e}")
            return None

    def _parse_frontmatter(self, path: Path) -> SkillMetadata | None:
        """Parse YAML frontmatter from skill file.

        Args:
            path: Path to markdown file

        Returns:
            SkillMetadata if valid frontmatter found, None otherwise
        """
        try:
            content = path.read_text(encoding="utf-8")
        except OSError as e:
            logger.warning(f"Failed to read skill file {path}: {e}")
            return None

        # Match YAML frontmatter between --- delimiters
        match = re.match(r"^---\n(.*?)\n---", content, re.DOTALL)
        if not match:
            return None

        frontmatter_text = match.group(1)

        # Parse YAML
        if YAML_AVAILABLE:
            try:
                data = yaml.safe_load(frontmatter_text)
            except yaml.YAMLError as e:
                logger.warning(f"Invalid YAML in skill file {path}: {e}")
                return None
        else:
            # Fallback: simple key-value parsing
            data = self._parse_simple_yaml(frontmatter_text)

        if not isinstance(data, dict):
            return None

        # Validate required fields
        name = data.get("name")
        if not name:
            # Fall back to filename without extension
            name = path.stem

        description = data.get("description", f"Skill: {name}")
        namespace = data.get("namespace", "default")

        return SkillMetadata(name=name, description=description, namespace=namespace)

    def _parse_simple_yaml(self, text: str) -> dict[str, Any]:
        """Simple YAML-like parsing fallback when PyYAML is not available.

        Only handles simple key: value pairs, not nested structures.
        """
        result: dict[str, Any] = {}
        for line in text.split("\n"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if ":" in line:
                key, _, value = line.partition(":")
                key = key.strip()
                value = value.strip()
                # Remove quotes if present
                if value.startswith('"') and value.endswith('"'):
                    value = value[1:-1]
                elif value.startswith("'") and value.endswith("'"):
                    value = value[1:-1]
                result[key] = value
        return result

    def _strip_frontmatter(self, content: str) -> str:
        """Remove YAML frontmatter from markdown content."""
        match = re.match(r"^---\n.*?\n---\n*", content, re.DOTALL)
        if match:
            return content[match.end() :]
        return content

    def build_skills_index(self) -> str:
        """Build skills index for system prompt inclusion.

        Returns:
            Formatted string listing available skills, or empty string if none
        """
        skills = self.discover_skills()
        if not skills:
            return ""

        lines = ["## Available Skills", ""]
        lines.append(
            "Use `invoke_skill` to load skill content into conversation context."
        )
        lines.append("")

        for skill in sorted(skills, key=lambda s: (s.namespace, s.name)):
            if skill.namespace == "default":
                lines.append(f"- **{skill.name}**: {skill.description}")
            else:
                lines.append(
                    f"- **{skill.namespace}:{skill.name}**: {skill.description}"
                )

        return "\n".join(lines)

    def get_skill_names(self) -> list[str]:
        """Get list of all available skill names.

        Returns:
            List of skill names (with namespace prefix if not default)
        """
        if not self._metadata_cache:
            self.discover_skills()

        names = []
        for full_name, metadata in self._metadata_cache.items():
            if metadata.namespace == "default":
                names.append(metadata.name)
            else:
                names.append(full_name)
        return names

    def clear_cache(self) -> None:
        """Clear all caches. Useful for reloading skills."""
        self._cache.clear()
        self._metadata_cache.clear()
