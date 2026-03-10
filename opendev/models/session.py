"""Session management models."""

import json
import logging
import re as _re
from datetime import datetime
from typing import TYPE_CHECKING, Any, Optional
from uuid import uuid4

from pydantic import BaseModel, ConfigDict, Field

from opendev.models.message import ChatMessage
from opendev.models.file_change import FileChange, FileChangeType

logger = logging.getLogger(__name__)

if TYPE_CHECKING:
    from opendev.core.context_engineering.memory import Playbook


class SessionMetadata(BaseModel):
    """Session metadata for listing and searching."""

    id: str
    created_at: datetime
    updated_at: datetime
    message_count: int
    total_tokens: int
    title: Optional[str] = None
    summary: Optional[str] = None
    tags: list[str] = Field(default_factory=list)
    working_directory: Optional[str] = None
    has_session_model: bool = False
    owner_id: Optional[str] = None

    # Summary stats (populated from Session computed properties)
    summary_additions: int = 0
    summary_deletions: int = 0
    summary_files: int = 0

    # Multi-channel fields
    channel: str = "cli"  # "telegram", "whatsapp", "web", "cli"
    channel_user_id: str = ""  # Channel-specific user identifier
    thread_id: Optional[str] = None  # Thread ID for threaded channels


class Session(BaseModel):
    """Represents a conversation session.

    The session uses ACE (Agentic Context Engine) Playbook for storing
    learned strategies extracted from tool executions.

    Multi-channel support: Sessions can be associated with different channels
    (CLI, Web, Telegram, WhatsApp) and track channel-specific delivery context.
    """

    id: str = Field(default_factory=lambda: uuid4().hex[:12])
    created_at: datetime = Field(default_factory=datetime.now)
    updated_at: datetime = Field(default_factory=datetime.now)
    messages: list[ChatMessage] = Field(default_factory=list)
    context_files: list[str] = Field(default_factory=list)
    working_directory: Optional[str] = None
    metadata: dict[str, Any] = Field(default_factory=dict)
    playbook: Optional[dict] = Field(default_factory=dict)  # Serialized ACE Playbook
    file_changes: list[FileChange] = Field(
        default_factory=list
    )  # Track file changes in this session

    # Multi-channel fields
    channel: str = "cli"  # "telegram", "whatsapp", "web", "cli"
    chat_type: str = "direct"  # "direct", "group"
    channel_user_id: str = ""  # Channel-specific user identifier (@user, +phone, tg:123)
    thread_id: Optional[str] = None  # For threaded channels (Telegram topics, Slack threads)
    delivery_context: dict[str, Any] = Field(default_factory=dict)  # Where to send responses
    last_activity: Optional[datetime] = None  # Last message timestamp (for reset policies)
    workspace_confirmed: bool = False  # Has user selected workspace for this channel session?
    owner_id: Optional[str] = None
    parent_id: Optional[str] = None  # ID of parent session (if forked)
    subagent_sessions: dict[str, str] = Field(
        default_factory=dict
    )  # tool_call_id -> child session_id
    time_archived: Optional[datetime] = None
    slug: Optional[str] = None

    model_config = ConfigDict(json_encoders={datetime: lambda v: v.isoformat()})

    def get_playbook(self) -> "Playbook":
        """Get the session's ACE playbook, creating if needed.

        Returns:
            ACE Playbook instance loaded from session data
        """
        from opendev.core.context_engineering.memory import Playbook

        if not self.playbook:
            return Playbook()

        # Load from serialized dict
        return Playbook.from_dict(self.playbook)

    def update_playbook(self, playbook: "Playbook") -> None:
        """Update the session's ACE playbook.

        Args:
            playbook: ACE Playbook instance to save
        """
        self.playbook = playbook.to_dict()
        self.updated_at = datetime.now()

    @property
    def summary_additions(self) -> int:
        """Total lines added across all file changes."""
        return sum(fc.lines_added for fc in self.file_changes)

    @property
    def summary_deletions(self) -> int:
        """Total lines removed across all file changes."""
        return sum(fc.lines_removed for fc in self.file_changes)

    @property
    def summary_files(self) -> int:
        """Number of unique files changed."""
        return len(set(fc.file_path for fc in self.file_changes))

    def archive(self) -> None:
        """Soft-archive this session."""
        self.time_archived = datetime.now()
        self.updated_at = datetime.now()

    def unarchive(self) -> None:
        """Restore an archived session."""
        self.time_archived = None
        self.updated_at = datetime.now()

    @property
    def is_archived(self) -> bool:
        """Check if session is archived."""
        return self.time_archived is not None

    def generate_slug(self, title: Optional[str] = None) -> str:
        """Generate URL-friendly slug from title."""
        text = title or self.metadata.get("title", "")
        if not text:
            return self.id[:8]
        # Lowercase, replace non-alnum with hyphens, collapse multiple hyphens
        slug = _re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
        # Limit length
        slug = slug[:50].rstrip("-")
        return slug or self.id[:8]

    def add_message(self, message: ChatMessage) -> bool:
        """Add a message to the session after validation.

        Returns:
            True if the message was added, False if rejected.
        """
        from opendev.models.message_validator import validate_message

        verdict = validate_message(message)
        if not verdict.is_valid:
            logger.warning("Rejected message (role=%s): %s", message.role.value, verdict.reason)
            return False
        self.messages.append(message)
        self.updated_at = datetime.now()
        return True

    def add_file_change(self, file_change: FileChange) -> None:
        """Add a file change to the session."""
        # Check if this is a modification of an existing file
        for i, existing_change in enumerate(self.file_changes):
            if (
                existing_change.file_path == file_change.file_path
                and existing_change.type == FileChangeType.MODIFIED
                and file_change.type == FileChangeType.MODIFIED
            ):
                # Merge with existing change
                self.file_changes[i].lines_added += file_change.lines_added
                self.file_changes[i].lines_removed += file_change.lines_removed
                self.file_changes[i].timestamp = file_change.timestamp
                self.file_changes[i].description = file_change.description
                return

        # Remove any previous change for the same file (for non-modifications)
        self.file_changes = [
            fc for fc in self.file_changes if fc.file_path != file_change.file_path
        ]

        # Add the new change
        file_change.session_id = self.id
        self.file_changes.append(file_change)
        self.updated_at = datetime.now()

    def get_file_changes_summary(self) -> dict:
        """Get a summary of file changes in this session."""
        created = len([fc for fc in self.file_changes if fc.type == FileChangeType.CREATED])
        modified = len([fc for fc in self.file_changes if fc.type == FileChangeType.MODIFIED])
        deleted = len([fc for fc in self.file_changes if fc.type == FileChangeType.DELETED])
        renamed = len([fc for fc in self.file_changes if fc.type == FileChangeType.RENAMED])
        total_lines_added = sum(fc.lines_added for fc in self.file_changes)
        total_lines_removed = sum(fc.lines_removed for fc in self.file_changes)

        return {
            "total": len(self.file_changes),
            "created": created,
            "modified": modified,
            "deleted": deleted,
            "renamed": renamed,
            "total_lines_added": total_lines_added,
            "total_lines_removed": total_lines_removed,
            "net_lines": total_lines_added - total_lines_removed,
        }

    def total_tokens(self) -> int:
        """Calculate total token count."""
        return sum(msg.token_estimate() for msg in self.messages)

    def get_metadata(self) -> SessionMetadata:
        """Get session metadata."""
        return SessionMetadata(
            id=self.id,
            created_at=self.created_at,
            updated_at=self.updated_at,
            message_count=len(self.messages),
            total_tokens=self.total_tokens(),
            title=self.metadata.get("title"),
            summary=self.metadata.get("summary"),
            tags=self.metadata.get("tags", []),
            working_directory=self.working_directory,
            summary_additions=self.summary_additions,
            summary_deletions=self.summary_deletions,
            summary_files=self.summary_files,
            channel=self.channel,
            channel_user_id=self.channel_user_id,
            thread_id=self.thread_id,
        )

    def to_api_messages(self, window_size: Optional[int] = None) -> list[dict[str, str]]:
        """Convert to API-compatible message format.

        Args:
            window_size: If provided, only include last N interactions (user+assistant pairs).
                        For ACE compatibility, use small window (1 interaction) or none.

        Returns:
            List of API messages with tool_calls and concise result summaries.

        Note:
            Tool results use concise summaries (e.g., "✓ Read file (100 lines)")
            instead of full results to prevent context bloat.
        """
        # Select messages based on window size
        messages_to_convert = self.messages

        if window_size is not None and len(self.messages) > 0:
            # Count interactions (user+assistant pairs) from the end
            interaction_count = 0
            cutoff_index = 0  # Default: include all messages

            # Walk backwards counting user messages (each starts an interaction)
            for i in range(len(self.messages) - 1, -1, -1):
                if self.messages[i].role.value == "user":
                    interaction_count += 1
                    if interaction_count > window_size:
                        cutoff_index = i + 1  # Don't include this user message
                        break

            messages_to_convert = self.messages[cutoff_index:]

        # Convert selected messages to API format
        result = []
        for msg in messages_to_convert:
            raw_content = None
            if msg.metadata and "raw_content" in msg.metadata:
                raw_content = msg.metadata["raw_content"]

            api_msg = {
                "role": msg.role.value,
                "content": raw_content if raw_content is not None else msg.content,
            }
            # Include tool_calls if present
            if msg.tool_calls:
                api_msg["tool_calls"] = [
                    {
                        "id": tc.id,
                        "type": "function",
                        "function": {"name": tc.name, "arguments": json.dumps(tc.parameters)},
                    }
                    for tc in msg.tool_calls
                ]
                # Add the assistant message with tool_calls
                result.append(api_msg)

                # Add tool result messages for each tool call
                # Use concise summaries instead of full results to prevent context bloat
                for tc in msg.tool_calls:
                    # Prefer result_summary (concise 1-2 line summary)
                    if tc.result_summary:
                        tool_content = tc.result_summary
                    else:
                        # Fallback: generate summary on-the-fly if not available
                        if tc.error:
                            tool_content = f"❌ Error: {str(tc.error)[:200]}"
                        elif tc.result:
                            result_str = str(tc.result)
                            if len(result_str) > 200:
                                tool_content = f"✓ Success ({len(result_str)} chars)"
                            else:
                                tool_content = f"✓ {result_str}"
                        else:
                            tool_content = "✓ Success"

                    result.append({"role": "tool", "tool_call_id": tc.id, "content": tool_content})
            else:
                result.append(api_msg)

        # Self-heal sessions loaded from disk with corrupted message sequences.
        # This is for backward compat only — new sessions use ValidatedMessageList
        # which enforces invariants at write time. Legacy sessions saved before the
        # validator may still have corrupted sequences on disk.
        from opendev.core.context_engineering.message_pair_validator import (
            MessagePairValidator,
        )

        result, _ = MessagePairValidator.repair(result)
        return result
