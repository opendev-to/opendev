"""Tests for /skills command handler."""

import tempfile
import shutil
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from rich.console import Console

from swecli.repl.commands.skills_commands import SkillsCommands, load_skill_generator_prompt


class TestLoadSkillGeneratorPrompt:
    """Tests for skill generator prompt loading."""

    def test_load_skill_generator_prompt_returns_content(self):
        """Test that the prompt file can be loaded."""
        content = load_skill_generator_prompt()
        assert content  # Not empty
        assert "Claude Code skills" in content
        assert "TDD Mapping" in content


class TestSkillsCommandsInit:
    """Tests for SkillsCommands initialization."""

    def test_init_creates_handler(self):
        """Test handler initialization."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()
        config_manager.working_dir = Path.cwd()

        handler = SkillsCommands(console, config_manager)

        assert handler.console == console
        assert handler.config_manager == config_manager


class TestSkillsCommandsMenu:
    """Tests for /skills menu display."""

    def test_show_menu_returns_success(self):
        """Test that show menu returns success."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()
        config_manager.working_dir = Path.cwd()

        handler = SkillsCommands(console, config_manager)
        result = handler.handle("")

        assert result.success is True

    def test_unknown_subcommand_shows_menu(self):
        """Test that unknown subcommand shows menu."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()
        config_manager.working_dir = Path.cwd()

        handler = SkillsCommands(console, config_manager)
        result = handler.handle("unknown")

        assert result.success is True


class TestSkillsCommandsList:
    """Tests for /skills list."""

    def test_list_skills_empty(self):
        """Test listing when no skills exist."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()

        # Use temp dir that doesn't have skills
        with tempfile.TemporaryDirectory() as tmpdir:
            config_manager.working_dir = Path(tmpdir)
            handler = SkillsCommands(console, config_manager)
            result = handler.handle("list")

            assert result.success is True

    def test_list_skills_with_skill(self):
        """Test listing when skills exist."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()

        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir_path = Path(tmpdir)
            config_manager.working_dir = tmpdir_path

            # Create a test skill
            skill_dir = tmpdir_path / ".opendev" / "skills" / "test-skill"
            skill_dir.mkdir(parents=True)
            skill_file = skill_dir / "SKILL.md"
            skill_file.write_text('''---
name: test-skill
description: "Use when testing skill commands."
---

# Test Skill

Instructions here.
''')

            handler = SkillsCommands(console, config_manager)
            result = handler.handle("list")

            assert result.success is True


class TestSkillsCommandsCreate:
    """Tests for /skills create."""

    @patch("swecli.repl.commands.skills_commands.Prompt.ask")
    @patch("swecli.repl.commands.skills_commands.Confirm.ask")
    def test_create_skill(self, mock_confirm, mock_prompt):
        """Test creating a skill."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()

        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir_path = Path(tmpdir)
            config_manager.working_dir = tmpdir_path

            # Mock user inputs: location=personal(1), name, purpose
            mock_prompt.side_effect = ["1", "my-test-skill", "testing skill creation"]

            handler = SkillsCommands(console, config_manager)

            # Need to mock the global skills dir
            with patch("swecli.repl.commands.skills_commands.get_paths") as mock_paths:
                paths = MagicMock()
                paths.global_skills_dir = tmpdir_path / "global_skills"
                paths.project_skills_dir = tmpdir_path / "project_skills"
                mock_paths.return_value = paths

                result = handler.handle("create")

            # Check skill was created
            assert result.success is True
            assert result.data["name"] == "my-test-skill"

            # Verify skill directory exists
            skill_dir = tmpdir_path / "global_skills" / "my-test-skill"
            assert skill_dir.exists()

            # Verify SKILL.md exists and has content
            skill_file = skill_dir / "SKILL.md"
            assert skill_file.exists()
            content = skill_file.read_text()
            assert "name: my-test-skill" in content
            assert "testing skill creation" in content


class TestSkillsCommandsParseMetadata:
    """Tests for SKILL.md metadata parsing."""

    def test_parse_skill_metadata(self):
        """Test parsing YAML frontmatter."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()
        config_manager.working_dir = Path.cwd()

        handler = SkillsCommands(console, config_manager)

        with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
            f.write('''---
name: test-skill
description: "Use when testing."
---

# Test Skill
''')
            f.flush()

            name, description = handler._parse_skill_metadata(Path(f.name))

            assert name == "test-skill"
            assert description == "Use when testing."

            # Cleanup
            Path(f.name).unlink()

    def test_parse_skill_metadata_missing_frontmatter(self):
        """Test parsing file without frontmatter."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()
        config_manager.working_dir = Path.cwd()

        handler = SkillsCommands(console, config_manager)

        with tempfile.NamedTemporaryFile(mode="w", suffix=".md", delete=False) as f:
            f.write("# Test Skill\n\nNo frontmatter here.")
            f.flush()

            name, description = handler._parse_skill_metadata(Path(f.name))

            assert name == ""
            assert description == ""

            # Cleanup
            Path(f.name).unlink()


class TestSkillsCommandsDelete:
    """Tests for /skills delete."""

    @patch("swecli.repl.commands.skills_commands.Confirm.ask")
    def test_delete_skill(self, mock_confirm):
        """Test deleting a skill."""
        mock_confirm.return_value = True

        console = Console(force_terminal=True)
        config_manager = MagicMock()

        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir_path = Path(tmpdir)
            config_manager.working_dir = tmpdir_path

            # Create a test skill
            skill_dir = tmpdir_path / ".opendev" / "skills" / "delete-me"
            skill_dir.mkdir(parents=True)
            (skill_dir / "SKILL.md").write_text("---\nname: delete-me\n---\n")

            handler = SkillsCommands(console, config_manager)
            result = handler.handle("delete delete-me")

            assert result.success is True
            assert not skill_dir.exists()

    def test_delete_nonexistent_skill(self):
        """Test deleting a skill that doesn't exist."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()

        with tempfile.TemporaryDirectory() as tmpdir:
            config_manager.working_dir = Path(tmpdir)

            handler = SkillsCommands(console, config_manager)
            result = handler.handle("delete nonexistent")

            assert result.success is False


class TestSkillsCommandsFindSkill:
    """Tests for finding skill directories."""

    def test_find_skill_dir_project(self):
        """Test finding skill in project directory."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()

        with tempfile.TemporaryDirectory() as tmpdir:
            tmpdir_path = Path(tmpdir)
            config_manager.working_dir = tmpdir_path

            # Create project skill
            skill_dir = tmpdir_path / ".opendev" / "skills" / "project-skill"
            skill_dir.mkdir(parents=True)
            (skill_dir / "SKILL.md").write_text("---\nname: project-skill\n---\n")

            handler = SkillsCommands(console, config_manager)
            found = handler._find_skill_dir("project-skill")

            assert found == skill_dir

    def test_find_skill_dir_not_found(self):
        """Test finding skill that doesn't exist."""
        console = Console(force_terminal=True)
        config_manager = MagicMock()

        with tempfile.TemporaryDirectory() as tmpdir:
            config_manager.working_dir = Path(tmpdir)

            handler = SkillsCommands(console, config_manager)
            found = handler._find_skill_dir("nonexistent")

            assert found is None
