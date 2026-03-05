<!--
name: 'Tool Description: get_subagent_output'
description: Get output from a background subagent
version: 2.0.0
-->

Retrieve output from a subagent that was launched with run_in_background=true.

## Usage notes

- ONLY use this for subagents launched with run_in_background=true. Synchronous subagents (the default) return results immediately in the tool response with [completion_status=success] — do NOT call this tool for them
- The tool_call_id from spawn_subagent is NOT a task_id — only background subagents return task_ids that can be used here
- By default blocks until the subagent completes. Use block=false for non-blocking status checks to see if the subagent is still running
- Returns the subagent's final result, including any summary or output it produced
