<!--
name: 'Thinking: Code References'
description: Format for referencing code locations in thinking output
version: 1.0.0
priority: 85
-->

# Code References

When referencing specific functions or code locations, include `file_path:line_number`:

## Example

```
user: Where are errors from the client handled?
assistant: Clients are marked as failed in `connectToServer` in src/services/process.ts:712.
```

This format allows users to navigate directly to the code location.
