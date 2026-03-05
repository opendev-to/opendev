"""Skill creator controller for the Textual chat app."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Optional, TYPE_CHECKING

from rich.console import RenderableType
from rich.text import Text

from swecli.ui_textual.components.skill_creator_panels import (
    create_location_panel,
    create_method_panel,
    create_identifier_input_panel,
    create_purpose_input_panel,
    create_description_input_panel,
    create_generating_panel,
    create_success_panel,
)
from swecli.ui_textual.managers.spinner_service import SpinnerType

if TYPE_CHECKING:
    from swecli.ui_textual.chat_app import SWECLIChatApp
    from swecli.ui_textual.managers.interrupt_manager import InterruptManager


# Default template for new skills
SKILL_TEMPLATE = """---
name: {name}
description: "{description}"
---

# {human_name}

## Overview
{overview}

## When to Use This Skill
{when_to_use}

## Instructions

### Step 1: Assess the Situation
Evaluate the current context and identify what needs to be done.

### Step 2: Apply the Technique
{instructions}

### Step 3: Verify Results
Confirm that the desired outcome was achieved.

## Examples

### Example 1: Basic Usage
**Situation:** [Describe a typical scenario]
**Approach:**
```
[Add example code or steps]
```

## Common Mistakes
- Not verifying prerequisites before starting
- Skipping error handling
- [Add more specific mistakes to avoid]

## Related Skills
- [List related skills here]
"""


class SkillCreatorController:
    """Encapsulates the skill creation wizard flow rendered inside the conversation log."""

    # Wizard stages
    STAGE_LOCATION = "location"
    STAGE_METHOD = "method"
    STAGE_IDENTIFIER = "identifier"
    STAGE_PURPOSE = "purpose"  # For manual path
    STAGE_DESCRIPTION = "description"  # For AI generation
    STAGE_GENERATING = "generating"

    def __init__(
        self,
        app: "SWECLIChatApp",
        interrupt_manager: Optional["InterruptManager"] = None,
    ) -> None:
        self.app = app
        self._interrupt_manager = interrupt_manager
        self.state: dict[str, Any] | None = None
        self._config_manager: Any = None
        self._on_complete: Any = None

    @property
    def active(self) -> bool:
        return self.state is not None

    def adjust_indices(self, delta: int, first_affected: int) -> None:
        """Adjust panel_start after resize.

        Args:
            delta: Number of lines added (positive) or removed (negative)
            first_affected: First line index affected by the change
        """
        if self.state is None:
            return
        start = self.state.get("panel_start")
        if start is not None and start >= first_affected:
            self.state["panel_start"] = start + delta

    def set_config_manager(self, config_manager: Any) -> None:
        """Set the config manager for path resolution."""
        self._config_manager = config_manager

    def set_on_complete(self, callback: Any) -> None:
        """Set callback to invoke when wizard completes."""
        self._on_complete = callback

    async def start(self) -> None:
        """Begin the skill creation wizard flow."""
        if self.active:
            self.app.conversation.add_system_message(
                "Skill wizard already open — finish or press Esc to cancel."
            )
            self.app.refresh()
            return

        self.state = {
            "stage": self.STAGE_LOCATION,
            "selected_index": 0,
            "location": None,  # "project" or "personal"
            "method": None,  # "generate" or "manual"
            "skill_name": "",
            "purpose": "",  # For manual path
            "description": "",  # For AI generation
            "input_value": "",
            "input_error": "",
            "panel_start": None,
        }

        # Track state for interrupt handling
        if self._interrupt_manager:
            from swecli.ui_textual.managers.interrupt_manager import InterruptState

            self._interrupt_manager.enter_state(
                InterruptState.SKILL_WIZARD,
                controller_ref=self,
            )

        # Clear input field and focus
        input_field = self.app.input_field
        input_field.load_text("")
        input_field.cursor_position = 0
        input_field.focus()

        self._render_current_panel()

    def end(self, message: str | None = None, *, clear_panel: bool = False) -> None:
        """Reset wizard state and optionally emit a message."""
        # Exit state tracking
        if self._interrupt_manager:
            self._interrupt_manager.exit_state()

        state = self.state
        if clear_panel and state:
            start = state.get("panel_start")
            if start is not None:
                self.app.conversation._truncate_from(start)
        self.state = None
        if message:
            self.app.conversation.add_system_message(message)
        self.app.refresh()

    def move(self, delta: int) -> None:
        """Handle up/down navigation in selection panels."""
        state = self.state
        if not state:
            return

        stage = state.get("stage")
        if stage == self.STAGE_LOCATION:
            # Location has 2 options (0 or 1)
            current = state.get("selected_index", 0)
            new_index = (current + delta) % 2
            state["selected_index"] = new_index
            self._render_current_panel()
        elif stage == self.STAGE_METHOD:
            # Method has 3 options (0, 1, or 2 for Back)
            current = state.get("selected_index", 0)
            new_index = (current + delta) % 3
            state["selected_index"] = new_index
            self._render_current_panel()

    def cancel(self) -> None:
        """Cancel the wizard."""
        if not self.state:
            return
        self.end("Skill creation cancelled.", clear_panel=True)

    def back(self) -> None:
        """Go back to the previous step in the wizard."""
        state = self.state
        if not state:
            return

        stage = state.get("stage")

        if stage == self.STAGE_METHOD:
            # Go back to location selection
            state["stage"] = self.STAGE_LOCATION
            state["selected_index"] = 0
            self._render_current_panel()
            return

        if stage == self.STAGE_IDENTIFIER:
            # Go back to method selection
            state["stage"] = self.STAGE_METHOD
            state["selected_index"] = 1  # Manual was selected
            state["input_value"] = ""
            state["input_error"] = ""
            self._render_current_panel()
            return

        if stage == self.STAGE_PURPOSE:
            # Go back to identifier input
            state["stage"] = self.STAGE_IDENTIFIER
            state["input_value"] = state.get("skill_name", "")
            state["input_error"] = ""
            self._render_current_panel()
            return

        if stage == self.STAGE_DESCRIPTION:
            # Go back to method selection
            state["stage"] = self.STAGE_METHOD
            state["selected_index"] = 0  # Generate was selected
            state["input_value"] = ""
            state["input_error"] = ""
            self._render_current_panel()
            return

        # For STAGE_LOCATION or STAGE_GENERATING, just cancel
        if stage == self.STAGE_LOCATION:
            self.cancel()

    async def confirm(self) -> None:
        """Handle Enter key press - confirm selection or submit input."""
        state = self.state
        if not state:
            return

        stage = state.get("stage")

        if stage == self.STAGE_LOCATION:
            # Save location choice and move to method selection
            state["location"] = "project" if state.get("selected_index", 0) == 0 else "personal"
            state["stage"] = self.STAGE_METHOD
            state["selected_index"] = 0  # Reset selection for next panel
            self._render_current_panel()
            return

        if stage == self.STAGE_METHOD:
            selected = state.get("selected_index", 0)
            if selected == 2:  # Back option
                self.back()
                return
            # Save method choice and move to appropriate input stage
            state["method"] = "generate" if selected == 0 else "manual"
            if state["method"] == "generate":
                state["stage"] = self.STAGE_DESCRIPTION
            else:
                state["stage"] = self.STAGE_IDENTIFIER
            state["input_value"] = ""
            state["input_error"] = ""
            self._render_current_panel()
            return

        if stage == self.STAGE_IDENTIFIER:
            # Validate and save identifier
            name = state.get("input_value", "").strip().replace(" ", "-").lower()
            if not name:
                state["input_error"] = "Skill name is required"
                self._render_current_panel()
                return

            # Check for invalid characters
            if not all(c.isalnum() or c == "-" for c in name):
                state["input_error"] = "Use only letters, numbers, and hyphens"
                self._render_current_panel()
                return

            state["skill_name"] = name
            state["input_value"] = ""
            state["input_error"] = ""
            state["stage"] = self.STAGE_PURPOSE
            self._render_current_panel()
            return

        if stage == self.STAGE_PURPOSE:
            # Save purpose and create skill
            purpose = state.get("input_value", "").strip()
            if not purpose:
                state["input_error"] = "Purpose is required"
                self._render_current_panel()
                return

            state["purpose"] = purpose
            await self._create_skill_manual()
            return

        if stage == self.STAGE_DESCRIPTION:
            # Save description and generate with AI
            desc = state.get("input_value", "").strip()
            if not desc:
                state["input_error"] = "Description is required"
                self._render_current_panel()
                return

            state["description"] = desc
            await self._create_skill_generate()
            return

    async def handle_input(self, raw_value: str) -> bool:
        """Handle text input submission.

        Returns True if input was consumed by the wizard.
        """
        state = self.state
        if not state:
            return False

        stage = state.get("stage")

        # Selection stages - handle number input
        if stage == self.STAGE_LOCATION:
            value = raw_value.strip()
            if value == "1":
                state["selected_index"] = 0
                await self.confirm()
                return True
            elif value == "2":
                state["selected_index"] = 1
                await self.confirm()
                return True
            return True  # Consume input even if invalid

        if stage == self.STAGE_METHOD:
            normalized = raw_value.strip().lower()
            if normalized in {"b", "back"}:
                self.back()
                return True
            if normalized == "1":
                state["selected_index"] = 0
                await self.confirm()
                return True
            elif normalized == "2":
                state["selected_index"] = 1
                await self.confirm()
                return True
            return True

        # Text input stages
        if stage in (self.STAGE_IDENTIFIER, self.STAGE_PURPOSE, self.STAGE_DESCRIPTION):
            state["input_value"] = raw_value.strip()
            await self.confirm()
            return True

        return False

    def update_input_preview(self, text: str) -> None:
        """Update the panel to show current input text (live preview)."""
        state = self.state
        if not state:
            return

        stage = state.get("stage")
        if stage in (self.STAGE_IDENTIFIER, self.STAGE_PURPOSE, self.STAGE_DESCRIPTION):
            state["input_value"] = text
            self._render_current_panel()

    def _render_current_panel(self) -> None:
        """Render the appropriate panel for the current stage."""
        state = self.state
        if not state:
            return

        stage = state.get("stage")
        working_dir = (
            getattr(self._config_manager, "working_dir", "") if self._config_manager else ""
        )

        if stage == self.STAGE_LOCATION:
            panel = create_location_panel(state.get("selected_index", 0), working_dir=working_dir)
        elif stage == self.STAGE_METHOD:
            panel = create_method_panel(state.get("selected_index", 0))
        elif stage == self.STAGE_IDENTIFIER:
            panel = create_identifier_input_panel(
                state.get("input_value", ""), state.get("input_error", "")
            )
        elif stage == self.STAGE_PURPOSE:
            panel = create_purpose_input_panel(state.get("input_value", ""))
        elif stage == self.STAGE_DESCRIPTION:
            panel = create_description_input_panel(state.get("input_value", ""))
        else:
            # STAGE_GENERATING is handled by spinner in _create_skill_generate()
            return

        self._post_panel(panel)

    def _post_panel(self, panel: RenderableType) -> None:
        """Post Rich panel to conversation, replacing previous panel if exists."""
        state = self.state
        if state is not None:
            start = state.get("panel_start")
            conversation = self.app.conversation
            if start is None or start > len(conversation.lines):
                state["panel_start"] = len(conversation.lines)
            else:
                conversation._truncate_from(start)

        # Write Rich renderable directly
        self.app.conversation.write(panel)
        self.app.conversation.scroll_end(animate=False)
        self.app.refresh()

    def _get_skills_dir(self) -> Path:
        """Get the appropriate skills directory based on location choice."""
        from swecli.core.paths import get_paths

        state = self.state
        if not state:
            raise ValueError("No wizard state")

        if self._config_manager:
            paths = get_paths(self._config_manager.working_dir)
        else:
            paths = get_paths(None)

        if state.get("location") == "project":
            return paths.project_skills_dir
        else:
            return paths.global_skills_dir

    async def _create_skill_manual(self) -> None:
        """Create skill with manual configuration."""
        state = self.state
        if not state:
            return

        name = state.get("skill_name", "")
        purpose = state.get("purpose", "")

        try:
            skills_dir = self._get_skills_dir()
            skill_dir = skills_dir / name
            skill_dir.mkdir(parents=True, exist_ok=True)

            skill_file = skill_dir / "SKILL.md"

            # Generate human-readable name
            human_name = name.replace("-", " ").title()

            # Generate description from purpose
            description = f"Use when {purpose.lower().rstrip('.')}."

            content = SKILL_TEMPLATE.format(
                name=name,
                description=description,
                human_name=human_name,
                overview=f"This skill provides guidance for {purpose.lower().rstrip('.')}.",
                when_to_use=f"- When you need to {purpose.lower().rstrip('.')}\n- When facing challenges related to {name.replace('-', ' ')}",
                instructions="[Add specific steps and guidance here]",
            )

            skill_file.write_text(content, encoding="utf-8")

            # Show success panel
            success_panel = create_success_panel(name, str(skill_dir))
            self._post_panel(success_panel)

            # Clear state but keep panel visible
            self.state = None

            if self._on_complete:
                self._on_complete(name, str(skill_dir))

        except Exception as e:
            self.end(f"Failed to create skill: {e}", clear_panel=True)

    async def _create_skill_generate(self) -> None:
        """Create skill using AI generation with in-panel spinner."""
        import asyncio

        state = self.state
        if not state:
            return

        # Set stage to GENERATING so keyboard handler allows normal input
        state["stage"] = self.STAGE_GENERATING

        # Set processing state so messages get queued during generation
        self.app._set_processing_state(True)

        # Pause message processor so queued messages wait until generation completes
        runner = getattr(self.app, "_runner", None)
        if runner:
            runner.pause_processing()

        description = state.get("description", "")

        # Get spinner service from app
        spinner_service = getattr(self.app, "spinner_service", None)
        spinner_id = None

        try:
            if spinner_service:
                # Callback to update panel with current spinner frame
                def update_generating_panel(frame) -> None:
                    panel = create_generating_panel(
                        description=description,
                        spinner_char=frame.char,
                        elapsed_seconds=frame.elapsed_seconds,
                    )
                    self._post_panel(panel)

                # Register with callback API for in-panel animation
                spinner_id = spinner_service.register(
                    spinner_type=SpinnerType.TOOL,
                    render_callback=update_generating_panel,
                )

                # Render initial panel immediately
                panel = create_generating_panel(
                    description=description,
                    spinner_char="⠋",
                    elapsed_seconds=0,
                )
                self._post_panel(panel)

            # Get config and create HTTP client
            if not self._config_manager:
                raise ValueError("Config manager not set")

            config = self._config_manager.get_config()

            from swecli.core.agents.components import (
                create_http_client,
                build_max_tokens_param,
                build_temperature_param,
            )

            http_client = create_http_client(config)

            # Load system prompt for skill generation
            prompt_path = (
                Path(__file__).parent.parent.parent
                / "core/agents/prompts/templates/generators/skill_generator_prompt.txt"
            )
            generator_system_prompt = prompt_path.read_text(encoding="utf-8")

            # Build messages
            messages = [
                {"role": "system", "content": generator_system_prompt},
                {"role": "user", "content": f"Create a skill for: {description}"},
            ]

            # Build payload (no tools needed for generation)
            payload = {
                "model": config.model,
                "messages": messages,
                **build_max_tokens_param(config.model, 4000),
                **build_temperature_param(config.model, 0.7),
            }

            # Run blocking HTTP call in background thread (non-blocking!)
            result = await asyncio.to_thread(
                http_client.post_json,
                payload,
                task_monitor=None,
            )

            if result.success and result.response and result.response.status_code == 200:
                response_data = result.response.json()
                content = response_data["choices"][0]["message"]["content"]

                # Stop spinner
                if spinner_service and spinner_id:
                    spinner_service.stop(spinner_id)

                # Parse the response to extract name and save skill
                name, skill_content = self._parse_generated_skill(content, description)

                # Save skill to directory
                skills_dir = self._get_skills_dir()
                skill_dir = skills_dir / name
                skill_dir.mkdir(parents=True, exist_ok=True)

                skill_file = skill_dir / "SKILL.md"
                skill_file.write_text(skill_content, encoding="utf-8")

                # Resume processing before showing success
                if runner:
                    runner.resume_processing()

                # Reset processing state if queue is empty
                queue_size = runner.get_queue_size() if runner else 0
                if queue_size == 0:
                    self.app.notify_processing_complete()

                # Show success panel
                success_panel = create_success_panel(name, str(skill_dir))
                self._post_panel(success_panel)

                # Clear state but keep panel visible
                self.state = None

                if self._on_complete:
                    self._on_complete(name, str(skill_dir))
                return
            else:
                # LLM call failed
                error_msg = result.error if result.error else "Unknown error"
                if spinner_service and spinner_id:
                    spinner_service.stop(spinner_id)

                # Resume processing
                if runner:
                    runner.resume_processing()

                queue_size = runner.get_queue_size() if runner else 0
                if queue_size == 0:
                    self.app.notify_processing_complete()

                await self._create_skill_fallback(description, error_msg)
                return

        except Exception as e:
            if spinner_service and spinner_id:
                spinner_service.stop(spinner_id)

            # Resume processing on error
            if runner:
                runner.resume_processing()

            queue_size = runner.get_queue_size() if runner else 0
            if queue_size == 0:
                self.app.notify_processing_complete()

            self.end(f"Failed to create skill: {e}", clear_panel=True)

    def _parse_generated_skill(self, content: str, description: str) -> tuple[str, str]:
        """Parse LLM-generated skill content and extract name.

        Returns:
            Tuple of (skill_name, full_content)
        """
        import re

        content = content.strip()

        # Remove markdown code block wrapper if present
        if content.startswith("```"):
            # Find first newline after opening backticks
            first_newline = content.find("\n")
            if first_newline != -1:
                content = content[first_newline + 1 :]
            # Remove closing backticks
            if content.rstrip().endswith("```"):
                content = content.rstrip()[:-3].rstrip()

        # Extract name from YAML frontmatter
        name = "custom-skill"
        frontmatter_match = re.match(r"^---\s*\n(.*?)\n---", content, re.DOTALL)
        if frontmatter_match:
            frontmatter = frontmatter_match.group(1)
            name_match = re.search(r"^name:\s*(.+)$", frontmatter, re.MULTILINE)
            if name_match:
                name = name_match.group(1).strip().strip('"').strip("'")

        # Clean the name to ensure it's valid
        name = "".join(c if c.isalnum() or c == "-" else "-" for c in name.lower())
        name = "-".join(filter(None, name.split("-")))[:30]  # Max 30 chars

        if not name:
            name = "custom-skill"

        return name, content

    async def _create_skill_fallback(self, description: str, error_msg: str) -> None:
        """Create skill with basic template when LLM generation fails."""
        state = self.state
        if not state:
            return

        # Extract a name from the description
        words = description.lower().split()
        name_candidates = []
        for word in words:
            if len(word) > 3 and word.isalpha():
                name_candidates.append(word)

        if name_candidates:
            name = "-".join(name_candidates[:2])
        else:
            name = "custom-skill"

        # Clean the name
        name = "".join(c if c.isalnum() or c == "-" else "-" for c in name)
        name = "-".join(filter(None, name.split("-")))[:30]

        # Generate human-readable name
        human_name = name.replace("-", " ").title()

        content = SKILL_TEMPLATE.format(
            name=name,
            description=f"Use when {description.lower().rstrip('.')}.",
            human_name=human_name,
            overview=f"This skill provides guidance for {description.lower().rstrip('.')}.",
            when_to_use=f"- When you need to {description.lower().rstrip('.')}\n- When facing challenges related to {name.replace('-', ' ')}",
            instructions="[Add specific steps and guidance here]",
        )

        try:
            skills_dir = self._get_skills_dir()
            skill_dir = skills_dir / name
            skill_dir.mkdir(parents=True, exist_ok=True)

            skill_file = skill_dir / "SKILL.md"
            skill_file.write_text(content, encoding="utf-8")

            # Show success panel (note: we fell back to template)
            success_panel = create_success_panel(name, str(skill_dir))
            self._post_panel(success_panel)

            # Clear state
            self.state = None

            if self._on_complete:
                self._on_complete(name, str(skill_dir))

        except Exception as e:
            self.end(f"Failed to create skill: {e}", clear_panel=True)


__all__ = ["SkillCreatorController"]
