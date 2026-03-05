"""Operation models for tracking actions and results."""

from datetime import datetime
from enum import Enum
from typing import Any, Optional

from pydantic import BaseModel, ConfigDict, Field


class OperationType(str, Enum):
    """Type of operation."""

    FILE_WRITE = "file_write"
    FILE_EDIT = "file_edit"
    FILE_DELETE = "file_delete"
    BASH_EXECUTE = "bash_execute"


class OperationStatus(str, Enum):
    """Status of operation."""

    PENDING = "pending"
    APPROVED = "approved"
    EXECUTING = "executing"
    SUCCESS = "success"
    FAILED = "failed"
    CANCELLED = "cancelled"


class Operation(BaseModel):
    """Represents a single operation to be performed."""

    id: str = Field(default_factory=lambda: datetime.now().strftime("%Y%m%d%H%M%S%f"))
    type: OperationType
    status: OperationStatus = OperationStatus.PENDING
    target: str  # File path, command, etc.
    parameters: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime = Field(default_factory=datetime.now)
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    approved: bool = False
    error: Optional[str] = None

    model_config = ConfigDict(json_encoders={datetime: lambda v: v.isoformat()})

    def mark_executing(self) -> None:
        """Mark operation as executing."""
        self.status = OperationStatus.EXECUTING
        self.started_at = datetime.now()

    def mark_success(self) -> None:
        """Mark operation as successful."""
        self.status = OperationStatus.SUCCESS
        self.completed_at = datetime.now()

    def mark_failed(self, error: str) -> None:
        """Mark operation as failed."""
        self.status = OperationStatus.FAILED
        self.completed_at = datetime.now()
        self.error = error


class WriteResult(BaseModel):
    """Result of a file write operation."""

    success: bool
    file_path: str
    created: bool
    size: int  # File size in bytes
    error: Optional[str] = None
    operation_id: Optional[str] = None
    interrupted: bool = False  # True if operation was interrupted


class EditResult(BaseModel):
    """Result of a file edit operation."""

    success: bool
    file_path: str
    lines_added: int
    lines_removed: int
    backup_path: Optional[str] = None
    error: Optional[str] = None
    operation_id: Optional[str] = None
    diff: Optional[str] = None  # Diff preview for the edit
    interrupted: bool = False  # True if operation was interrupted


class BashResult(BaseModel):
    """Result of a bash command execution."""

    success: bool
    command: str
    exit_code: int
    stdout: str
    stderr: str
    duration: float  # Seconds
    error: Optional[str] = None
    operation_id: Optional[str] = None
    background_task_id: Optional[str] = None  # ID if running as background task
