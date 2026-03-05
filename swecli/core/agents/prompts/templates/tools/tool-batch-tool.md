<!--
name: 'Tool Description: batch_tool'
description: Execute multiple tool invocations in parallel or serial
version: 2.0.0
-->

Execute multiple tool invocations in a single call, either in parallel or serial mode.

## When to use

- **Parallel mode** (default): For independent operations that don't depend on each other's results. Examples: reading multiple files at once, running multiple searches, checking several processes
- **Serial mode**: When operations depend on each other and must run in sequence. Example: read a file then edit it based on what you found

## Usage notes

- Max 5 concurrent workers in parallel mode
- Results are returned in the same order as the invocations, regardless of completion order
- If one invocation fails in parallel mode, the others still complete — failures don't cancel sibling operations
- Each invocation in the batch is a standard tool call with name and arguments
- Use batch_tool to maximize efficiency when you need to perform multiple independent operations in a single turn
