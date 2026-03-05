<!--
name: 'Tool Description: get_process_output'
description: Get output from a background process
version: 2.0.0
-->

Get output from a background process started with run_command background=true.

## Usage notes

- Returns stdout, stderr, process status, and exit code (if the process has finished)
- Use this to monitor long-running background commands — check server startup logs, build output, or test results
- The process must have been started with run_command background=true. Use list_processes to find available process IDs
- Can retrieve output from both running and completed processes
