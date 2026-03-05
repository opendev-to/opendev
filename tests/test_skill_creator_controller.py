"""Tests for SkillCreatorController LLM generation."""

import pytest
from unittest.mock import MagicMock, AsyncMock, patch
from pathlib import Path
import tempfile


class TestParseGeneratedSkill:
    """Test the _parse_generated_skill method."""

    def setup_method(self):
        """Create a controller instance for testing."""
        from swecli.ui_textual.controllers.skill_creator_controller import (
            SkillCreatorController,
        )

        mock_app = MagicMock()
        self.controller = SkillCreatorController(mock_app)

    def test_parse_simple_skill(self):
        """Test parsing a simple skill definition."""
        content = """---
name: test-skill
description: "Use when testing"
---

# Test Skill

You are a test skill.
"""
        name, parsed_content = self.controller._parse_generated_skill(content, "test")
        assert name == "test-skill"
        assert "# Test Skill" in parsed_content

    def test_parse_skill_with_code_block(self):
        """Test parsing skill wrapped in markdown code block."""
        content = """```markdown
---
name: code-review-skill
description: "Use when reviewing code"
---

# Code Review Skill

Reviews code.
```"""
        name, parsed_content = self.controller._parse_generated_skill(content, "test")
        assert name == "code-review-skill"
        assert "Reviews code." in parsed_content
        assert "```" not in parsed_content

    def test_parse_skill_name_cleanup(self):
        """Test that skill names are cleaned up properly."""
        content = """---
name: "Test Skill With Spaces"
description: "A test"
---

Content here.
"""
        name, parsed_content = self.controller._parse_generated_skill(content, "test")
        assert name == "test-skill-with-spaces"
        assert "-" in name
        assert " " not in name

    def test_parse_skill_no_name(self):
        """Test fallback when no name is found."""
        content = """---
description: "No name field"
---

Content here.
"""
        name, parsed_content = self.controller._parse_generated_skill(content, "test")
        assert name == "custom-skill"

    def test_parse_skill_name_too_long(self):
        """Test that long names are truncated."""
        content = """---
name: this-is-a-very-very-very-very-long-skill-name-that-should-be-truncated
description: "A test"
---

Content here.
"""
        name, parsed_content = self.controller._parse_generated_skill(content, "test")
        assert len(name) <= 30


class TestCreateSkillFallback:
    """Test the _create_skill_fallback method."""

    def setup_method(self):
        """Create a controller instance for testing."""
        from swecli.ui_textual.controllers.skill_creator_controller import (
            SkillCreatorController,
        )

        mock_app = MagicMock()
        mock_app.conversation = MagicMock()
        mock_app.conversation.lines = []
        mock_app.conversation.scroll_end = MagicMock()
        mock_app.conversation._truncate_from = MagicMock()
        mock_app.conversation.write = MagicMock()
        mock_app.refresh = MagicMock()

        self.controller = SkillCreatorController(mock_app)

    def test_fallback_creates_skill_directory(self):
        """Test that fallback creates a skill directory with SKILL.md."""
        import asyncio

        with tempfile.TemporaryDirectory() as tmpdir:
            self.controller._get_skills_dir = MagicMock(return_value=Path(tmpdir))
            self.controller.state = {"panel_start": 0}

            asyncio.run(
                self.controller._create_skill_fallback("A skill for debugging Python code", "Error")
            )

            # Verify skill directory was created
            skill_dirs = [d for d in Path(tmpdir).iterdir() if d.is_dir()]
            assert len(skill_dirs) == 1

            # Verify SKILL.md exists
            skill_file = skill_dirs[0] / "SKILL.md"
            assert skill_file.exists()

            content = skill_file.read_text()
            assert "debug" in content.lower()
            assert "---" in content  # Has frontmatter
            assert "name:" in content
            assert "description:" in content

    def test_fallback_name_extraction(self):
        """Test that fallback extracts meaningful name from description."""
        import asyncio

        with tempfile.TemporaryDirectory() as tmpdir:
            self.controller._get_skills_dir = MagicMock(return_value=Path(tmpdir))
            self.controller.state = {"panel_start": 0}

            asyncio.run(
                self.controller._create_skill_fallback(
                    "A skill for testing Python applications", "Error"
                )
            )

            skill_dirs = [d for d in Path(tmpdir).iterdir() if d.is_dir()]
            assert len(skill_dirs) == 1
            # Name should contain meaningful words from description
            dirname = skill_dirs[0].name
            assert "testing" in dirname or "python" in dirname


class TestSkillGeneratorPromptExists:
    """Test that the skill generator prompt file exists and is valid."""

    def test_prompt_file_exists(self):
        """Test that the skill generator prompt file exists."""
        prompt_path = (
            Path(__file__).parent.parent
            / "swecli/core/agents/prompts/templates/generators/skill_generator_prompt.txt"
        )
        assert prompt_path.exists(), f"Prompt file not found at {prompt_path}"

    def test_prompt_has_required_content(self):
        """Test that prompt contains key instructions."""
        prompt_path = (
            Path(__file__).parent.parent
            / "swecli/core/agents/prompts/templates/generators/skill_generator_prompt.txt"
        )
        content = prompt_path.read_text()

        # Check for key elements that should be in the prompt
        assert "name:" in content.lower() or "skill" in content.lower()
        assert "description" in content.lower()
        assert "---" in content  # YAML frontmatter instruction


class TestSkillCreatorWizardStates:
    """Test the skill creator wizard state machine."""

    def setup_method(self):
        """Create a controller instance for testing."""
        from swecli.ui_textual.controllers.skill_creator_controller import (
            SkillCreatorController,
        )

        mock_app = MagicMock()
        mock_app.conversation = MagicMock()
        mock_app.conversation.lines = []
        mock_app.conversation.scroll_end = MagicMock()
        mock_app.conversation._truncate_from = MagicMock()
        mock_app.conversation.write = MagicMock()
        mock_app.refresh = MagicMock()
        mock_app.input_field = MagicMock()
        mock_app.input_field.load_text = MagicMock()
        mock_app.input_field.cursor_position = 0
        mock_app.input_field.focus = MagicMock()

        self.controller = SkillCreatorController(mock_app)

    def test_start_wizard(self):
        """Test starting the wizard initializes state correctly."""
        import asyncio

        asyncio.run(self.controller.start())

        assert self.controller.active is True
        assert self.controller.state is not None
        assert self.controller.state["stage"] == "location"
        assert self.controller.state["selected_index"] == 0

    def test_cancel_wizard(self):
        """Test cancelling the wizard clears state."""
        import asyncio

        asyncio.run(self.controller.start())
        assert self.controller.active is True

        self.controller.cancel()
        assert self.controller.active is False
        assert self.controller.state is None

    def test_move_navigation(self):
        """Test up/down navigation in selection panels."""
        import asyncio

        asyncio.run(self.controller.start())

        # Start at index 0
        assert self.controller.state["selected_index"] == 0

        # Move down
        self.controller.move(1)
        assert self.controller.state["selected_index"] == 1

        # Move down again (should wrap to 0)
        self.controller.move(1)
        assert self.controller.state["selected_index"] == 0

        # Move up (should wrap to 1)
        self.controller.move(-1)
        assert self.controller.state["selected_index"] == 1

    def test_back_from_location_cancels(self):
        """Test that going back from location cancels the wizard."""
        import asyncio

        asyncio.run(self.controller.start())
        assert self.controller.state["stage"] == "location"

        self.controller.back()
        assert self.controller.active is False


class TestSkillCreatorPanels:
    """Test the skill creator panel rendering functions."""

    def test_create_location_panel(self):
        """Test location panel creation."""
        from swecli.ui_textual.components.skill_creator_panels import create_location_panel

        panel = create_location_panel(0, working_dir="/test")
        assert panel is not None

    def test_create_method_panel(self):
        """Test method panel creation."""
        from swecli.ui_textual.components.skill_creator_panels import create_method_panel

        panel = create_method_panel(0)
        assert panel is not None

    def test_create_identifier_input_panel(self):
        """Test identifier input panel creation."""
        from swecli.ui_textual.components.skill_creator_panels import (
            create_identifier_input_panel,
        )

        panel = create_identifier_input_panel("test-skill", "")
        assert panel is not None

        # With error
        panel_with_error = create_identifier_input_panel("", "Name required")
        assert panel_with_error is not None

    def test_create_purpose_input_panel(self):
        """Test purpose input panel creation."""
        from swecli.ui_textual.components.skill_creator_panels import (
            create_purpose_input_panel,
        )

        panel = create_purpose_input_panel("Help with testing")
        assert panel is not None

    def test_create_description_input_panel(self):
        """Test description input panel creation."""
        from swecli.ui_textual.components.skill_creator_panels import (
            create_description_input_panel,
        )

        panel = create_description_input_panel("A skill for testing")
        assert panel is not None

    def test_create_generating_panel(self):
        """Test generating panel creation."""
        from swecli.ui_textual.components.skill_creator_panels import create_generating_panel

        panel = create_generating_panel("test description", "⠋", 5)
        assert panel is not None

    def test_create_success_panel(self):
        """Test success panel creation."""
        from swecli.ui_textual.components.skill_creator_panels import create_success_panel

        panel = create_success_panel("test-skill", "/path/to/skill")
        assert panel is not None
