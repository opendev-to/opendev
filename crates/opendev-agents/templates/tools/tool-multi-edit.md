<!--
name: 'Tool Description: multi_edit'
description: Apply multiple sequential edits to a single file atomically
version: 1.0.0
-->

Apply multiple sequential edits to a single file in one operation. Built on the same fuzzy matching as edit_file.

## Usage notes

- Prefer multi_edit over multiple edit_file calls when you need to make several changes to the same file
- All edits are applied in sequence — each edit operates on the result of the previous one
- **Atomic**: if any edit fails, none are applied. The file is left unchanged
- Each edit follows the same rules as edit_file: old_string must match (fuzzy matching is supported), and must be unique unless replace_all is true
- Plan your edits carefully: earlier edits change the content that later edits search through
- IMPORTANT: You MUST read the file first with read_file before using multi_edit
- old_string and new_string must be different for each edit
- Use replace_all on individual edits when you want to rename a variable or replace all occurrences of a pattern
