<!--
name: 'Tool Description: write_file'
description: Create a new file with specified content
version: 2.0.0
-->

Create a new file or overwrite an existing file with the specified content.

## Usage notes

- This tool will overwrite the existing file if there is one at the provided path
- If this is an existing file, you MUST use read_file first to read its contents. This tool will fail if you did not read the file first
- ALWAYS prefer editing existing files with edit_file over writing new files. Only create new files when explicitly required
- NEVER proactively create documentation files (*.md) or README files. Only create documentation files if explicitly requested by the user
- Parent directories are created automatically when create_dirs=true
- Do not create files unless absolutely necessary for achieving the goal. Prefer editing an existing file to creating a new one, as this prevents file bloat and builds on existing work
