"""Command handler for /skills command to create and manage custom skills."""

import shutil
from pathlib import Path
from typing import Any, Optional

from rich.console import Console
from rich.table import Table
from rich.prompt import Prompt, Confirm
from rich.panel import Panel

from swecli.repl.commands.base import CommandHandler, CommandResult
from swecli.core.paths import get_paths, APP_DIR_NAME


def load_skill_generator_prompt() -> str:
    """Load the skill generator prompt from file.

    Returns:
        Content of skill_generator_prompt.txt
    """
    prompt_path = (
        Path(__file__).parent.parent.parent
        / "core"
        / "agents"
        / "prompts"
        / "templates"
        / "generators"
        / "skill_generator_prompt.txt"
    )
    if prompt_path.exists():
        return prompt_path.read_text(encoding="utf-8")
    return ""


class SkillsCommands(CommandHandler):
    """Handler for /skills command to create and manage custom skills."""

    def __init__(
        self,
        console: Console,
        config_manager: Any,
    ):
        """Initialize skills command handler.

        Args:
            console: Rich console for output
            config_manager: Configuration manager
        """
        super().__init__(console)
        self.config_manager = config_manager

    def handle(self, args: str) -> CommandResult:
        """Handle /skills command and subcommands.

        Args:
            args: Command arguments

        Returns:
            CommandResult with execution status
        """
        if not args:
            return self._show_menu()

        parts = args.split(maxsplit=1)
        subcmd = parts[0].lower()
        subcmd_args = parts[1] if len(parts) > 1 else ""

        if subcmd == "create":
            return self._create_skill(subcmd_args)
        elif subcmd == "list":
            return self._list_skills()
        elif subcmd == "edit":
            return self._edit_skill(subcmd_args)
        elif subcmd == "test":
            return self._test_skill(subcmd_args)
        elif subcmd == "delete":
            return self._delete_skill(subcmd_args)
        else:
            return self._show_menu()

    def _show_menu(self) -> CommandResult:
        """Show available skills commands."""
        self.print_line("[cyan]/skills create[/cyan]  Create a new custom skill with AI assistance")
        self.print_continuation("[cyan]/skills list[/cyan]    List all available skills")
        self.print_continuation("[cyan]/skills edit[/cyan]    Edit an existing skill")
        self.print_continuation("[cyan]/skills test[/cyan]    Test a skill with sample scenario")
        self.print_continuation("[cyan]/skills delete[/cyan]  Delete a skill")
        self.console.print()

        return CommandResult(success=True)

    def _create_skill(self, args: str) -> CommandResult:
        """Create a new custom skill with AI-assisted generation.

        Args:
            args: Optional skill name

        Returns:
            CommandResult with creation status
        """
        try:
            # Ask for location
            paths = get_paths(self.config_manager.working_dir)
            self.print_info("Where should the skill be created?")
            self.console.print(f"  [cyan]1[/cyan]. Personal (~/{APP_DIR_NAME}/skills/)")
            self.console.print(f"  [cyan]2[/cyan]. Project ({APP_DIR_NAME}/skills/)")

            choice = Prompt.ask("Select location", choices=["1", "2"], default="1")
            is_personal = choice == "1"

            if is_personal:
                skills_dir = paths.global_skills_dir
            else:
                skills_dir = paths.project_skills_dir

            # Ask for skill name
            if args:
                name = args.strip().replace(" ", "-").lower()
            else:
                name = Prompt.ask("Skill name (e.g., wait-for-condition)")
                name = name.strip().replace(" ", "-").lower()

            if not name:
                self.print_error("Skill name is required")
                return CommandResult(success=False, message="Skill name required")

            # Check if skill already exists
            skill_dir = skills_dir / name
            if skill_dir.exists():
                if not Confirm.ask(f"Skill '{name}' already exists. Overwrite?"):
                    self.print_info("Cancelled")
                    return CommandResult(success=False, message="Cancelled")
                # Remove existing to replace
                shutil.rmtree(skill_dir)

            # Ask for purpose/description
            self.console.print()
            self.print_info("Describe what this skill should do:")
            self.console.print(
                "  [dim](Be specific about when to use it and what it teaches)[/dim]"
            )
            purpose = Prompt.ask("Purpose")

            if not purpose:
                self.print_error("Purpose description is required")
                return CommandResult(success=False, message="Purpose required")

            # Generate skill with AI assistance
            skill_content = self._generate_skill_with_ai(name, purpose)

            # Create skill directory and SKILL.md
            skill_dir.mkdir(parents=True, exist_ok=True)
            skill_file = skill_dir / "SKILL.md"
            skill_file.write_text(skill_content, encoding="utf-8")

            self.print_success(f"Created skill: {skill_dir}")
            self.console.print(f"  Edit [cyan]{skill_file}[/cyan] to customize the skill.")
            self.console.print()

            # Show preview
            self.console.print(
                Panel(
                    skill_content[:500] + ("..." if len(skill_content) > 500 else ""),
                    title="[bold]Generated SKILL.md Preview[/bold]",
                    border_style="dim",
                )
            )

            return CommandResult(
                success=True,
                message=f"Skill created: {name}",
                data={"path": str(skill_dir), "name": name},
            )

        except Exception as e:
            self.print_error(f"Failed to create skill: {e}")
            return CommandResult(success=False, message=str(e))

    def _generate_skill_with_ai(self, name: str, purpose: str) -> str:
        """Generate skill content using AI.

        Args:
            name: Skill name
            purpose: Purpose description from user

        Returns:
            Generated SKILL.md content
        """
        # For now, generate a template. AI integration can be added later.
        # The skill generator prompt is available for AI-powered generation.
        human_name = name.replace("-", " ").title()

        return f"""---
name: {name}
description: "Use when {purpose.lower().rstrip('.')}."
---

# {human_name}

## Overview
This skill provides guidance for {purpose.lower().rstrip('.')}.

## When to Use This Skill
- When you need to {purpose.lower().rstrip('.')}
- When facing challenges related to {name.replace("-", " ")}

## Instructions

### Step 1: Assess the Situation
Evaluate the current context and identify what needs to be done.

### Step 2: Apply the Technique
[Add specific steps and guidance here]

### Step 3: Verify Results
Confirm that the desired outcome was achieved.

## Examples

### Example 1: Basic Usage
**Situation:** [Describe a typical scenario]
**Approach:**
```
[Add example code or steps]
```

### Example 2: Advanced Usage
**Situation:** [Describe a more complex scenario]
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

    def _list_skills(self) -> CommandResult:
        """List all available skills including plugin skills.

        Returns:
            CommandResult with skill list
        """
        from swecli.core.plugins import PluginManager

        paths = get_paths(self.config_manager.working_dir)

        table = Table(show_header=True, header_style="bold")
        table.add_column("Name", style="cyan")
        table.add_column("Description")
        table.add_column("Source", style="dim")
        table.add_column("Tokens", style="dim", justify="right")

        skill_count = 0

        # Search project skills first, then global
        search_dirs = [
            (paths.project_skills_dir, "project"),
            (paths.global_skills_dir, "personal"),
        ]

        for skills_dir, location in search_dirs:
            if not skills_dir.exists():
                continue

            for skill_dir in sorted(skills_dir.iterdir()):
                if not skill_dir.is_dir():
                    continue

                skill_file = skill_dir / "SKILL.md"
                if not skill_file.exists():
                    continue

                # Parse SKILL.md for metadata
                name, description = self._parse_skill_metadata(skill_file)
                if not name:
                    name = skill_dir.name

                if len(description) > 50:
                    description = description[:50] + "..."

                # Calculate token count
                token_count = self._estimate_tokens(skill_file)
                token_str = f"~{token_count:,}" if token_count else "—"

                table.add_row(name, description, location, token_str)
                skill_count += 1

        # Add built-in skills (flat .md files with frontmatter)
        builtin_dir = paths.builtin_skills_dir
        if builtin_dir.exists():
            for md_file in sorted(builtin_dir.glob("*.md")):
                name, description = self._parse_skill_metadata(md_file)
                if not name:
                    name = md_file.stem
                if len(description) > 50:
                    description = description[:50] + "..."
                token_count = self._estimate_tokens(md_file)
                token_str = f"~{token_count:,}" if token_count else "\u2014"
                table.add_row(name, description, "built-in", token_str)
                skill_count += 1

        # Add plugin skills
        try:
            plugin_manager = PluginManager(self.config_manager.working_dir)
            plugin_skills = plugin_manager.get_plugin_skills()

            for skill in plugin_skills:
                description = skill.description
                if len(description) > 50:
                    description = description[:50] + "..."

                token_str = f"~{skill.token_count:,}" if skill.token_count else "—"

                table.add_row(
                    skill.display_name,
                    description,
                    skill.source_display,
                    token_str,
                )
                skill_count += 1
        except Exception:
            # Plugin loading failed, continue with local skills only
            pass

        if skill_count == 0:
            self.print_info("No skills found.")
            self.print_continuation(f"Create one with [cyan]/skills create[/cyan]")
            self.print_continuation(f"Or install plugins with [cyan]/plugins install[/cyan]")
            self.console.print()
        else:
            self.console.print(table)

        return CommandResult(success=True)

    def _estimate_tokens(self, file_path: Path) -> int:
        """Estimate token count for a file.

        Uses a simple heuristic: ~4 characters per token.

        Args:
            file_path: Path to file

        Returns:
            Estimated token count
        """
        try:
            content = file_path.read_text(encoding="utf-8")
            # Rough estimate: 4 characters per token
            return len(content) // 4
        except Exception:
            return 0

    def _parse_skill_metadata(self, skill_file: Path) -> tuple[str, str]:
        """Parse SKILL.md file to extract name and description from YAML frontmatter.

        Args:
            skill_file: Path to SKILL.md

        Returns:
            Tuple of (name, description)
        """
        try:
            content = skill_file.read_text(encoding="utf-8")
            name = ""
            description = ""

            # Check for YAML frontmatter
            if content.startswith("---"):
                parts = content.split("---", 2)
                if len(parts) >= 3:
                    frontmatter = parts[1]
                    for line in frontmatter.strip().split("\n"):
                        if line.startswith("name:"):
                            name = line.split(":", 1)[1].strip().strip("\"'")
                        elif line.startswith("description:"):
                            description = line.split(":", 1)[1].strip().strip("\"'")

            return name, description
        except Exception:
            return "", ""

    def _edit_skill(self, name: str) -> CommandResult:
        """Open a skill's SKILL.md file for editing.

        Args:
            name: Skill name to edit

        Returns:
            CommandResult with edit status
        """
        if not name:
            self.print_error("Skill name required: /skills edit <name>")
            return CommandResult(success=False, message="Skill name required")

        # Find the skill directory
        skill_dir = self._find_skill_dir(name)
        if not skill_dir:
            self.print_error(f"Skill not found: {name}")
            self.print_info("Use '/skills list' to see available skills")
            return CommandResult(success=False, message=f"Skill not found: {name}")

        skill_file = skill_dir / "SKILL.md"
        if not skill_file.exists():
            self.print_error(f"SKILL.md not found in: {skill_dir}")
            return CommandResult(success=False, message="SKILL.md not found")

        # Try to open in editor
        import os
        import subprocess

        editor = os.environ.get("EDITOR", "nano")
        try:
            subprocess.run([editor, str(skill_file)])
            self.print_success(f"Edited: {skill_file}")
            return CommandResult(success=True, message=f"Edited: {name}")
        except Exception as e:
            self.print_error(f"Failed to open editor: {e}")
            self.print_info(f"File location: {skill_file}")
            return CommandResult(success=False, message=str(e))

    def _test_skill(self, name: str) -> CommandResult:
        """Test a skill by loading it and showing a sample scenario.

        Args:
            name: Skill name to test

        Returns:
            CommandResult with test status
        """
        if not name:
            self.print_error("Skill name required: /skills test <name>")
            return CommandResult(success=False, message="Skill name required")

        # Find the skill directory
        skill_dir = self._find_skill_dir(name)
        if not skill_dir:
            self.print_error(f"Skill not found: {name}")
            self.print_info("Use '/skills list' to see available skills")
            return CommandResult(success=False, message=f"Skill not found: {name}")

        skill_file = skill_dir / "SKILL.md"
        if not skill_file.exists():
            self.print_error(f"SKILL.md not found in: {skill_dir}")
            return CommandResult(success=False, message="SKILL.md not found")

        # Load and display skill content
        try:
            content = skill_file.read_text(encoding="utf-8")
            skill_name, description = self._parse_skill_metadata(skill_file)

            self.console.print()
            self.console.print(
                Panel(
                    f"[bold]Name:[/bold] {skill_name or name}\n"
                    f"[bold]Description:[/bold] {description}\n"
                    f"[bold]Location:[/bold] {skill_dir}",
                    title="[bold cyan]Skill Metadata[/bold cyan]",
                    border_style="cyan",
                )
            )

            # Show content preview
            self.console.print()
            self.console.print(
                Panel(
                    content[:1500] + ("..." if len(content) > 1500 else ""),
                    title="[bold]SKILL.md Content[/bold]",
                    border_style="dim",
                )
            )

            # Prompt for test scenario
            self.console.print()
            self.print_info("To test this skill, send a message that would trigger it.")
            self.print_continuation(
                f'The skill will be loaded when Claude detects: "{description[:80]}..."'
            )
            self.console.print()

            return CommandResult(
                success=True,
                message=f"Skill loaded: {name}",
                data={"content": content, "path": str(skill_dir)},
            )

        except Exception as e:
            self.print_error(f"Failed to load skill: {e}")
            return CommandResult(success=False, message=str(e))

    def _delete_skill(self, name: str) -> CommandResult:
        """Delete a custom skill.

        Args:
            name: Skill name to delete

        Returns:
            CommandResult with deletion status
        """
        if not name:
            self.print_error("Skill name required: /skills delete <name>")
            return CommandResult(success=False, message="Skill name required")

        # Find the skill directory
        skill_dir = self._find_skill_dir(name)
        if not skill_dir:
            self.print_error(f"Skill not found: {name}")
            return CommandResult(success=False, message=f"Skill not found: {name}")

        # Confirm deletion
        if not Confirm.ask(f"Delete skill '{name}' and all its files?"):
            self.print_info("Cancelled")
            return CommandResult(success=False, message="Cancelled")

        try:
            shutil.rmtree(skill_dir)
            self.print_success(f"Deleted: {name}")
            return CommandResult(success=True, message=f"Deleted: {name}")
        except Exception as e:
            self.print_error(f"Failed to delete: {e}")
            return CommandResult(success=False, message=str(e))

    def _find_skill_dir(self, name: str) -> Optional[Path]:
        """Find the directory for a skill.

        Args:
            name: Skill name

        Returns:
            Path to the skill directory or None if not found
        """
        # Normalize name
        name = name.strip().replace(" ", "-").lower()

        # Search in project first, then global
        paths = get_paths(self.config_manager.working_dir)
        search_dirs = []
        if self.config_manager.working_dir:
            search_dirs.append(paths.project_skills_dir)
        search_dirs.append(paths.global_skills_dir)

        for skills_dir in search_dirs:
            skill_dir = skills_dir / name
            if skill_dir.exists() and skill_dir.is_dir():
                return skill_dir

        return None
