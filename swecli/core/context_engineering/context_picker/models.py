"""Data models for traceable context selection.

This module defines the core data structures for the ContextPicker system,
providing transparency into why each piece of context is included in LLM calls.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Optional


class ContextCategory(Enum):
    """Category of context piece for organization and filtering."""
    
    SYSTEM_PROMPT = "system_prompt"
    FILE_REFERENCE = "file_reference"
    DIRECTORY_LISTING = "directory_listing"
    CONVERSATION_HISTORY = "conversation_history"
    PLAYBOOK_STRATEGY = "playbook_strategy"
    IMAGE_CONTENT = "image_content"
    PDF_CONTENT = "pdf_content"
    TOOL_RESULT = "tool_result"
    USER_QUERY = "user_query"


@dataclass
class ContextReason:
    """Documents why a context piece was included.
    
    This is the key to traceability - every piece of context should have
    a clear reason for inclusion that can be logged and debugged.
    
    Attributes:
        source: Identifier of the source (e.g., "file_reference", "playbook")
        reason: Human-readable explanation of why this was included
        relevance_score: 0.0-1.0 score for ranked selection (1.0 = most relevant)
        tokens_estimate: Estimated token count for this piece
        metadata: Source-specific metadata for debugging
    """
    
    source: str
    reason: str
    relevance_score: float = 1.0
    tokens_estimate: int = 0
    metadata: dict[str, Any] = field(default_factory=dict)
    
    def __str__(self) -> str:
        """Human-readable representation."""
        score_str = f" (score={self.relevance_score:.2f})" if self.relevance_score < 1.0 else ""
        tokens_str = f" [{self.tokens_estimate} tokens]" if self.tokens_estimate > 0 else ""
        return f"[{self.source}]{score_str}{tokens_str}: {self.reason}"


@dataclass
class ContextPiece:
    """A single piece of context to include in the LLM call.
    
    Combines the actual content with its reason for inclusion and category.
    
    Attributes:
        content: The actual text content to include
        reason: ContextReason documenting why this was included
        category: Category for organization
        order: Optional ordering hint (lower = earlier in context)
    """
    
    content: str
    reason: ContextReason
    category: ContextCategory
    order: int = 100  # Default ordering, lower numbers come first
    
    @property
    def tokens_estimate(self) -> int:
        """Estimated token count (from reason or calculated)."""
        if self.reason.tokens_estimate > 0:
            return self.reason.tokens_estimate
        # Rough estimate: ~4 chars per token
        return len(self.content) // 4
    
    def __str__(self) -> str:
        """Human-readable representation."""
        preview = self.content[:50] + "..." if len(self.content) > 50 else self.content
        preview = preview.replace("\n", "\\n")
        return f"{self.category.value}: {preview}"


@dataclass
class AssembledContext:
    """Final assembled context ready for LLM call.
    
    This is the output of ContextPicker.pick_context() and contains
    everything needed for an LLM call plus traceability information.
    
    Attributes:
        system_prompt: The complete system prompt
        messages: List of message dicts for the API call
        pieces: All context pieces with their reasons (for tracing)
        image_blocks: Multimodal image blocks for vision API
        total_tokens_estimate: Total estimated tokens
    """
    
    system_prompt: str
    messages: list[dict[str, Any]]
    pieces: list[ContextPiece] = field(default_factory=list)
    image_blocks: list[dict[str, Any]] = field(default_factory=list)
    total_tokens_estimate: int = 0
    
    def summary(self) -> str:
        """Return concise summary of context for display.
        
        Returns:
            Formatted summary string
        """
        # Group pieces by category
        by_category: dict[ContextCategory, list[ContextPiece]] = {}
        for piece in self.pieces:
            if piece.category not in by_category:
                by_category[piece.category] = []
            by_category[piece.category].append(piece)
        
        parts = []
        for category, pieces in by_category.items():
            total_tokens = sum(p.tokens_estimate for p in pieces)
            parts.append(f"{category.value}: ~{total_tokens:,} tokens")
        
        summary = f"Context: {self.total_tokens_estimate:,} tokens"
        if parts:
            summary += f" ({', '.join(parts)})"
        
        return summary
