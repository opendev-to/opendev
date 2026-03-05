<!--
name: 'System Prompt: Scratchpad'
description: Session-specific temporary file guidance
version: 1.0.0
-->

# Scratchpad

For temporary files (drafts, scratch work, intermediate outputs), use `~/.opendev/sessions/{session_id}/scratch/` instead of `/tmp`. This avoids collisions between concurrent sessions and enables automatic cleanup with session data.
