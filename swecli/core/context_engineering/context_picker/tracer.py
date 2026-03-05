"""Optional tracing for context selection decisions.

This module provides simple logging utilities for understanding
context selection. It's primarily for internal debugging, not user display.
"""

from __future__ import annotations

import json
import logging
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from .models import AssembledContext

logger = logging.getLogger(__name__)


class ContextTracer:
    """Simple tracer for context selection.
    
    Primarily for internal debugging - logs context decisions
    without verbose user-facing output.
    """
    
    def __init__(self, *, log_level: str = "DEBUG"):
        """Initialize the tracer.
        
        Args:
            log_level: Logging level for trace output
        """
        self.log_level = getattr(logging, log_level.upper(), logging.DEBUG)
    
    def trace(self, context: "AssembledContext") -> None:
        """Log context summary to the logger.
        
        Args:
            context: The assembled context to trace
        """
        if logger.isEnabledFor(self.log_level):
            logger.log(self.log_level, f"[ContextPicker] {context.summary()}")
    
    def export_trace(
        self,
        context: "AssembledContext",
        path: str,
    ) -> None:
        """Export trace to a JSON file for debugging.
        
        Args:
            context: The assembled context to export
            path: Path to save the trace file
        """
        trace_data = {
            "timestamp": datetime.now().isoformat(),
            "total_tokens_estimate": context.total_tokens_estimate,
            "message_count": len(context.messages),
            "piece_count": len(context.pieces),
            "image_count": len(context.image_blocks),
            "pieces": [
                {
                    "category": p.category.value,
                    "source": p.reason.source,
                    "tokens_estimate": p.tokens_estimate,
                }
                for p in context.pieces
            ],
        }
        
        output_path = Path(path)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        
        with open(output_path, "w") as f:
            json.dump(trace_data, f, indent=2)
        
        logger.debug(f"Context trace exported to {path}")


# Global tracer instance
_default_tracer: Optional[ContextTracer] = None


def get_tracer() -> ContextTracer:
    """Get the default tracer instance."""
    global _default_tracer
    if _default_tracer is None:
        _default_tracer = ContextTracer()
    return _default_tracer
