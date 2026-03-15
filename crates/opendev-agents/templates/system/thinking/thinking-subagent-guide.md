<!--
name: 'Thinking: Subagent Selection Guide'
description: Guide for deciding when and which subagent to delegate to
version: 1.0.0
priority: 50
-->

# Subagent Selection Guide

When considering delegation to a subagent, reason through these questions:

## Which subagent matches the task?

- **ask-user**: Need clarification on ambiguous requirements or user preferences? (e.g., which auth method, database choice)
- **Code-Explorer**: Need to understand LOCAL codebase structure, find implementations, or trace patterns?
- **Planner**: Need to create a detailed implementation plan? Spawn a Planner subagent.

## Is a subagent appropriate?

**YES, spawn a subagent when**:
- Task requires specialized expertise (paper implementation, web generation)
- Task involves multiple files and complex coordination
- Task requires deep codebase exploration with many searches
- Task needs user input through structured questions
- Task is isolated and can run in fresh context

**NO, handle directly when**:
- Single file edit or quick refactor
- Simple grep/search operation
- Reading one or two files
- Running a single command
- Quick answer from existing context

## Anti-patterns — do NOT spawn a subagent when:
- The task is creative/design work with no existing codebase to explore (game design, brainstorming, writing specs from scratch) — handle directly
- The task doesn't match ANY subagent's purpose — don't force-fit the closest option
- Code-Explorer is ONLY for LOCAL files that already exist — never use it for greenfield design or external knowledge tasks

## Common patterns to recognize:

### Direct Tool Usage (Handle yourself):
- "Read src/app.py" -> `read_file("src/app.py")`
- "Find function handleError" -> `search("def handleError", type="text")`
- "List all Python files" -> `list_files("**/*.py")`
- "Show me the package.json" -> `read_file("package.json")`
- "Run the tests" -> `run_command("pytest")`
- "Find all TODO comments" -> `search("TODO", type="text")`
- "What's in the config?" -> `read_file` on config file
- "Create a utils file" -> `write_file` with content

### Subagent Delegation (Spawn subagent):
- "How does authentication work in this codebase?" -> **Code-Explorer** (multi-file trace)
- "Understand the database schema" -> **Code-Explorer** (models + migrations)
- "What caching strategy is used?" -> **Code-Explorer** (find all cache uses)
- "Plan adding real-time features" -> **Planner** subagent (design approach)
- "Which library should we use for X?" -> **ask-user** (user preference)
- "Implement user registration system" -> **Planner** subagent first for design, then implement
- "Explain the error handling strategy" -> **Code-Explorer** (multi-file analysis)

**Remember**: Subagent results aren't shown to the user - you must summarize their findings in your reasoning and response.
