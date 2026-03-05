"""File change tracking models."""

from datetime import datetime
from enum import Enum
from typing import Optional
from uuid import uuid4

from pydantic import BaseModel, Field


class FileChangeType(str, Enum):
    """Types of file changes."""
    CREATED = "created"
    MODIFIED = "modified"
    DELETED = "deleted"
    RENAMED = "renamed"


class FileChange(BaseModel):
    """Represents a file change within a session."""

    id: str = Field(default_factory=lambda: uuid4().hex[:8])
    type: FileChangeType
    file_path: str
    old_path: Optional[str] = None  # For renames
    timestamp: datetime = Field(default_factory=datetime.now)
    lines_added: int = 0
    lines_removed: int = 0
    tool_call_id: Optional[str] = None
    session_id: Optional[str] = None
    description: Optional[str] = None  # Human-readable description

    class Config:
        json_encoders = {
            datetime: lambda v: v.isoformat()
        }

    @classmethod
    def from_tool_result(cls, tool_name: str, tool_args: dict, tool_result: dict, session_id: str) -> "FileChange":
        """Create a FileChange from tool execution result."""
        from swecli.models.operation import WriteResult, EditResult

        if tool_name == "write_file":
            write_result = WriteResult.from_dict(tool_result)
            if write_result.success:
                return cls(
                    type=FileChangeType.CREATED,
                    file_path=tool_args.get("file_path", ""),
                    lines_added=len(tool_args.get("content", "").split("\n")),
                    session_id=session_id,
                    description=f"Created {tool_args.get('file_path', '').split('/')[-1]}"
                )

        elif tool_name == "edit_file":
            edit_result = EditResult.from_dict(tool_result)
            if edit_result.success:
                return cls(
                    type=FileChangeType.MODIFIED,
                    file_path=tool_args.get("file_path", ""),
                    lines_added=edit_result.lines_added,
                    lines_removed=edit_result.lines_removed,
                    session_id=session_id,
                    description=f"Modified {tool_args.get('file_path', '').split('/')[-1]} (+{edit_result.lines_added} -{edit_result.lines_removed})"
                )

        # For other tools, create generic changes
        return cls(
            type=FileChangeType.MODIFIED,
            file_path=tool_args.get("file_path", tool_args.get("path", "")),
            session_id=session_id,
            description=f"Changed by {tool_name}"
        )

    def get_file_icon(self) -> str:
        """Get file icon based on type and extension."""
        if self.type == FileChangeType.CREATED:
            return "+"
        elif self.type == FileChangeType.MODIFIED:
            return "~"
        elif self.type == FileChangeType.DELETED:
            return "-"
        elif self.type == FileChangeType.RENAMED:
            return ">"
        return "~"

    def get_status_color(self) -> str:
        """Get status color for UI display."""
        if self.type == FileChangeType.CREATED:
            return "green"
        elif self.type == FileChangeType.MODIFIED:
            return "blue"
        elif self.type == FileChangeType.DELETED:
            return "red"
        elif self.type == FileChangeType.RENAMED:
            return "orange"
        return "gray"

    def get_change_summary(self) -> str:
        """Get human-readable change summary."""
        if self.type == FileChangeType.CREATED:
            return "New file"
        elif self.type == FileChangeType.MODIFIED:
            if self.lines_added > 0 and self.lines_removed > 0:
                return f"+{self.lines_added} -{self.lines_removed}"
            elif self.lines_added > 0:
                return f"+{self.lines_added}"
            elif self.lines_removed > 0:
                return f"-{self.lines_removed}"
            return "Modified"
        elif self.type == FileChangeType.DELETED:
            return "Deleted"
        elif self.type == FileChangeType.RENAMED:
            return f"Renamed → {self.file_path}"
        return "Changed"