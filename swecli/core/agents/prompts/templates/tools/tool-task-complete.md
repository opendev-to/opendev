<!--
name: 'Tool Description: task_complete'
description: Signal task completion
version: 2.0.0
-->

Signal that you have completed the user's request. You MUST call this tool to properly end the task — do NOT just stop making tool calls.

## Usage notes

- Provide a clear summary of what was accomplished in the result parameter
- Only call this when the work is truly done — all requested changes made, tests passing, and no unresolved issues
- For subagents: this is the required way to end the subagent conversation and return results to the parent agent
