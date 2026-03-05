"""Token counting utilities used for ACE context summaries."""

from __future__ import annotations

from typing import List

import tiktoken

from swecli.models.message import ChatMessage, ToolCall


class ContextTokenMonitor:
    """Monitor and count tokens using tiktoken for session context."""

    def __init__(self, model: str = "gpt-4") -> None:
        """Initialize with tiktoken encoding."""
        try:
            self.encoding = tiktoken.encoding_for_model(model)
        except KeyError:
            self.encoding = tiktoken.get_encoding("cl100k_base")

    def count_tokens(self, text: str) -> int:
        """Count tokens in text."""
        return len(self.encoding.encode(text))

    def count_message_tokens(self, message: ChatMessage) -> int:
        """Count tokens in a complete message, including tool calls."""
        total = self.count_tokens(message.content)
        for tool_call in message.tool_calls:
            total += self._count_tool_call_tokens(tool_call)
        return total

    def _count_tool_call_tokens(self, tool_call: ToolCall) -> int:
        """Count tokens in a tool call."""
        total = self.count_tokens(tool_call.name)
        total += self.count_tokens(str(tool_call.parameters))
        if tool_call.result:
            total += self.count_tokens(str(tool_call.result))
        return total

    def count_messages_total(self, messages: List[ChatMessage]) -> int:
        """Count total tokens across all messages."""
        return sum(self.count_message_tokens(msg) for msg in messages)
