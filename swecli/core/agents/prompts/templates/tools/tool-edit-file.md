<!--
name: 'Tool Description: edit_file'
description: Edit an existing file by replacing content
version: 2.0.0
-->

Edit an existing file by replacing old content with new content.

## Usage notes

- IMPORTANT: You MUST read the file first with read_file before editing it. This tool will error if you attempt an edit without reading the file
- When editing text from read_file output, preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: spaces + line number + tab. Everything after that tab is the actual file content. Never include any part of the line number prefix in old_content or new_content
- The old_content must match EXACTLY — including indentation, whitespace, and newlines. If old_content is not found, the edit will fail. Read the file again to get the exact content
- The edit will FAIL if old_content is not unique in the file. Either provide a larger string with more surrounding context to make it unique, or use match_all=true to change every instance
- Use match_all=true for replacing and renaming strings across the file. This is useful for renaming a variable or updating repeated patterns
- Prefer edit_file over write_file for modifications — it preserves unrelated content and is safer
- ALWAYS prefer editing existing files over creating new ones, as this prevents file bloat and builds on existing work
