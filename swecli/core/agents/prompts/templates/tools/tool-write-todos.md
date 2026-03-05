<!--
name: 'Tool Description: write_todos'
description: Create a todo list for complex tasks
version: 2.0.0
-->

Create a structured task list at the START of a complex task. This helps track progress, organize multi-step work, and demonstrates thoroughness to the user.

## When to use

- Complex multi-step tasks requiring 3 or more distinct steps
- User provides multiple tasks (numbered or comma-separated)
- After receiving new instructions that involve significant work
- When using plan mode, to track the implementation plan

## When NOT to use

- Single straightforward task that can be completed in fewer than 3 trivial steps
- Trivial tasks where tracking provides no organizational benefit
- Purely conversational or informational tasks

## Task fields

- **content**: A brief, actionable title in plain text, imperative form (e.g., "Fix authentication bug in login flow"). NEVER use markdown formatting — no **bold**, *italic*, `backticks`, or any markup. Plain text only.
- **activeForm**: Present continuous form shown in the spinner when the task is in_progress (e.g., "Fixing authentication bug"). Plain text only. ALWAYS provide this field.
- **status**: All initial items should use 'pending' status

## Usage notes

- Write 4-7 todo items that break down the work into steps
- REPLACES the entire todo list — call once at the beginning, then use update_todo to change status as you work
- Exactly ONE task should be in_progress at any time. Mark it in_progress BEFORE beginning work on it
- ONLY mark a task as completed when you have FULLY accomplished it. Never mark completed if tests are failing, implementation is partial, or errors are unresolved
- After completing a task, check list_todos for the next available task
