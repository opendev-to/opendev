<!--
name: 'System Prompt: Task Tracking'
description: Using todos for multi-step work
version: 2.0.0
-->

# Task Tracking

Use todos for multi-file changes, feature implementation, or build/test/fix cycles. Skip for simple single-file edits.

## Workflow

1. Create todos ONCE at start with `write_todos` (all start as `pending`)
2. Work through todos IN ORDER:
   - `update_todo(id, status="in_progress")` when starting
   - Do the work
   - `complete_todo(id)` when finished
3. Keep only ONE todo `in_progress` at a time
4. **NEVER skip todos** - if work was done implicitly, mark it complete
5. **The system will remind you if todos remain incomplete when you try to finish**

## When to Use

✅ Multi-file changes
✅ Feature implementation with multiple steps
✅ Build/test/fix cycles
❌ Simple single-file edits

## Formatting

Todo content must be plain text — no markdown (no bold, italic, backticks, or links). The system strips markdown automatically, so formatting is wasted tokens.
