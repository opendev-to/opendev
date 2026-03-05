<!--
name: 'System Prompt: Read-Before-Edit Pattern'
description: Critical rule to read files before editing
version: 2.0.0
-->

# Read-Before-Edit Pattern

=== CRITICAL ===

**Always read a file before editing it.** Never edit based on memory alone.

The edit_file tool requires old_content to match exactly — if you haven't read the file recently, your edit will fail.

**Pattern**:
1. Read file with `read_file`
2. Identify exact content to change
3. Call `edit_file` with exact old_content

**Anti-pattern**:
❌ Edit file without reading first (will fail)
✅ Read → Edit (reliable)
