"""Handler for notebook edit tool invocations."""

from __future__ import annotations

from typing import Any


class NotebookEditHandler:
    """Executes notebook edit operations."""

    def __init__(self, notebook_edit_tool: Any) -> None:
        """Initialize the handler.

        Args:
            notebook_edit_tool: NotebookEditTool instance for editing notebooks
        """
        self._tool = notebook_edit_tool

    def edit_cell(self, args: dict[str, Any]) -> dict[str, Any]:
        """Handle notebook cell edit requests.

        Args:
            args: Dictionary with:
                - notebook_path: Path to the .ipynb file (required)
                - new_source: New cell source content (required)
                - cell_id: Cell ID to edit (optional)
                - cell_number: 0-indexed cell position (optional, alternative to cell_id)
                - cell_type: "code" or "markdown" (optional)
                - edit_mode: "replace", "insert", or "delete" (default: "replace")

        Returns:
            Result dictionary with formatted output
        """
        if not self._tool:
            return {
                "success": False,
                "error": "NotebookEditTool not available",
                "output": None,
            }

        notebook_path = args.get("notebook_path", "")
        new_source = args.get("new_source", "")
        cell_id = args.get("cell_id")
        cell_number = args.get("cell_number")
        cell_type = args.get("cell_type")
        edit_mode = args.get("edit_mode", "replace")

        if not notebook_path:
            return {
                "success": False,
                "error": "notebook_path is required",
                "output": None,
            }

        # For delete mode, new_source is not required
        if edit_mode != "delete" and not new_source and new_source != "":
            return {
                "success": False,
                "error": "new_source is required for replace and insert modes",
                "output": None,
            }

        # Perform the edit
        result = self._tool.edit_cell(
            notebook_path=notebook_path,
            new_source=new_source,
            cell_id=cell_id,
            cell_number=cell_number,
            cell_type=cell_type,
            edit_mode=edit_mode,
        )

        if not result.get("success"):
            return {
                "success": False,
                "error": result.get("error", "Unknown error"),
                "output": None,
            }

        return {
            "success": True,
            "output": result.get("output", "Cell edited successfully"),
            "cell_id": result.get("cell_id"),
        }
