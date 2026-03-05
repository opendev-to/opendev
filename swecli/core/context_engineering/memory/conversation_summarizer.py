"""Conversation summarization for thinking context.

Implements episodic memory through LLM-generated conversation summaries.
Uses incremental summarization - only sends new messages to LLM and
merges them with the existing summary to save tokens.
"""

from typing import Optional, List, Dict, Any, Callable
from dataclasses import dataclass

from swecli.core.agents.prompts.loader import load_prompt


@dataclass
class ConversationSummary:
    """Cached conversation summary."""

    summary: str
    message_count: int  # Total number of messages when summary was created
    last_summarized_index: int  # Index in filtered messages list up to which we've summarized


class ConversationSummarizer:
    """Generates and caches conversation summaries for episodic memory.

    Uses LLM to create concise summaries of conversation history.
    Implements incremental summarization - only new messages since the
    last summary are sent to the LLM, merged with the previous summary.
    """

    def __init__(
        self,
        regenerate_threshold: int = 5,  # Regenerate after N new messages
        max_summary_length: int = 500,
        exclude_last_n: int = 6,  # Don't summarize recent messages
    ):
        self._cache: Optional[ConversationSummary] = None
        self._regenerate_threshold = regenerate_threshold
        self._max_summary_length = max_summary_length
        self._exclude_last_n = exclude_last_n

    def needs_regeneration(self, current_message_count: int) -> bool:
        """Check if summary needs regeneration."""
        if self._cache is None:
            return True

        messages_since_update = current_message_count - self._cache.message_count
        return messages_since_update >= self._regenerate_threshold

    def get_cached_summary(self) -> Optional[str]:
        """Get cached summary if available."""
        return self._cache.summary if self._cache else None

    def generate_summary(
        self,
        messages: List[Dict[str, Any]],
        llm_caller: Callable[[List[Dict[str, Any]], Any], Dict[str, Any]],
    ) -> str:
        """Generate summary of conversation history using incremental approach.

        Only sends new messages since the last summary to the LLM,
        along with the previous summary for context merging.

        Args:
            messages: Full conversation messages
            llm_caller: Function that takes (messages, task_monitor) and returns response

        Returns:
            Summary string
        """
        # Filter out system messages
        filtered = [m for m in messages if m.get("role") != "system"]

        # Calculate the end index (excluding last N messages for short-term memory)
        end_index = len(filtered) - self._exclude_last_n
        if end_index <= 0:
            # Not enough history to summarize
            return self._cache.summary if self._cache else ""

        # Determine which messages are new
        if self._cache:
            new_start = self._cache.last_summarized_index
            previous_summary = self._cache.summary
        else:
            new_start = 0
            previous_summary = ""

        # Extract only new messages
        new_messages = filtered[new_start:end_index]

        if not new_messages:
            # No new messages to summarize
            return self._cache.summary if self._cache else ""

        # Build prompt with both placeholders
        prompt_template = load_prompt("memory/conversation_summary_prompt")
        prompt = prompt_template.format(
            previous_summary=previous_summary if previous_summary else "(No previous summary)",
            new_messages=self._format_conversation(new_messages),
        )

        # Call LLM for summary
        summary_messages = [
            {
                "role": "system",
                "content": "You are a helpful assistant that summarizes conversations concisely.",
            },
            {"role": "user", "content": prompt},
        ]

        try:
            response = llm_caller(summary_messages, None)
            if response.get("success"):
                summary = response.get("content", "")[: self._max_summary_length]

                # Cache the result with updated index
                self._cache = ConversationSummary(
                    summary=summary,
                    message_count=len(messages),
                    last_summarized_index=end_index,
                )

                return summary
        except Exception:
            pass

        return self._cache.summary if self._cache else ""

    def _format_conversation(self, messages: List[Dict[str, Any]]) -> str:
        """Format messages into readable conversation text."""
        lines = []
        for msg in messages:
            role = msg.get("role", "").upper()
            content = msg.get("content", "")

            if role == "USER":
                lines.append(f"User: {content[:200]}")
            elif role == "ASSISTANT":
                tool_calls = msg.get("tool_calls", [])
                if tool_calls:
                    tool_names = [tc["function"]["name"] for tc in tool_calls]
                    lines.append(f"Assistant: [Called tools: {', '.join(tool_names)}]")
                elif content:
                    lines.append(f"Assistant: {content[:200]}")
            elif role == "TOOL":
                # Just note that tool executed, don't include output
                lines.append("Tool: [result received]")

        return "\n".join(lines)

    def clear_cache(self) -> None:
        """Clear the cached summary."""
        self._cache = None

    def to_dict(self) -> Optional[Dict[str, Any]]:
        """Serialize cache for session persistence."""
        if not self._cache:
            return None
        return {
            "summary": self._cache.summary,
            "message_count": self._cache.message_count,
            "last_summarized_index": self._cache.last_summarized_index,
        }

    def load_from_dict(self, data: Optional[Dict[str, Any]]) -> None:
        """Load cache from session persistence."""
        if not data:
            self._cache = None
            return
        self._cache = ConversationSummary(
            summary=data.get("summary", ""),
            message_count=data.get("message_count", 0),
            last_summarized_index=data.get("last_summarized_index", 0),
        )
