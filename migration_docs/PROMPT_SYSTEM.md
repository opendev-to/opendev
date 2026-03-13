# Prompt System

## Overview

The prompt template system is the subsystem responsible for composing LLM system prompts from modular markdown sections. It lives in `opendev-agents` and is consumed by the ReAct loop (`opendev-agents`), the REPL layer (`opendev-repl`), and indirectly by the TUI and web backends. The system supports priority-ordered section registration, conditional inclusion based on runtime context, two-part cache-aware splitting for Anthropic prompt caching, variable substitution, and a three-tier template resolution chain (embedded compile-time, filesystem, fallback).

In the Python codebase the system spans five modules under `opendev/core/agents/prompts/`. In Rust it is consolidated into four modules under `crates/opendev-agents/src/prompts/`, with 91 markdown templates embedded into the binary at compile time via `include_str!`.

## Python Architecture

### Module Structure

```
opendev/core/agents/prompts/
    __init__.py          # Public API: load_prompt, get_prompt_path, get_reminder
    composition.py       # PromptComposer, PromptSection, factory functions
    loader.py            # load_prompt(), load_tool_description(), save_prompt()
    renderer.py          # PromptRenderer with ${VAR} substitution
    variables.py         # PromptVariables registry (ToolVariable, SystemReminderVariable)
    reminders.py         # get_reminder(), append_nudge(), section-delimited reminders.md parser
    templates/           # ~91 markdown template files
```

### Key Abstractions

- **`PromptSection`** (dataclass) -- A section descriptor holding `name`, `file_path`, optional `condition` callable, `priority` (int, lower = earlier), and `cacheable` (bool).
- **`PromptComposer`** -- The central class. Holds a `templates_dir` and a list of `PromptSection` instances. Provides `compose(context)` for single-string output and `compose_two_part(context)` for cache-aware splitting.
- **`PromptRenderer`** -- Renders individual template files with `${VAR}` substitution, supporting dotted access (`${TOOL.name}`), boolean coercion, and runtime variable injection.
- **`PromptVariables`** -- A registry of named template variables (tool references like `EDIT_TOOL`, `BASH_TOOL`; agent configuration like `EXPLORE_AGENT_COUNT`). Exports to a flat dict via `to_dict(**runtime_vars)`.
- **`get_reminder(name, **kwargs)`** -- Loads named reminder strings from `reminders.md` (section-delimited with `--- SECTION_NAME ---` markers) with `str.format()` placeholder substitution.

### Design Patterns

- **Strategy pattern**: Condition functions (`Callable[[Dict, Any], bool]`) determine section inclusion at compose time. Each section carries its own inclusion predicate.
- **Factory pattern**: `create_default_composer()`, `create_thinking_composer()`, and `create_composer(mode)` are factory functions that wire up the correct sections for each prompt mode.
- **Template Method**: `compose()` and `compose_two_part()` follow a fixed pipeline (filter -> sort -> load -> join) with hook points (conditions, frontmatter stripping).
- **Registry pattern**: `PromptVariables` acts as a central registry of named values for template interpolation.

### SOLID Analysis

- **SRP**: Each module has a single concern -- composition logic, file loading, variable management, rendering, and reminder retrieval are all separate.
- **OCP**: New sections are added by calling `register_section()` without modifying the composer class. New variables are added to `PromptVariables` without changing the renderer.
- **LSP**: Not heavily applicable -- no deep class hierarchies.
- **ISP**: The public API surface (`__init__.py`) exposes only `load_prompt`, `get_prompt_path`, and `get_reminder`.
- **DIP**: The composer depends on abstract conditions (`Callable`), not concrete context types.

### Variable Substitution Syntax

Python uses two substitution syntaxes:
1. **`${VAR}` / `${TOOL.name}`** in `PromptRenderer` -- regex-based, supports dotted property access.
2. **`{placeholder}`** in `reminders.py` -- standard Python `str.format()`, used for runtime values like `{original_task}`, `{plan_file_path}`.

## Rust Architecture

### Module Structure

```
crates/opendev-agents/src/prompts/
    mod.rs               # Public re-exports
    composer.rs          # PromptComposer, PromptSection, factory functions, substitute_variables()
    loader.rs            # PromptLoader struct with resolution chain
    embedded.rs          # 91 include_str! constants + LazyLock<HashMap> registry
```

```
crates/opendev-agents/templates/
    generators/          # 2 templates (agent, skill generators)
    memory/              # 3 templates (sentiment, topic, update instructions)
    subagents/           # 8 templates (ask-user, code-explorer, planner, etc.)
    system/              # 5 top-level + main/ (22 section templates) + thinking/ (4 templates)
    tools/               # 47 tool description templates
    reminders.md         # Section-delimited reminder strings
```

### Key Abstractions

- **`PromptSection`** (struct) -- Mirrors the Python dataclass. Fields: `name: String`, `file_path: String`, `condition: Option<ConditionFn>`, `priority: i32`, `cacheable: bool`. Since `ConditionFn` is a boxed closure, `Debug` is implemented manually.
- **`PromptComposer`** (struct) -- Owns a `Vec<PromptSection>` and a `templates_dir: PathBuf`. Provides `compose()`, `compose_with_vars()`, `compose_two_part()`, and `compose_two_part_with_vars()`.
- **`PromptLoader`** (struct) -- Resolves individual prompt files by name, trying filesystem first (user overrides), then embedded store, then optional fallback. Handles `.md` vs `.txt` format preference and frontmatter stripping.
- **`PromptContext`** -- Type alias for `HashMap<String, serde_json::Value>`. Runtime context for condition evaluation.
- **`ConditionFn`** -- Type alias for `Box<dyn Fn(&PromptContext) -> bool + Send + Sync>`. Boxed trait object enabling closure-based section predicates.
- **Condition helpers** -- `ctx_bool()`, `ctx_eq()`, `ctx_in()`, `ctx_present()` are factory functions that produce `ConditionFn` instances for common context checks.

### Embedded Template Store

All 91 templates are compiled into the binary via `include_str!` macros in `embedded.rs`. A `LazyLock<HashMap<&'static str, &'static str>>` (`TEMPLATES`) maps relative paths to their content. Category-specific accessor functions (`system_main_templates()`, `tool_templates()`, `subagent_templates()`, etc.) filter by path prefix.

This eliminates runtime filesystem dependencies for the default prompt set. User overrides are still supported via the filesystem fallback path.

### Template Resolution Order

Resolution differs slightly between the two main entry points:

| Entry Point | Priority 1 | Priority 2 | Priority 3 |
|---|---|---|---|
| `PromptComposer::load_section_content()` | Embedded store | Filesystem (`templates_dir`) | Skip (returns `None`) |
| `PromptLoader::load_prompt()` | Filesystem (user overrides) | Embedded store | Fallback string or error |

The composer favors embedded templates (guaranteed availability), while the loader favors filesystem (enabling user customization for individual prompt lookups).

### Design Patterns

- **Strategy pattern**: `ConditionFn` closures, identical to Python's approach but using boxed trait objects instead of bare callables.
- **Factory pattern**: `create_default_composer()`, `create_thinking_composer()`, `create_composer()` -- direct ports of the Python factories.
- **Flyweight pattern**: The embedded template store uses `&'static str` references to compile-time data, sharing template content without allocation.
- **Registry pattern**: The `TEMPLATES` HashMap serves as a compile-time registry. Category accessor functions provide filtered views.

### SOLID Analysis

- **SRP**: Composition (`composer.rs`), loading (`loader.rs`), and embedding (`embedded.rs`) are cleanly separated.
- **OCP**: New sections added via `register_section()` calls. New templates added by extending `embedded.rs` and the `TEMPLATES` map.
- **LSP**: No trait hierarchies -- the system uses composition throughout.
- **ISP**: `mod.rs` re-exports only the necessary public types: `PromptComposer`, `PromptLoader`, `PromptSection`, condition helpers, and key embedded accessors.
- **DIP**: The composer depends on `ConditionFn` (a trait object), not concrete condition implementations. Template content is accessed via `get_embedded()` (an abstraction over the HashMap), not via direct constant access.

### Variable Substitution Syntax

Rust uses `{{variable_name}}` syntax (double-brace), replacing the Python `${VAR}` notation. The `substitute_variables()` function applies a regex (`\{\{(\w+)\}\}`) over the composed output, replacing matches from a `HashMap<String, String>`. Unmatched placeholders are left as-is.

The Python `${TOOL.name}` dotted-access pattern was not ported. Rust templates use pre-resolved flat string values instead.

Reminder templates retain Python's `{placeholder}` syntax for `str.format()`-style substitution, applied at the call site rather than by the composer.

## Migration Mapping

| Python Class/Module | Rust Struct/Trait | Pattern Change | Notes |
|---|---|---|---|
| `PromptSection` (dataclass) | `PromptSection` (struct) | None -- direct port | `ConditionFn` is boxed trait object instead of bare `Callable` |
| `PromptComposer` (class) | `PromptComposer` (struct) | None -- direct port | Added `compose_with_vars()` and `compose_two_part_with_vars()` methods |
| `PromptRenderer` (class) | `substitute_variables()` (free fn) | Class -> function | Dotted `${TOOL.name}` access dropped; flat `{{var}}` map only |
| `PromptVariables` (class) | `HashMap<String, String>` | Class -> type alias | Variable registry replaced by caller-constructed HashMap |
| `loader.load_prompt()` (fn) | `PromptLoader::load_prompt()` (method) | Module fn -> struct method | Added embedded-first resolution; struct holds `templates_dir` |
| `loader.load_tool_description()` | `PromptLoader::load_tool_description()` | Same pattern | Kebab-case conversion preserved |
| `reminders.get_reminder()` (fn) | Embedded `REMINDERS` constant | Module -> compile-time embed | Section parsing happens at call site, not in a dedicated module |
| `reminders.append_nudge()` (fn) | Not in prompts module | Moved to agent loop | Nudge injection handled by the ReAct loop directly |
| `create_default_composer()` | `create_default_composer()` | Same factory pattern | Identical section registrations and priority numbers |
| `create_thinking_composer()` | `create_thinking_composer()` | Same factory pattern | Rust adds `thinking_core` section at priority 10 |
| N/A (filesystem only) | `embedded.rs` (91 `include_str!`) | New in Rust | Zero-I/O template loading; Python always reads from disk |
| Condition lambdas | `ctx_bool()`, `ctx_eq()`, `ctx_in()`, `ctx_present()` | Lambda -> named constructors | Improves readability; same runtime behavior |

## Key Design Decisions

### Compile-time embedding replaces filesystem-only loading

Python loads all templates from disk at runtime. Rust embeds all 91 templates into the binary via `include_str!`, making the prompt system zero-I/O by default. This eliminates a class of deployment failures (missing template files) and improves cold-start performance. Filesystem loading is retained as a fallback for user customization.

### PromptRenderer + PromptVariables collapsed into substitute_variables()

Python separated rendering (regex replacement) from variable storage (the `PromptVariables` class with `ToolVariable` and `SystemReminderVariable` dataclasses). In Rust, the renderer is a single free function operating on a flat `HashMap<String, String>`. The `ToolVariable` abstraction with dotted access (`${TOOL.name}`) was dropped because templates can simply use the resolved tool name directly. This reduces indirection with no loss of expressiveness.

### Condition closures use named constructors

Python uses inline lambdas (`lambda ctx: ctx.get("in_git_repo", False)`) for section conditions. Rust provides named factory functions (`ctx_bool("in_git_repo")`) that produce boxed closures. This improves readability in the factory functions and ensures type-safe context access via `serde_json::Value` methods.

### Variable syntax changed from ${VAR} to {{var}}

The `${VAR}` syntax conflicted with shell variable expansion in some contexts. The `{{var}}` double-brace syntax is unambiguous and aligns with common templating conventions (Handlebars, Jinja2). The regex `\{\{(\w+)\}\}` is simpler than the Python `\$\{([^}]+)\}` pattern.

### Two-part caching preserved as a first-class feature

Both Python and Rust support `compose_two_part()`, splitting the prompt into stable (cacheable) and dynamic sections. This directly supports Anthropic's prompt caching API, where the stable prefix gets `cache_control` markers. The Rust version adds `compose_two_part_with_vars()` for convenience.

### Reminder system simplified

Python's `reminders.py` module (with lazy section parsing, module-level cache, and `append_nudge()` helper) was not directly ported as a standalone module. The `reminders.md` file is embedded as a constant (`REMINDERS`), and section parsing is handled at the call site. The `append_nudge()` function moved to the ReAct loop where nudge injection actually happens.

## Code Examples

### Section Registration (Factory Function)

**Python** (`composition.py`):
```python
composer.register_section(
    "git_workflow",
    "system/main/main-git-workflow.md",
    condition=lambda ctx: ctx.get("in_git_repo", False),
    priority=70,
)
```

**Rust** (`composer.rs`):
```rust
composer.register_section(
    "git_workflow",
    "system/main/main-git-workflow.md",
    Some(ctx_bool("in_git_repo")),
    70,
    true,
);
```

The Rust version uses a named condition constructor (`ctx_bool`) instead of an inline lambda, and explicitly passes the `cacheable` flag (Python defaults to `True`).

### Two-Part Composition

**Python** (`composition.py`):
```python
stable, dynamic = composer.compose_two_part(context)
# stable gets cache_control in the API call
```

**Rust** (`composer.rs`):
```rust
let (stable, dynamic) = composer.compose_two_part(&context);
// Or with variable substitution:
let (stable, dynamic) = composer.compose_two_part_with_vars(&context, &variables);
```

### Template Resolution (Embedded First)

**Python** (`loader.py`) -- filesystem only:
```python
def load_prompt(prompt_name, fallback=None):
    prompt_file = get_prompt_path(prompt_name)  # .md preferred, .txt fallback
    if not prompt_file.exists():
        if fallback is not None:
            return fallback
        raise FileNotFoundError(...)
    content = prompt_file.read_text()
    return _strip_frontmatter(content) if prompt_file.suffix == ".md" else content.strip()
```

**Rust** (`composer.rs`) -- embedded, then filesystem:
```rust
fn load_section_content(&self, section: &PromptSection) -> Option<String> {
    // 1. Try embedded templates (zero I/O)
    if let Some(raw) = embedded::get_embedded(&section.file_path) {
        let stripped = strip_frontmatter(raw);
        if !stripped.is_empty() {
            return Some(stripped);
        }
    }
    // 2. Fallback to filesystem
    let file_path = self.templates_dir.join(&section.file_path);
    // ...
}
```

### Variable Substitution

**Python** (`renderer.py`) -- `${VAR}` with dotted access:
```python
content = re.sub(r"\$\{([^}]+)\}", replace_var, content)
# Supports ${TOOL.name}, ${VAR}, boolean coercion
```

**Rust** (`composer.rs`) -- `{{var}}` flat map:
```rust
pub fn substitute_variables(template: &str, variables: &HashMap<String, String>) -> String {
    VARIABLE_RE
        .replace_all(template, |caps: &regex::Captures| {
            variables.get(&caps[1]).cloned().unwrap_or_else(|| caps[0].to_string())
        })
        .into_owned()
}
```

## Remaining Gaps

1. **No dedicated `PromptVariables` equivalent** -- Rust callers must construct the variables HashMap manually. A builder or typed struct could reduce boilerplate if variable sets grow.
2. **No `save_prompt()` equivalent** -- Python supports writing customized prompts back to disk. Rust does not expose this functionality.
3. **Reminder section parsing not centralized** -- Python's lazy-parsed `_sections` cache in `reminders.py` has no direct Rust counterpart. If reminder lookups become frequent, a similar parsed cache could be added.
4. **No `append_nudge()` in prompts module** -- The nudge helper was moved to the agent loop rather than being co-located with prompt utilities.

## References

### Python Files
- `opendev-py/opendev/core/agents/prompts/composition.py` -- Composer, sections, factory functions
- `opendev-py/opendev/core/agents/prompts/loader.py` -- Prompt file loading
- `opendev-py/opendev/core/agents/prompts/renderer.py` -- Template rendering with `${VAR}` substitution
- `opendev-py/opendev/core/agents/prompts/variables.py` -- Variable registry
- `opendev-py/opendev/core/agents/prompts/reminders.py` -- Reminder string loading and nudge injection

### Rust Files
- `crates/opendev-agents/src/prompts/mod.rs` -- Public API re-exports
- `crates/opendev-agents/src/prompts/composer.rs` -- Composer, sections, conditions, variable substitution, factory functions
- `crates/opendev-agents/src/prompts/loader.rs` -- PromptLoader with resolution chain
- `crates/opendev-agents/src/prompts/embedded.rs` -- 91 `include_str!` constants and `TEMPLATES` registry
- `crates/opendev-agents/templates/` -- All markdown template files (system, tools, subagents, memory, generators, reminders)
