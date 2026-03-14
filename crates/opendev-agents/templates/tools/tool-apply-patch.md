<!--
name: 'Tool Description: patch'
description: Apply unified diff or structured patches to files
version: 2.0.0
-->

Apply patches to files. Supports two formats:

## Format 1: Unified Diff (standard)

Standard unified diff format (output of `git diff` or `diff -u`). Uses `git apply` first, falls back to manual application.

## Format 2: Structured Patch

A file-oriented format with explicit operations:

```
*** Begin Patch
*** Add File: path/to/new_file.rs
+content of new file here...
*** Update File: path/to/existing_file.rs
 context line for seeking
-old line to remove
+new line to add
*** Delete File: path/to/remove.rs
*** Move File: old/path.rs -> new/path.rs
*** End Patch
```

Operations:
- **Add File** — create a new file (with mkdir -p). Every line is prefixed with `+`
- **Delete File** — remove a file
- **Move File** — rename/move a file (optionally followed by update operations)
- **Update File** — modify in-place using context-based seeking with multi-pass matching (exact, trim-end, trim)

## Usage notes

- Multi-file patches are supported in both formats
- For structured patches, context lines (starting with space) are used to locate the position for changes
- Prefer edit_file or multi_edit for simple changes; use patch for complex multi-file diffs
