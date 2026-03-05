"""Tests for built-in skills infrastructure."""

from __future__ import annotations

import textwrap
from pathlib import Path

from swecli.core.paths import get_paths
from swecli.core.skills import SkillLoader

SAMPLE_SKILL = textwrap.dedent("""\
    ---
    name: test-skill
    description: A test skill for unit tests
    ---

    # Test Skill

    This is a test skill used in unit tests.
    It has enough content to verify loading works correctly.
""")


class TestBuiltinSkillsDir:
    """Test that Paths.builtin_skills_dir resolves correctly."""

    def test_builtin_skills_dir_resolves(self):
        """builtin_skills_dir should point to swecli/skills/builtin/."""
        paths = get_paths()
        expected = Path(__file__).parent.parent / "swecli" / "skills" / "builtin"
        assert paths.builtin_skills_dir.resolve() == expected.resolve()

    def test_builtin_skills_dir_exists(self):
        """The builtin skills directory should exist in the package."""
        paths = get_paths()
        assert paths.builtin_skills_dir.exists()


class TestGetSkillDirsIncludesBuiltin:
    """Test that get_skill_dirs() includes builtin dir at the end."""

    def test_paths_get_skill_dirs_includes_builtin(self):
        """Paths.get_skill_dirs() should include builtin dir as last entry."""
        paths = get_paths()
        dirs = paths.get_skill_dirs()
        assert len(dirs) >= 1
        assert dirs[-1] == paths.builtin_skills_dir

    def test_builtin_is_lowest_priority(self):
        """Builtin dir should be at end of list (lowest priority)."""
        paths = get_paths()
        dirs = paths.get_skill_dirs()
        # builtin should always be last
        assert dirs[-1] == paths.builtin_skills_dir


class TestSkillPriorityOverride:
    """Test that project/user skills override builtin skills."""

    def test_project_skill_overrides_builtin(self, tmp_path: Path):
        """A project skill with the same name should override builtin."""
        # Create a "builtin" skill
        builtin_dir = tmp_path / "builtin"
        builtin_dir.mkdir()
        builtin_skill = builtin_dir / "test-skill.md"
        builtin_skill.write_text(SAMPLE_SKILL, encoding="utf-8")

        # Create a project skill that overrides
        project_dir = tmp_path / "project"
        project_dir.mkdir()
        override_file = project_dir / "test-skill.md"
        override_file.write_text(
            textwrap.dedent("""\
                ---
                name: test-skill
                description: Custom project override
                ---

                # Custom Test Skill
                This is a project-level override.
            """),
            encoding="utf-8",
        )

        # Project dir first (highest priority), builtin last (lowest)
        loader = SkillLoader([project_dir, builtin_dir])
        skills = loader.discover_skills()

        matching = [s for s in skills if s.name == "test-skill"]
        assert len(matching) == 1
        # Should be the project override, not builtin
        assert matching[0].source == "project"
        assert matching[0].description == "Custom project override"

    def test_builtin_used_when_no_override(self, tmp_path: Path):
        """Without an override, the skill from the directory should be used."""
        skill_dir = tmp_path / "skills"
        skill_dir.mkdir()
        skill_file = skill_dir / "test-skill.md"
        skill_file.write_text(SAMPLE_SKILL, encoding="utf-8")

        empty_dir = tmp_path / "empty"
        empty_dir.mkdir()

        loader = SkillLoader([empty_dir, skill_dir])
        skills = loader.discover_skills()

        matching = [s for s in skills if s.name == "test-skill"]
        assert len(matching) == 1


class TestInvokeSkillDedup:
    """Test that invoke_skill dedup guard prevents infinite re-invocation."""

    def test_second_invoke_returns_already_loaded(self, tmp_path: Path):
        """Second invoke of same skill should return short dedup message."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        # Create a test skill
        skill_file = tmp_path / "test-skill.md"
        skill_file.write_text(SAMPLE_SKILL, encoding="utf-8")

        registry = ToolRegistry()
        loader = SkillLoader([tmp_path])
        registry.set_skill_loader(loader)

        # First invocation — full content
        result1 = registry._handle_invoke_skill({"skill_name": "test-skill"})
        assert result1["success"]
        assert "Test Skill" in result1["output"]

        # Second invocation — dedup message
        result2 = registry._handle_invoke_skill({"skill_name": "test-skill"})
        assert result2["success"]
        assert "already loaded" in result2["output"]
        assert "# Test Skill" not in result2["output"]

    def test_different_skills_not_deduped(self, tmp_path: Path):
        """Different skill names should not trigger dedup."""
        from swecli.core.context_engineering.tools.registry import ToolRegistry

        # Create a test skill
        skill_file = tmp_path / "test-skill.md"
        skill_file.write_text(SAMPLE_SKILL, encoding="utf-8")

        registry = ToolRegistry()
        loader = SkillLoader([tmp_path])
        registry.set_skill_loader(loader)

        result1 = registry._handle_invoke_skill({"skill_name": "test-skill"})
        assert "Test Skill" in result1["output"]

        # A different (non-existent) skill should NOT be blocked by dedup
        result2 = registry._handle_invoke_skill({"skill_name": "other-skill"})
        assert not result2["success"]  # Not found, not dedup
