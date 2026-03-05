"""Session-level cost tracking for LLM API usage."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING, Optional

if TYPE_CHECKING:
    from swecli.config.models import ModelInfo

logger = logging.getLogger(__name__)


class CostTracker:
    """Tracks cumulative token usage and estimated cost for a session.

    Uses ModelInfo pricing ($ per million tokens) to compute cost from
    the usage dict returned by each LLM API call.
    """

    def __init__(self) -> None:
        self.total_input_tokens: int = 0
        self.total_output_tokens: int = 0
        self.total_cost_usd: float = 0.0
        self.call_count: int = 0

    def record_usage(
        self,
        usage: dict,
        model_info: Optional["ModelInfo"] = None,
    ) -> float:
        """Record token usage from a single LLM call.

        Args:
            usage: Usage dict with prompt_tokens, completion_tokens keys.
            model_info: ModelInfo with pricing_input/pricing_output ($ per 1M tokens).
                If None, tokens are tracked but cost is not computed.

        Returns:
            Incremental cost for this call in USD.
        """
        prompt_tokens = usage.get("prompt_tokens", 0)
        completion_tokens = usage.get("completion_tokens", 0)

        self.total_input_tokens += prompt_tokens
        self.total_output_tokens += completion_tokens
        self.call_count += 1

        incremental_cost = 0.0
        if model_info and (model_info.pricing_input or model_info.pricing_output):
            incremental_cost = (
                (prompt_tokens / 1_000_000) * model_info.pricing_input
                + (completion_tokens / 1_000_000) * model_info.pricing_output
            )
            self.total_cost_usd += incremental_cost

        logger.debug(
            "cost_tracker: call=%d in=%d out=%d cost=+$%.6f total=$%.6f",
            self.call_count,
            prompt_tokens,
            completion_tokens,
            incremental_cost,
            self.total_cost_usd,
        )

        return incremental_cost

    def format_cost(self) -> str:
        """Format the total cost for display.

        Returns:
            Human-readable cost string (e.g., "$0.12" or "$1.23").
        """
        if self.total_cost_usd < 0.01:
            return f"${self.total_cost_usd:.4f}"
        return f"${self.total_cost_usd:.2f}"

    def to_metadata(self) -> dict:
        """Export cost data for session metadata persistence.

        Returns:
            Dict suitable for storing in session.metadata.
        """
        return {
            "total_cost_usd": round(self.total_cost_usd, 6),
            "total_input_tokens": self.total_input_tokens,
            "total_output_tokens": self.total_output_tokens,
            "api_call_count": self.call_count,
        }

    def restore_from_metadata(self, metadata: dict) -> None:
        """Restore cost state from session metadata (for --continue sessions).

        Args:
            metadata: Session metadata dict containing cost tracking keys.
        """
        cost_data = metadata.get("cost_tracking")
        if not cost_data:
            return
        self.total_cost_usd = cost_data.get("total_cost_usd", 0.0)
        self.total_input_tokens = cost_data.get("total_input_tokens", 0)
        self.total_output_tokens = cost_data.get("total_output_tokens", 0)
        self.call_count = cost_data.get("api_call_count", 0)
        logger.debug(
            "cost_tracker: restored from metadata — $%.6f, %d calls",
            self.total_cost_usd,
            self.call_count,
        )
