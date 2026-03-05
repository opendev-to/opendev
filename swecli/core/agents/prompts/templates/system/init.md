<!--
name: 'System Prompt: Init'
description: Session initialization guidance
version: 1.0.0
-->

Analyze the codebase at {path} and generate a comprehensive OPENDEV.md that serves as the definitive reference for any AI agent or developer working in this repository.

Use spawn_subagent with Code-Explorer:
"Explore {path} thoroughly. Read ALL config files (package.json, pyproject.toml, setup.py, setup.cfg, Makefile, Dockerfile, Cargo.toml, go.mod, etc.), README, and any CI/CD configs (.github/workflows/, .gitlab-ci.yml). Also read 2-3 core source files to understand the architecture. Report: project name, description, tech stack, all available commands (install, run, test, lint, build, deploy), main directories with purposes, architecture layers, key design patterns, code style conventions, and any testing requirements."

After exploration, use write_file to create {path}/OPENDEV.md with this format:

```
# OPENDEV.md

This file provides guidance when working with code in this repository.

## Build & Development Commands

```bash
# Install dependencies
<actual install command>

# Run the application
<actual run command(s) with comments explaining different modes>

# Code quality
<actual lint/format/typecheck commands>

# Tests
<actual test commands: all tests, single file, single test, with coverage>

# Other useful commands (if applicable)
<deploy, build, migration, etc.>
```

## Architecture Overview

```
<ASCII diagram showing the main layers/components and how they connect>
Entry Point (file.ext)
       ↓
Layer 1 (directory/)
  - component: description
       ↓
Layer 2 (directory/)
  - component: description
```

## Key Patterns

**Pattern Name** (`relevant_file.ext`): Brief explanation of the pattern and how it works in this codebase.

**Pattern Name**: Brief explanation.

(Include: architectural patterns like MVC/ReAct/CQRS, dependency injection, configuration loading, extension/plugin systems, error handling patterns, etc.)

## Code Style

- Line length, formatter, linter used
- Naming conventions
- Import ordering
- Docstring style
- Type annotation requirements
- Any other enforced conventions
```

CRITICAL RULES:
- Use REAL commands discovered from config files — never guess or use placeholders
- If a section has no applicable content, omit it entirely (don't write "N/A")
- Architecture diagram should reflect the ACTUAL directory structure
- Keep descriptions concise — max 500 words total
- Format all file paths, commands, and code references with backticks
- Do NOT include testing requirements or instructions for AI agents — only include factual project information
