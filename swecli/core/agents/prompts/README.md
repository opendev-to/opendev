# OpenDev Prompt Architecture

This directory contains the modular prompt system for OpenDev agents. The architecture is inspired by Claude Code's composable prompt design.

## Directory Structure

```
prompts/
├── composition.py          # Conditional loading engine
├── loader.py              # File loading (.md/.txt support)
├── renderer.py            # Variable substitution
├── variables.py           # Template variable registry
├── injections.py          # Runtime injection strings
├── CHANGELOG.md           # Version history
├── README.md              # This file
└── templates/
    ├── system/
    │   ├── main_system_prompt.md          # Monolithic (fallback)
    │   ├── planner_system_prompt.md       # Plan mode prompt
    │   ├── thinking_system_prompt.md      # Thinking mode prompt
    │   └── modular/                       # 🆕 Modular sections
    │       ├── system-prompt-core-role.md
    │       ├── system-prompt-security-policy.md
    │       ├── system-prompt-tone-and-style.md
    │       ├── system-prompt-git-workflow.md
    │       └── ... (16 total files)
    ├── subagents/                         # Subagent prompts
    ├── tools/                             # Tool descriptions
    ├── reminders/                         # System reminders
    ├── docker/                            # Docker context
    ├── generators/                        # Code generators
    └── memory/                            # Memory agents
```

## Quick Start

### Using the Composition System

```python
from swecli.core.agents.prompts.composition import create_default_composer
from pathlib import Path

# Create composer
templates_dir = Path("swecli/core/agents/prompts/templates")
composer = create_default_composer(templates_dir)

# Build context
context = {
    "in_git_repo": True,
    "has_subagents": True,
    "todo_tracking_enabled": True,
}

# Compose prompt
prompt = composer.compose(context)
```

### Loading Individual Prompts

```python
from swecli.core.agents.prompts.loader import load_prompt

# Loads .md (preferred) or .txt (fallback)
prompt = load_prompt("system/main_system_prompt")
```

### Variable Substitution

```python
from swecli.core.agents.prompts.renderer import PromptRenderer
from pathlib import Path

renderer = PromptRenderer()
template_path = Path("templates/tools/tool-enter-plan-mode.md")

# Renders with automatic variable substitution
result = renderer.render(template_path)
# ${EXIT_PLAN_MODE_TOOL.name} → "exit_plan_mode"
# ${ASK_USER_QUESTION_TOOL_NAME} → "ask_user"
```

## Architecture

### Modular Composition

The prompt system uses **conditional loading** with **priority-based ordering**:

| Priority | Category | Condition | Example |
|---|---|---|---|
| 10-30 | Core identity | Always | core-role, security-policy |
| 40-50 | Tool guidance | Always | available-tools, tool-selection |
| 60-80 | Conditional | Context | git-workflow (if in git repo) |
| 90+ | Utilities | Always | code-references, system-reminders |

### File Format

All modular files use markdown with YAML frontmatter:

```markdown
<!--
name: 'System Prompt: Section Name'
description: Brief description of this section
version: 2.0.0
variables:
  - TOOL_NAME
  - ANOTHER_VAR
-->

# Section Content

Your content here with ${VARIABLE} substitution.
```

### Claude Code Patterns

We adopt these patterns from Claude Code:

1. **Emphasis Hierarchy**:
   - `=== CRITICAL ===` for absolute requirements
   - `**IMPORTANT:**` for key rules
   - `**NOTE:**` for helpful context

2. **Decision Tables**: Markdown tables for tool/agent selection
   ```markdown
   | Request | Approach | Tool | Reason |
   |---|---|---|---|
   | "Read file" | Direct | read_file | Known path |
   | "Understand auth" | Subagent | Code-Explorer | Multi-file |
   ```

3. **Anti-Patterns**: Show what NOT to do
   ```markdown
   ❌ Edit without reading (will fail)
   ✅ Read → Edit (reliable)
   ```

4. **Variable Interpolation**: `${VAR}` syntax everywhere

## Adding New Sections

### 1. Create Section File

```bash
touch templates/system/modular/system-prompt-my-feature.md
```

```markdown
<!--
name: 'System Prompt: My Feature'
description: Feature-specific guidance
version: 2.0.0
-->

# My Feature

When using my feature:
- Do this
- Don't do that

**Rule of thumb**: ${FEATURE_TOOL.name} for simple cases, subagent for complex.
```

### 2. Register in Composition

Edit `composition.py`:

```python
def create_default_composer(templates_dir: Path) -> PromptComposer:
    composer = PromptComposer(templates_dir)

    # ... existing sections ...

    composer.register_section(
        "my_feature",
        "system/modular/system-prompt-my-feature.md",
        condition=lambda ctx: ctx.get("my_feature_enabled", False),
        priority=75  # Choose appropriate priority
    )

    return composer
```

### 3. Add Test Coverage

Create test in `tests/integration/test_modular_composition.py`:

```python
def test_my_feature_conditional_loading():
    """Verify my feature section loads conditionally."""
    composer = create_default_composer(templates_dir)

    # Without feature
    context_no_feature = {"my_feature_enabled": False}
    prompt_no = composer.compose(context_no_feature)
    assert "My Feature" not in prompt_no

    # With feature
    context_with_feature = {"my_feature_enabled": True}
    prompt_yes = composer.compose(context_with_feature)
    assert "My Feature" in prompt_yes
```

### 4. Run Tests

```bash
uv run pytest tests/integration/test_modular_composition.py -v
```

## Conditional Loading

Sections can be loaded conditionally based on runtime context:

```python
# In composition.py
composer.register_section(
    "git_workflow",
    "system/modular/system-prompt-git-workflow.md",
    condition=lambda ctx: ctx.get("in_git_repo", False),  # Only if git repo
    priority=70
)

# In builders.py
context = {
    "in_git_repo": self._env_context and self._env_context.is_git_repo,
    "has_subagents": True,
    "todo_tracking_enabled": True,
}
prompt = composer.compose(context)
```

## Template Variables

Available variables (defined in `variables.py`):

### Tool References
- `${EDIT_TOOL.name}` → `"edit_file"`
- `${WRITE_TOOL.name}` → `"write_file"`
- `${READ_TOOL.name}` → `"read_file"`
- `${BASH_TOOL.name}` → `"run_command"`
- `${GLOB_TOOL.name}` → `"list_files"`
- `${GREP_TOOL.name}` → `"search"`
- `${EXIT_PLAN_MODE_TOOL.name}` → `"exit_plan_mode"`
- `${ASK_USER_QUESTION_TOOL_NAME}` → `"ask_user"`

### Agent Configuration
- `${EXPLORE_AGENT_COUNT}` → `3`
- `${PLAN_AGENT_COUNT}` → `1`

### System Reminder
- `${SYSTEM_REMINDER.planFilePath}` → Path to plan file
- `${SYSTEM_REMINDER.planExists}` → Boolean

### Adding New Variables

Edit `variables.py`:

```python
class PromptVariables:
    def __init__(self):
        # ... existing variables ...
        self.MY_NEW_VAR = "value"

    def to_dict(self, **runtime_vars: Any) -> Dict[str, Any]:
        base = {
            # ... existing ...
            "MY_NEW_VAR": self.MY_NEW_VAR,
        }
        base.update(runtime_vars)
        return base
```

Use in templates:

```markdown
The value is ${MY_NEW_VAR}.
```

## Testing

Run the full integration test suite:

```bash
# All tests
uv run pytest tests/integration/ -v

# Specific test file
uv run pytest tests/integration/test_modular_composition.py -v

# Single test
uv run pytest tests/integration/test_modular_composition.py::test_modular_composition_loads_all_sections -v
```

### Test Coverage

- ✅ Prompt composition (loads all sections)
- ✅ Conditional loading (git workflow, todos, subagents)
- ✅ Priority ordering (sections load in correct order)
- ✅ Variable substitution (no unresolved variables)
- ✅ Frontmatter stripping (clean content)
- ✅ SystemPromptBuilder integration
- ✅ Reminder injection
- ✅ Tool descriptions

## Migration Guide

### From Monolithic to Modular

If you have custom modifications to `main_system_prompt.txt`:

1. **Identify your changes**:
   ```bash
   git diff main_system_prompt.txt
   ```

2. **Find the corresponding modular file**:
   - Core identity → `system-prompt-core-role.md`
   - Security → `system-prompt-security-policy.md`
   - Git workflow → `system-prompt-git-workflow.md`
   - etc.

3. **Apply changes to modular file**:
   ```bash
   # Edit the specific modular file
   vim templates/system/modular/system-prompt-git-workflow.md
   ```

4. **Test**:
   ```bash
   uv run pytest tests/integration/ -v
   ```

### Creating Custom Sections

For project-specific needs, create custom modular sections:

```bash
# Create custom section
touch templates/system/modular/system-prompt-custom-myproject.md

# Register in composition.py (optional - can also load manually)
# Test with integration tests
```

## Performance

- **Composition time**: < 50ms (16 sections)
- **Caching**: Not yet implemented (planned for v2.1)
- **Lazy loading**: Conditional sections only loaded when needed

## Troubleshooting

### "Prompt file not found"

Check both .md and .txt exist:
```bash
ls templates/system/main_system_prompt.*
```

### "Unresolved variables in prompt"

Check `variables.py` for variable definition:
```python
grep "MY_VAR" swecli/core/agents/prompts/variables.py
```

### "Section not loading"

Check composition registration and condition:
```python
# In composition.py
composer.register_section(
    "my_section",
    "system/modular/system-prompt-my-section.md",
    condition=lambda ctx: ctx.get("enabled", True),  # Check this
    priority=50
)
```

Debug with test:
```python
context = {"enabled": True}
prompt = composer.compose(context)
print("my_section" in prompt)  # Should be True
```

## Future Enhancements

- [ ] Hot-reload for development
- [ ] Prompt validation script
- [ ] Composition visualizer (what's loaded when)
- [ ] Performance profiling
- [ ] Prompt caching
- [ ] Variable resolution error tracking

## References

- **CHANGELOG.md**: Version history and migration notes
- **Tests**: `tests/integration/test_modular_composition.py`
- **Claude Code**: Inspiration and patterns
- **Plan**: `PROMPT_OVERHAUL_PROGRESS.md`
