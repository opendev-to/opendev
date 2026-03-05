"""Utilities for cleaning and interpreting LLM responses."""

from __future__ import annotations

import re
from typing import Optional


class ResponseCleaner:
    """Strips provider-specific tokens from model responses."""

    CLEANUP_PATTERNS = (
        (re.compile(r"<\|[^|]+\|>"), ""),  # Match chat template tokens like <|im_end|>, <|im_user|>, etc.
        (re.compile(r"</?tool_call>"), ""),
        (re.compile(r"</?tool_response>"), ""),
        (re.compile(r"<function=[^>]+>"), ""),
        (re.compile(r"</?parameter[^>]*>"), ""),
    )

    def clean(self, content: Optional[str]) -> Optional[str]:
        """Return the sanitized content string."""
        if not content:
            return content

        cleaned = content
        for pattern, replacement in self.CLEANUP_PATTERNS:
            cleaned = pattern.sub(replacement, cleaned)

        return cleaned.strip()
