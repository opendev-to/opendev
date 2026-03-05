"""Todo/Task management handler for tracking development tasks."""

import logging
from dataclasses import dataclass, asdict
from datetime import datetime
from typing import Dict, List, Optional


# Configure logger for todo ID validation warnings
logger = logging.getLogger(__name__)


@dataclass
class TodoItem:
    """A todo/task item."""

    id: str
    title: str
    status: str  # "todo", "doing", or "done"
    active_form: str = ""  # Present continuous form for spinner display (e.g., "Running tests")
    log: str = ""
    expanded: bool = False
    created_at: str = ""
    updated_at: str = ""

    def __post_init__(self):
        if not self.created_at:
            self.created_at = datetime.now().isoformat()
        if not self.updated_at:
            self.updated_at = datetime.now().isoformat()


class TodoHandler:
    """Handler for todo/task management operations."""

    @staticmethod
    def _strip_markdown(text: str) -> str:
        """Strip markdown formatting from todo titles."""
        import re

        text = re.sub(r"\*\*(.+?)\*\*", r"\1", text)
        text = re.sub(r"__(.+?)__", r"\1", text)
        text = re.sub(r"\*(.+?)\*", r"\1", text)
        text = re.sub(r"(?<!\w)_(.+?)_(?!\w)", r"\1", text)
        text = re.sub(r"`(.+?)`", r"\1", text)
        text = re.sub(r"~~(.+?)~~", r"\1", text)
        text = re.sub(r"^#{1,6}\s+", "", text)
        text = re.sub(r"\[(.+?)\]\(.+?\)", r"\1", text)
        return text.strip()

    def __init__(self):
        """Initialize todo handler with in-memory storage."""
        self._todos: Dict[str, TodoItem] = {}
        self._next_id = 1

    def write_todos(self, todos: List[str] | List[dict]) -> dict:
        """Create multiple todo items in a single call.

        Supports both formats:
        - List[str]: Simple string list ["Task 1", "Task 2"]
        - List[dict]: Deep Agent format [{"content": "Task 1", "status": "pending"}]

        Args:
            todos: List of todo titles/descriptions (str) or todo objects (dict)

        Returns:
            Result dict with success status and summary
        """
        if not todos:
            return {
                "success": False,
                "error": "No todos provided. 'todos' parameter must be a non-empty list.",
                "output": None,
            }

        if not isinstance(todos, list):
            return {
                "success": False,
                "error": f"'todos' must be a list. Got {type(todos).__name__}.",
                "output": None,
            }

        # Normalize to string list
        # Handle both List[str] and List[dict] formats (Deep Agent compatibility)
        normalized_todos = []
        for item in todos:
            if isinstance(item, str):
                normalized_todos.append(item)
            elif isinstance(item, dict):
                # Extract 'content', 'status', and 'activeForm' fields from Deep Agent's todo dict format
                content = item.get("content", "")
                status = item.get("status", "pending")
                active_form = item.get("activeForm", "")
                if content:
                    # Map Deep Agent status to internal status
                    status_mapping = {
                        "pending": "todo",
                        "in_progress": "doing",
                        "completed": "done",
                        "todo": "todo",
                        "doing": "doing",
                        "done": "done",
                    }
                    mapped_status = status_mapping.get(status, "todo")
                    # Store as tuple to preserve status and activeForm information
                    normalized_todos.append((content, mapped_status, active_form))
            else:
                # Skip invalid items
                continue

        if not normalized_todos:
            return {
                "success": False,
                "error": "No valid todos found in the list.",
                "output": None,
            }

        todos = normalized_todos

        # Check if this is a status-only update (same content, different statuses)
        # This avoids duplicate display when AI calls write_todos twice with same list
        if self._todos and self._is_status_only_update(normalized_todos):
            return self._apply_status_updates(normalized_todos)

        # Clear existing todos - write_todos replaces the entire list
        self._todos.clear()
        self._next_id = 1

        # Create all todos
        results = []
        created_count = 0
        failed_count = 0
        created_ids = []

        logger.debug(f"[TODO] write_todos called with {len(normalized_todos)} items")

        for i, todo_item in enumerate(normalized_todos, 1):
            # Handle both string and tuple formats
            if isinstance(todo_item, tuple):
                if len(todo_item) == 3:
                    todo_text, todo_status, todo_active_form = todo_item
                else:
                    todo_text, todo_status = todo_item
                    todo_active_form = ""
            else:
                todo_text = todo_item
                todo_status = "todo"
                todo_active_form = ""

            if not todo_text or not str(todo_text).strip():
                failed_count += 1
                results.append(f"  {i}. [SKIPPED] Empty todo")
                continue

            # Call create_todo for each item with correct status and activeForm
            result = self.create_todo(
                title=str(todo_text).strip(), status=todo_status, active_form=todo_active_form
            )

            if result.get("success"):
                todo_id = result.get("todo_id", "?")
                created_ids.append(todo_id)
                # Format with symbols based on status (no Rich markup - output goes to plain text)
                if todo_status == "done":
                    results.append(f"  ✓ {str(todo_text).strip()}")
                elif todo_status == "doing":
                    results.append(f"  ▶ {str(todo_text).strip()}")
                else:
                    results.append(f"  ○ {str(todo_text).strip()}")
                created_count += 1
            else:
                error = result.get("error", "Unknown error")
                results.append(f"  ✗ {error}")
                failed_count += 1

        # Build summary with instructive message for continuation
        summary_lines = [
            "Todos updated. Now proceed with the next action.",
            "",
            f"Created {created_count} todo(s) from {len(todos)} item(s):",
            "",
        ]
        summary_lines.extend(results)

        if failed_count > 0:
            summary_lines.append(f"\nWarning: {failed_count} todo(s) failed to create.")

        return {
            "success": True,
            "output": "\n".join(summary_lines),
            "created_count": created_count,
            "failed_count": failed_count,
            "todo_ids": created_ids,
        }

    def create_todo(
        self,
        title: str,
        status: str = "todo",
        active_form: str = "",
        log: str = "",
        expanded: bool = False,
    ) -> dict:
        """Create a new todo item.

        Args:
            title: Todo title/description
            status: Status ("todo", "doing", "done" OR "pending", "in_progress", "completed")
            active_form: Present continuous form for spinner display (e.g., "Running tests")
            log: Optional log/notes
            expanded: Whether to show expanded in UI

        Returns:
            Result dict with success status and todo ID
        """
        # Map Deep Agent statuses to internal statuses
        status_map = {
            "pending": "todo",
            "in_progress": "doing",
            "completed": "done",
        }

        # Normalize status
        normalized_status = status_map.get(status, status)

        # Validate status
        if normalized_status not in ["todo", "doing", "done"]:
            return {
                "success": False,
                "error": f"Invalid status '{status}'. Must be 'todo', 'doing', or 'done' (or 'pending', 'in_progress', 'completed').",
                "output": None,
            }

        # Strip markdown formatting from title and active_form
        title = self._strip_markdown(title)
        if active_form:
            active_form = self._strip_markdown(active_form)

        # Create todo with Deep Agent compatible ID format
        todo_id = f"todo-{self._next_id}"
        self._next_id += 1

        todo = TodoItem(
            id=todo_id,
            title=title,
            status=normalized_status,
            active_form=active_form,
            log=log,
            expanded=expanded,
        )

        self._todos[todo_id] = todo
        logger.debug(f"[TODO] Created: {todo_id} = {title[:40]}...")

        return {
            "success": True,
            "output": f"Created todo #{todo_id}: {title}",
            "todo_id": todo_id,
            "todo": asdict(todo),
        }

    def _find_todo(self, id: str | int) -> tuple[Optional[str], Optional[TodoItem]]:
        """Find a todo by ID, trying multiple matching strategies.

        Supports both 1-based indexing (Claude's default) and 0-based indexing.
        Also supports finding by title string, kebab-case slugs, and fuzzy matching.

        Args:
            id: Todo ID in formats: 1, "1", "todo-1", exact title, kebab-case slug

        Returns:
            Tuple of (actual_id, todo_item) or (None, None) if not found
        """
        # Convert to string first (handle both int and str inputs)
        id = str(id)

        # Empty string should return None
        if not id or not id.strip():
            return None, None

        # Try exact match first (no warning needed)
        if id in self._todos:
            return id, self._todos[id]

        # If numeric ID provided, try both 0-based and 1-based indexing
        if id.isdigit():
            numeric_id = int(id)

            # Try 0-based indexing first (Deep Agent format): "0" → "todo-1", "2" → "todo-3"
            one_based_id = numeric_id + 1
            todo_id = f"todo-{one_based_id}"
            if todo_id in self._todos:
                return todo_id, self._todos[todo_id]

            # Fallback to 1-based indexing (Claude uses 1-based): "1" → "todo-1"
            todo_id = f"todo-{numeric_id}"
            if todo_id in self._todos:
                return todo_id, self._todos[todo_id]

        # If "todo-X" provided, try numeric format
        if id.startswith("todo-"):
            numeric_id = id[5:]
            if numeric_id in self._todos:
                return numeric_id, self._todos[numeric_id]

        # If ":X" provided (colon format), treat as numeric 0-based index
        if id.startswith(":") and len(id) > 1:
            numeric_part = id[1:]
            if numeric_part.isdigit():
                # Convert ":1" → "todo-2" (0-based to 1-based)
                one_based_id = int(numeric_part) + 1
                todo_id = f"todo-{one_based_id}"
                if todo_id in self._todos:
                    return todo_id, self._todos[todo_id]

        # If "todo_X" provided (Deep Agent format with underscore), convert to "todo-X"
        if id.startswith("todo_"):
            numeric_part = id[5:]
            if numeric_part.isdigit():
                # Convert "todo_1" → "todo-1" (our internal format)
                internal_id = f"todo-{numeric_part}"
                if internal_id in self._todos:
                    return internal_id, self._todos[internal_id]

        # Try to find by title (case-sensitive exact match)
        for todo_id, todo in self._todos.items():
            if todo.title == id:
                return todo_id, todo

        # Try case-insensitive exact match
        id_lower = id.lower()
        for todo_id, todo in self._todos.items():
            if todo.title.lower() == id_lower:
                return todo_id, todo

        # Try kebab-case slug matching (e.g., "implement-basic-level" → "Implement basic level design...")
        # Convert kebab-case to words and try fuzzy matching
        if "-" in id:
            # Convert "implement-basic-level" → "implement basic level"
            slug_words = id.replace("-", " ").lower()
            for todo_id, todo in self._todos.items():
                title_lower = todo.title.lower()
                # Check if slug words appear at start of title
                if title_lower.startswith(slug_words):
                    return todo_id, todo
                # Check if all slug words appear in title (in order)
                if all(word in title_lower for word in slug_words.split()):
                    # Verify words appear in order
                    pos = 0
                    all_in_order = True
                    for word in slug_words.split():
                        idx = title_lower.find(word, pos)
                        if idx == -1:
                            all_in_order = False
                            break
                        pos = idx + len(word)
                    if all_in_order:
                        return todo_id, todo

        # Try partial matching - if id is contained in title
        for todo_id, todo in self._todos.items():
            if id_lower in todo.title.lower():
                return todo_id, todo

        return None, None

    def update_todo(
        self,
        id: str,
        title: Optional[str] = None,
        status: Optional[str] = None,
        active_form: Optional[str] = None,
        log: Optional[str] = None,
        expanded: Optional[bool] = None,
    ) -> dict:
        """Update an existing todo item.

        Args:
            id: Todo ID
            title: New title (optional)
            status: New status ("todo", "doing", "done" OR "pending", "in_progress", "completed") (optional)
            active_form: Present continuous form for spinner display (optional)
            log: New log/notes (optional)
            expanded: New expanded state (optional)

        Returns:
            Result dict with success status
        """
        logger.debug(f"[TODO] update_todo called: id={id}, status={status}")
        actual_id, todo = self._find_todo(id)
        if todo is None:
            logger.debug(f"[TODO] Todo not found: id={id}")
            # Build helpful error message with valid ID suggestions
            valid_ids = sorted(self._todos.keys())
            if valid_ids:
                ids_list = ", ".join(valid_ids)
                error_msg = (
                    f"Todo '{id}' not found. "
                    f"Valid IDs: {ids_list}. "
                    f"Use exact 'todo-N' format for best results."
                )
            else:
                error_msg = f"Todo '{id}' not found. No todos exist yet. Create todos with write_todos first."

            return {
                "success": False,
                "error": error_msg,
                "output": None,
            }

        logger.debug(f"[TODO] Found todo: actual_id={actual_id}, title={todo.title[:30]}...")
        old_status = todo.status

        # Update fields
        if title is not None:
            todo.title = self._strip_markdown(title)
        if status is not None:
            # Map Deep Agent statuses to internal statuses
            status_map = {
                "pending": "todo",
                "in_progress": "doing",
                "completed": "done",
            }

            # Normalize status
            normalized_status = status_map.get(status, status)

            if normalized_status not in ["todo", "doing", "done"]:
                return {
                    "success": False,
                    "error": f"Invalid status '{status}'. Must be 'todo', 'doing', or 'done' (or 'pending', 'in_progress', 'completed').",
                    "output": None,
                }
            todo.status = normalized_status

            # ENFORCEMENT: Ensure only one todo can be "doing" at a time
            if normalized_status == "doing":
                for other_id, other_todo in self._todos.items():
                    if other_id != actual_id and other_todo.status == "doing":
                        other_todo.status = "todo"

        if active_form is not None:
            todo.active_form = self._strip_markdown(active_form)
        if log is not None:
            todo.log = log
        if expanded is not None:
            todo.expanded = expanded

        todo.updated_at = datetime.now().isoformat()
        logger.debug(f"[TODO] Status changed: {old_status} -> {todo.status}")

        # Generate minimal status update
        if todo.status == "doing":
            output_lines = [f"▶ Now working on: {todo.title}"]
        elif todo.status == "done":
            output_lines = [f"Completed: {todo.title}"]
        else:
            output_lines = [f"⏸ Paused: {todo.title}"]

        return {
            "success": True,
            "output": "\n".join(output_lines),
            "todo": asdict(todo),
        }

    def complete_todo(self, id: str, log: Optional[str] = None) -> dict:
        """Mark a todo as complete.

        Args:
            id: Todo ID
            log: Optional final log entry

        Returns:
            Result dict with success status
        """
        logger.debug(f"[TODO] complete_todo called: id={id}")
        actual_id, todo = self._find_todo(id)
        if todo is None:
            # Build helpful error message with valid ID suggestions
            valid_ids = sorted(self._todos.keys())
            if valid_ids:
                ids_list = ", ".join(valid_ids)
                error_msg = (
                    f"Todo '{id}' not found. "
                    f"Valid IDs: {ids_list}. "
                    f"Use exact 'todo-N' format for best results."
                )
            else:
                error_msg = f"Todo '{id}' not found. No todos exist yet. Create todos with write_todos first."

            return {
                "success": False,
                "error": error_msg,
                "output": None,
            }
        old_status = todo.status
        todo.status = "done"
        logger.debug(
            f"[TODO] Completed: actual_id={actual_id}, status changed: {old_status} -> done"
        )

        if log:
            if todo.log:
                todo.log += f"\n{log}"
            else:
                todo.log = log

        todo.updated_at = datetime.now().isoformat()

        # Generate minimal completion output
        output_lines = [f"Completed: {todo.title}"]

        return {
            "success": True,
            "output": "\n".join(output_lines),
            "todo": asdict(todo),
        }

    def _format_todo_list_simple(self) -> list[str]:
        """Format todo list for display after updates.

        Returns:
            List of formatted todo lines with status indicators and strikethrough for completed items.
        """
        if not self._todos:
            return []

        lines = []
        status_order = {"doing": 0, "todo": 1, "done": 2}

        def extract_id_number(todo_id: str) -> int:
            """Extract numeric part from 'todo-X' format."""
            if todo_id.startswith("todo-"):
                return int(todo_id[5:])
            return int(todo_id)

        sorted_todos = sorted(
            self._todos.values(),
            key=lambda t: (status_order.get(t.status, 3), extract_id_number(t.id)),
        )

        for todo in sorted_todos:
            if todo.status == "done":
                # Completed: green with strikethrough
                lines.append(f"  [green]✓ ~~{todo.title}~~[/green]")
            elif todo.status == "doing":
                # In progress: yellow
                lines.append(f"  [yellow]▶ {todo.title}[/yellow]")
            else:
                # Pending: cyan
                lines.append(f"  [cyan]○ {todo.title}[/cyan]")

        return lines

    def complete_and_activate_next(self, id: str, log: Optional[str] = None) -> dict:
        """Complete a todo and automatically activate the next pending one.

        This is an atomic operation that:
        1. Marks the specified todo as completed
        2. Deactivates any other active todos
        3. Activates the next pending todo (if any)

        Args:
            id: Todo ID to complete
            log: Optional completion log message

        Returns:
            Result dict with success status and formatted output
        """
        actual_id, todo = self._find_todo(id)
        if not todo:
            return {
                "success": False,
                "error": f"Todo #{id} not found",
                "output": None,
            }

        # Mark current todo as completed
        todo.status = "done"
        if log is not None:
            todo.log = log
        todo.updated_at = datetime.now().isoformat()

        # Find the next pending todo to activate
        next_todo = None
        pending_todos = [t for t in self._todos.values() if t.status == "todo"]

        if pending_todos:
            # Sort by original creation order
            def extract_id_number(todo_id: str) -> int:
                if todo_id.startswith("todo-"):
                    return int(todo_id[5:])
                return int(todo_id)

            next_todo = min(pending_todos, key=lambda t: extract_id_number(t.id))
            next_todo.status = "doing"

        # Generate output
        output_lines = [f"Completed: {todo.title}"]
        if next_todo:
            output_lines.append(f"▶ Now working on: {next_todo.title}")
        else:
            output_lines.append("All todos completed!")

        return {
            "success": True,
            "output": "\n".join(output_lines),
            "todo": asdict(todo),
        }

    def list_todos(self) -> dict:
        """List all todos with formatted display.

        Returns:
            Result dict with success status and formatted output
        """
        if not self._todos:
            return {
                "success": True,
                "output": "No todos found. Create one with create_todo().",
                "todos": [],
            }

        # Sort by status (doing -> todo -> done) then by ID
        status_order = {"doing": 0, "todo": 1, "done": 2}

        def extract_id_number(todo_id: str) -> int:
            """Extract numeric part from 'todo-X' format."""
            if todo_id.startswith("todo-"):
                return int(todo_id[5:])
            return int(todo_id)

        sorted_todos = sorted(
            self._todos.values(),
            key=lambda t: (status_order.get(t.status, 3), extract_id_number(t.id)),
        )

        lines = []
        for todo in sorted_todos:
            if todo.status == "done":
                lines.append(f"✓ [{todo.id}] {todo.title}")
            elif todo.status == "doing":
                lines.append(f"▶ [{todo.id}] {todo.title}")
            else:
                lines.append(f"○ [{todo.id}] {todo.title}")
        output = "\n".join(lines) if lines else "No todos."

        return {
            "success": True,
            "output": output,
            "todos": [asdict(t) for t in sorted_todos],
            "count": len(self._todos),
        }

    def _is_status_only_update(self, new_todos: list) -> bool:
        """Check if new todos have same content as existing, just different statuses.

        This detects when write_todos is called with the same todo list but only
        status changes (e.g., marking one item as in_progress).

        Args:
            new_todos: Normalized todo list (list of strings or tuples)

        Returns:
            True if content matches existing todos and only statuses differ
        """
        if len(new_todos) != len(self._todos):
            return False

        existing_titles = [t.title for t in self._todos.values()]
        new_titles = []
        for item in new_todos:
            if isinstance(item, tuple):
                new_titles.append(item[0])
            else:
                new_titles.append(str(item))

        return existing_titles == new_titles

    def _apply_status_updates(self, new_todos: list) -> dict:
        """Update only the statuses without recreating todos.

        This is called when write_todos detects a status-only update,
        avoiding the overhead of clearing and recreating all todos.

        Args:
            new_todos: Normalized todo list with new statuses

        Returns:
            Result dict with minimal output
        """
        updated = []
        for i, (todo_id, todo) in enumerate(self._todos.items()):
            if i < len(new_todos):
                item = new_todos[i]
                if isinstance(item, tuple) and len(item) >= 2:
                    new_status = item[1]
                    new_active_form = item[2] if len(item) >= 3 else ""
                    if todo.status != new_status:
                        todo.status = new_status
                        if new_active_form:
                            todo.active_form = self._strip_markdown(new_active_form)
                        todo.updated_at = datetime.now().isoformat()
                        updated.append(todo.title)

                        # ENFORCEMENT: Ensure only one todo can be "doing" at a time
                        if new_status == "doing":
                            for other_id, other_todo in self._todos.items():
                                if other_id != todo_id and other_todo.status == "doing":
                                    other_todo.status = "todo"

        if updated:
            # Return minimal output - just note the update
            return {
                "success": True,
                "output": (
                    f"▶ Now working on: {updated[0]}"
                    if len(updated) == 1
                    else f"Updated {len(updated)} todos"
                ),
                "updated_count": len(updated),
            }
        return {
            "success": True,
            "output": "No changes needed",
            "updated_count": 0,
        }

    def get_active_todo_message(self) -> Optional[str]:
        """Get the activeForm text of the current in_progress todo.

        Returns:
            The active_form string if there's a todo in "doing" status with active_form set,
            otherwise None.
        """
        for todo in self._todos.values():
            if todo.status == "doing" and todo.active_form:
                return todo.active_form
        return None

    def has_todos(self) -> bool:
        """Check if any todos exist.

        Returns:
            True if any todos have been created, False otherwise.
        """
        return bool(self._todos)

    def has_incomplete_todos(self) -> bool:
        """Check if any todos remain incomplete.

        Returns:
            True if any todo has status != 'done', False otherwise.
        """
        return any(t.status != "done" for t in self._todos.values())

    def get_incomplete_todos(self) -> List["TodoItem"]:
        """Get all todos that are not done.

        Returns:
            List of TodoItem objects with status != 'done'.
        """
        return [t for t in self._todos.values() if t.status != "done"]
