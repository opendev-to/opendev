"""Context Picker - Unified context selection for LLM calls.

This module provides a single entry point for all context engineering
before LLM calls. It coordinates file references, conversation history,
playbook strategies, and system prompt assembly.

Usage:
    from swecli.core.context_engineering.context_picker import ContextPicker
    
    picker = ContextPicker(session_manager, config, file_ops)
    context = picker.pick_context("Explain @main.py", agent)
    
    # Use for LLM call
    messages = context.messages
    
    # Get summary for display
    print(context.summary())
"""

from swecli.core.context_engineering.context_picker.models import (
    AssembledContext,
    ContextCategory,
    ContextPiece,
    ContextReason,
)
from swecli.core.context_engineering.context_picker.picker import ContextPicker
from swecli.core.context_engineering.context_picker.tracer import (
    ContextTracer,
    get_tracer,
)

__all__ = [
    # Main class
    "ContextPicker",
    # Models
    "AssembledContext",
    "ContextCategory",
    "ContextPiece",
    "ContextReason",
    # Tracer
    "ContextTracer",
    "get_tracer",
]
