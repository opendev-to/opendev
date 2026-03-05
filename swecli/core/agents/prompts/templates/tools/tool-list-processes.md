<!--
name: 'Tool Description: list_processes'
description: List running background processes
version: 2.0.0
-->

List all running background processes started by run_command with background=true.

## Usage notes

- Returns process information including PID, command, status (running/exited), and runtime
- Only lists processes started within this session — does not show system processes
- Use this to check on background servers, long-running tasks, or to find a process ID before using kill_process
