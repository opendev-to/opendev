"""Tool for creating new files."""

from pathlib import Path
from typing import Optional, TYPE_CHECKING

from swecli.models.config import AppConfig
from swecli.models.operation import WriteResult, Operation
from swecli.core.context_engineering.tools.implementations.base import BaseTool

if TYPE_CHECKING:
    from swecli.core.runtime.task_monitor import TaskMonitor


class WriteTool(BaseTool):
    """Tool for creating new files with permission checking."""

    @property
    def name(self) -> str:
        """Tool name."""
        return "write_file"

    @property
    def description(self) -> str:
        """Tool description."""
        return "Create a new file with specified content"

    def __init__(self, config: AppConfig, working_dir: Path):
        """Initialize write tool.

        Args:
            config: Application configuration
            working_dir: Working directory for operations
        """
        self.config = config
        self.working_dir = working_dir

    def write_file(
        self,
        file_path: str,
        content: str,
        create_dirs: bool = True,
        dry_run: bool = False,
        operation: Optional[Operation] = None,
        task_monitor: Optional["TaskMonitor"] = None,
    ) -> WriteResult:
        """Create a new file with content.

        Args:
            file_path: Path to file to create (relative or absolute)
            content: Content to write to file
            create_dirs: Create parent directories if needed
            dry_run: If True, don't actually write file
            operation: Operation object for tracking
            task_monitor: Optional task monitor for interrupt checking

        Returns:
            WriteResult with operation details

        Raises:
            FileExistsError: If file already exists
            PermissionError: If write is not permitted
        """
        # Check for interrupt before starting
        if task_monitor and task_monitor.should_interrupt():
            error = "Interrupted before write"
            if operation:
                operation.mark_failed(error)
            return WriteResult(
                success=False,
                file_path=file_path,
                created=False,
                size=0,
                error=error,
                operation_id=operation.id if operation else None,
                interrupted=True,
            )

        # Resolve path
        path = self._resolve_path(file_path)

        # Check if file already exists
        if path.exists():
            error = f"File already exists: {path}"
            if operation:
                operation.mark_failed(error)
            return WriteResult(
                success=False,
                file_path=str(path),
                created=False,
                size=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

        # Check write permissions
        if not self.config.permissions.file_write.is_allowed(str(path)):
            error = f"Writing to {path} is not permitted by configuration"
            if operation:
                operation.mark_failed(error)
            return WriteResult(
                success=False,
                file_path=str(path),
                created=False,
                size=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

        # Dry run - don't actually write
        if dry_run:
            return WriteResult(
                success=True,
                file_path=str(path),
                created=False,
                size=len(content.encode("utf-8")),
                operation_id=operation.id if operation else None,
            )

        try:
            # Mark operation as executing
            if operation:
                operation.mark_executing()

            # Create parent directories if needed
            if create_dirs:
                path.parent.mkdir(parents=True, exist_ok=True)

            # Write file
            with open(path, "w", encoding="utf-8") as f:
                f.write(content)

            # Get file size
            size = path.stat().st_size

            # Mark operation as successful
            if operation:
                operation.mark_success()

            return WriteResult(
                success=True,
                file_path=str(path),
                created=True,
                size=size,
                operation_id=operation.id if operation else None,
            )

        except Exception as e:
            error = f"Failed to write file: {str(e)}"
            if operation:
                operation.mark_failed(error)
            return WriteResult(
                success=False,
                file_path=str(path),
                created=False,
                size=0,
                error=error,
                operation_id=operation.id if operation else None,
            )

    def execute(self, **kwargs) -> WriteResult:
        """Execute the tool.

        Args:
            **kwargs: Arguments for write_file

        Returns:
            WriteResult
        """
        return self.write_file(**kwargs)

    def _resolve_path(self, path: str) -> Path:
        """Resolve a path relative to working directory.

        Args:
            path: Path string (relative or absolute)

        Returns:
            Resolved Path object
        """
        p = Path(path)
        if p.is_absolute():
            return p
        return (self.working_dir / p).resolve()

    def preview_write(self, file_path: str, content: str) -> str:
        """Generate a preview of the file write operation.

        Args:
            file_path: Path to file
            content: Content to write

        Returns:
            Formatted preview string
        """
        path = self._resolve_path(file_path)
        lines = content.split("\n")
        preview_lines = lines[:20]  # Show first 20 lines
        truncated = len(lines) > 20

        preview = f"Create: {path}\n"
        preview += f"Size: {len(content)} bytes\n"
        preview += f"Lines: {len(lines)}\n"
        preview += "\nContent:\n"
        preview += "─" * 50 + "\n"
        for i, line in enumerate(preview_lines, 1):
            preview += f"{i:4d} │ {line}\n"

        if truncated:
            preview += "     │ ...\n"
            preview += f"     │ ({len(lines) - 20} more lines)\n"

        preview += "─" * 50 + "\n"

        return preview
