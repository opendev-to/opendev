"""Approval workflow interfaces shared across core components."""

from __future__ import annotations

from typing import Union, TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from swecli.core.runtime.approval import ApprovalResult
    from swecli.models.operation import Operation


class ApprovalManagerInterface(Protocol):
    """Protocol exposing the approval workflow used by core services."""

    auto_approve_remaining: bool

    def request_approval(
        self,
        operation: "Operation",
        preview: str,
        *,
        command: Union[str, None] = None,
        working_dir: Union[str, None] = None,
    ) -> "ApprovalResult":
        """Ask the user to approve or reject an operation."""

    def reset_auto_approve(self) -> None:
        """Clear any auto-approval state accumulated in the session."""
