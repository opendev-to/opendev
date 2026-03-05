"""Tool for editing Jupyter notebook cells."""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any, Literal
import uuid

logger = logging.getLogger(__name__)


class NotebookEditTool:
    """Edit Jupyter notebook (.ipynb) cells.

    Supports three edit modes:
    - replace: Replace an existing cell's content
    - insert: Insert a new cell at a position
    - delete: Delete a cell

    Cells can be identified by:
    - cell_id: The cell's unique ID (preferred)
    - cell_number: 0-indexed position in the notebook
    """

    def __init__(self, working_dir: Path):
        """Initialize notebook edit tool.

        Args:
            working_dir: Working directory for resolving relative paths
        """
        self.working_dir = working_dir

    def edit_cell(
        self,
        notebook_path: str,
        new_source: str,
        cell_id: str | None = None,
        cell_number: int | None = None,
        cell_type: Literal["code", "markdown"] | None = None,
        edit_mode: Literal["replace", "insert", "delete"] = "replace",
    ) -> dict[str, Any]:
        """Edit a cell in a Jupyter notebook.

        Args:
            notebook_path: Path to the .ipynb file (absolute or relative)
            new_source: New cell source content
            cell_id: Cell ID to edit (preferred method)
            cell_number: 0-indexed cell position (alternative to cell_id)
            cell_type: Cell type (code or markdown). Required for insert mode.
            edit_mode: Operation type - replace, insert, or delete

        Returns:
            Dictionary with:
            - success: bool
            - cell_id: str (ID of affected cell)
            - output: str (operation summary)
            - error: str | None
        """
        # Resolve path
        path = Path(notebook_path)
        if not path.is_absolute():
            path = self.working_dir / path

        # Validate file exists and is a notebook
        if not path.exists():
            return {
                "success": False,
                "error": f"Notebook not found: {notebook_path}",
                "cell_id": None,
                "output": None,
            }

        if not path.suffix == ".ipynb":
            return {
                "success": False,
                "error": f"Not a Jupyter notebook file: {notebook_path}",
                "cell_id": None,
                "output": None,
            }

        try:
            # Load notebook
            with open(path, "r", encoding="utf-8") as f:
                notebook = json.load(f)

            cells = notebook.get("cells", [])

            if edit_mode == "insert":
                return self._insert_cell(
                    path, notebook, cells, new_source, cell_id, cell_number, cell_type
                )
            elif edit_mode == "delete":
                return self._delete_cell(path, notebook, cells, cell_id, cell_number)
            else:  # replace
                return self._replace_cell(
                    path, notebook, cells, new_source, cell_id, cell_number, cell_type
                )

        except json.JSONDecodeError as e:
            return {
                "success": False,
                "error": f"Invalid notebook JSON: {str(e)}",
                "cell_id": None,
                "output": None,
            }
        except Exception as e:
            logger.exception(f"Failed to edit notebook: {notebook_path}")
            return {
                "success": False,
                "error": f"Failed to edit notebook: {str(e)}",
                "cell_id": None,
                "output": None,
            }

    def _find_cell_index(
        self,
        cells: list[dict],
        cell_id: str | None,
        cell_number: int | None,
    ) -> tuple[int | None, str | None]:
        """Find a cell by ID or number.

        Args:
            cells: List of notebook cells
            cell_id: Cell ID to find
            cell_number: Cell number (0-indexed) to find

        Returns:
            Tuple of (index, error_message)
        """
        if cell_id is not None:
            # Find by ID
            for i, cell in enumerate(cells):
                if cell.get("id") == cell_id:
                    return i, None
            return None, f"Cell with ID '{cell_id}' not found"

        if cell_number is not None:
            if cell_number < 0 or cell_number >= len(cells):
                return None, f"Cell number {cell_number} out of range (0-{len(cells) - 1})"
            return cell_number, None

        return None, "Either cell_id or cell_number must be provided"

    def _replace_cell(
        self,
        path: Path,
        notebook: dict,
        cells: list[dict],
        new_source: str,
        cell_id: str | None,
        cell_number: int | None,
        cell_type: str | None,
    ) -> dict[str, Any]:
        """Replace an existing cell's content."""
        index, error = self._find_cell_index(cells, cell_id, cell_number)
        if error:
            return {"success": False, "error": error, "cell_id": None, "output": None}

        cell = cells[index]
        old_source = "".join(cell.get("source", []))

        # Update source (store as list of lines for proper format)
        source_lines = new_source.split("\n")
        cell["source"] = [line + "\n" for line in source_lines[:-1]] + [source_lines[-1]]

        # Update cell type if specified
        if cell_type:
            cell["cell_type"] = cell_type

        # Save notebook
        self._save_notebook(path, notebook)

        result_cell_id = cell.get("id", f"cell-{index}")
        return {
            "success": True,
            "cell_id": result_cell_id,
            "output": f"Replaced cell {result_cell_id} content ({len(old_source)} -> {len(new_source)} chars)",
            "error": None,
        }

    def _insert_cell(
        self,
        path: Path,
        notebook: dict,
        cells: list[dict],
        new_source: str,
        after_cell_id: str | None,
        at_position: int | None,
        cell_type: str | None,
    ) -> dict[str, Any]:
        """Insert a new cell."""
        if not cell_type:
            cell_type = "code"  # Default to code cell

        # Determine insert position
        if after_cell_id is not None:
            index, error = self._find_cell_index(cells, after_cell_id, None)
            if error:
                return {"success": False, "error": error, "cell_id": None, "output": None}
            insert_pos = index + 1
        elif at_position is not None:
            # Clamp to valid range (can insert at end)
            insert_pos = max(0, min(at_position, len(cells)))
        else:
            # Default: insert at end
            insert_pos = len(cells)

        # Create new cell
        new_cell_id = str(uuid.uuid4())[:8]
        source_lines = new_source.split("\n")
        new_cell = {
            "id": new_cell_id,
            "cell_type": cell_type,
            "metadata": {},
            "source": [line + "\n" for line in source_lines[:-1]] + [source_lines[-1]],
        }

        # Add execution_count and outputs for code cells
        if cell_type == "code":
            new_cell["execution_count"] = None
            new_cell["outputs"] = []

        # Insert cell
        cells.insert(insert_pos, new_cell)
        notebook["cells"] = cells

        # Save notebook
        self._save_notebook(path, notebook)

        return {
            "success": True,
            "cell_id": new_cell_id,
            "output": f"Inserted new {cell_type} cell at position {insert_pos}",
            "error": None,
        }

    def _delete_cell(
        self,
        path: Path,
        notebook: dict,
        cells: list[dict],
        cell_id: str | None,
        cell_number: int | None,
    ) -> dict[str, Any]:
        """Delete a cell."""
        index, error = self._find_cell_index(cells, cell_id, cell_number)
        if error:
            return {"success": False, "error": error, "cell_id": None, "output": None}

        deleted_cell = cells.pop(index)
        deleted_cell_id = deleted_cell.get("id", f"cell-{index}")
        notebook["cells"] = cells

        # Save notebook
        self._save_notebook(path, notebook)

        return {
            "success": True,
            "cell_id": deleted_cell_id,
            "output": f"Deleted cell {deleted_cell_id} (was at position {index})",
            "error": None,
        }

    def _save_notebook(self, path: Path, notebook: dict) -> None:
        """Save notebook to file with proper formatting."""
        with open(path, "w", encoding="utf-8") as f:
            json.dump(notebook, f, indent=1, ensure_ascii=False)
            f.write("\n")  # Ensure trailing newline
