<!--
name: 'Tool Description: read_file'
description: Read a file from the local filesystem
version: 2.0.0
-->

Read a file from the local filesystem. Returns content with line numbers in cat -n format, with line numbers starting at 1.

## Usage notes

- The file_path must be an absolute or relative path
- By default reads up to 2000 lines from the beginning of the file. Use offset and max_lines for large files, but prefer reading the whole file when feasible
- Lines longer than 2000 characters are truncated
- Binary files are detected and rejected with an error message
- This tool can read images (PNG, JPG, etc.) — image contents are presented visually since the model is multimodal
- This tool can read PDF files (.pdf). For large PDFs (more than 10 pages), provide the pages parameter to read specific page ranges (e.g., pages: "1-5"). Maximum 20 pages per request
- This tool can read Jupyter notebooks (.ipynb) and returns all cells with their outputs, combining code, text, and visualizations
- This tool can only read files, not directories. To list directory contents, use list_files
- If you read a file that exists but has empty contents, you will receive a warning in place of file contents
- IMPORTANT: Always read a file before editing it. edit_file will fail if old_content doesn't match the actual file content
- Prefer read_file over run_command with cat/head/tail
- Read multiple files in parallel using batch_tool when you need to examine several files at once
- When the user provides a path to a screenshot or image, ALWAYS use this tool to view it
