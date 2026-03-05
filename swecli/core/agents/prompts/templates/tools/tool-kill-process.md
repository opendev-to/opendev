<!--
name: 'Tool Description: kill_process'
description: Kill a background process
version: 2.0.0
-->

Kill a background process by its process ID.

## Usage notes

- Signal 15 (SIGTERM): Graceful shutdown — allows the process to clean up resources. Use this by default
- Signal 9 (SIGKILL): Force kill — immediately terminates the process. Use only when SIGTERM doesn't work
- Use list_processes first to find the process ID
- Clean up background processes (servers, watchers) when they are no longer needed or before the session ends
