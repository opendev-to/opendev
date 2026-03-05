<!--
name: 'Tool Description: update_todo'
description: Update an existing todo status
version: 2.0.0
-->

Update an existing todo item's status or content as you work through your plan.

## Status workflow

pending → in_progress → completed

- Set to 'in_progress' BEFORE starting work on a task
- Set to 'completed' IMMEDIATELY after finishing work — but ONLY when the task is FULLY accomplished
- Never mark completed if: tests are failing, implementation is partial, or unresolved errors remain

## Usage notes

- Use the todo ID (e.g., 'todo-1' or just '1') to identify which task to update
- This is how you track progress through your plan — the user sees status changes in real time
- After marking a task completed, check list_todos for the next pending task
