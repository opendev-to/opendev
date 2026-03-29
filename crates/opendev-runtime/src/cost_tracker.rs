//! Session-level cost tracking for LLM API usage.
//!
//! Uses ModelInfo pricing ($ per million tokens) to compute cost from
//! the usage dict returned by each LLM API call.
//!
//! Ported from `opendev/core/runtime/cost_tracker.py`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// Token usage from a single LLM call.
///
/// Maps to the usage dict returned by OpenAI/Anthropic APIs.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Anthropic prompt-caching: tokens read from cache.
    pub cache_read_input_tokens: u64,
    /// Anthropic prompt-caching: tokens written to cache.
    pub cache_creation_input_tokens: u64,
}

impl TokenUsage {
    /// Parse from a serde_json::Value (the `usage` field in API responses).
    pub fn from_json(value: &serde_json::Value) -> Self {
        Self {
            prompt_tokens: value
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            completion_tokens: value
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_read_input_tokens: value
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_creation_input_tokens: value
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        }
    }
}

/// Pricing info needed for cost computation.
///
/// Prices are in USD per 1 million tokens.
#[derive(Debug, Clone)]
pub struct PricingInfo {
    pub input_price_per_million: f64,
    pub output_price_per_million: f64,
}

/// Tracks cumulative token usage and estimated cost for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTracker {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub call_count: u64,
    /// Optional session cost budget in USD. When set, the agent loop should
    /// check [`is_over_budget`](CostTracker::is_over_budget) and pause when
    /// the budget is exhausted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_usd: Option<f64>,
}

/// Anthropic charges higher rates for prompts over 200K tokens.
const OVER_200K_THRESHOLD: u64 = 200_000;
const OVER_200K_MULTIPLIER: f64 = 1.5;
/// Cache read tokens are typically 10% of input price.
const CACHE_READ_DISCOUNT: f64 = 0.1;

impl CostTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            call_count: 0,
            budget_usd: None,
        }
    }

    /// Set a cost budget in USD for the session.
    ///
    /// Once [`total_cost_usd`](CostTracker::total_cost_usd) reaches or
    /// exceeds this value, [`is_over_budget`](CostTracker::is_over_budget)
    /// returns `true` and the agent loop should pause.
    pub fn set_budget(&mut self, usd: f64) {
        self.budget_usd = Some(usd);
    }

    /// Check whether the session has exceeded its cost budget.
    ///
    /// Returns `false` when no budget has been set.
    pub fn is_over_budget(&self) -> bool {
        match self.budget_usd {
            Some(budget) => self.total_cost_usd >= budget,
            None => false,
        }
    }

    /// Return the remaining budget in USD, or `None` if no budget is set.
    pub fn remaining_budget(&self) -> Option<f64> {
        self.budget_usd
            .map(|budget| (budget - self.total_cost_usd).max(0.0))
    }

    /// Record token usage from a single LLM call.
    ///
    /// Returns the incremental cost for this call in USD.
    pub fn record_usage(&mut self, usage: &TokenUsage, pricing: Option<&PricingInfo>) -> f64 {
        self.total_input_tokens += usage.prompt_tokens;
        self.total_output_tokens += usage.completion_tokens;
        self.call_count += 1;

        let incremental_cost = if let Some(p) = pricing {
            if p.input_price_per_million > 0.0 || p.output_price_per_million > 0.0 {
                self.compute_cost(usage, p)
            } else {
                0.0
            }
        } else {
            0.0
        };

        self.total_cost_usd += incremental_cost;

        debug!(
            call = self.call_count,
            input = usage.prompt_tokens,
            output = usage.completion_tokens,
            cost_delta = format!("${:.6}", incremental_cost),
            cost_total = format!("${:.6}", self.total_cost_usd),
            "cost_tracker: recorded usage"
        );

        incremental_cost
    }

    fn compute_cost(&self, usage: &TokenUsage, pricing: &PricingInfo) -> f64 {
        // Handle tiered pricing for inputs over 200K tokens
        let input_cost = if usage.prompt_tokens > OVER_200K_THRESHOLD {
            let base = (OVER_200K_THRESHOLD as f64 / 1_000_000.0) * pricing.input_price_per_million;
            let over = ((usage.prompt_tokens - OVER_200K_THRESHOLD) as f64 / 1_000_000.0)
                * (pricing.input_price_per_million * OVER_200K_MULTIPLIER);
            base + over
        } else {
            (usage.prompt_tokens as f64 / 1_000_000.0) * pricing.input_price_per_million
        };

        // Cache read tokens at 10% of input price
        let cache_cost = if usage.cache_read_input_tokens > 0 {
            (usage.cache_read_input_tokens as f64 / 1_000_000.0)
                * (pricing.input_price_per_million * CACHE_READ_DISCOUNT)
        } else {
            0.0
        };

        let output_cost =
            (usage.completion_tokens as f64 / 1_000_000.0) * pricing.output_price_per_million;

        input_cost + output_cost + cache_cost
    }

    /// Format the total cost for display.
    pub fn format_cost(&self) -> String {
        if self.total_cost_usd < 0.01 {
            format!("${:.4}", self.total_cost_usd)
        } else {
            format!("${:.2}", self.total_cost_usd)
        }
    }

    /// Export cost data for session metadata persistence.
    pub fn to_metadata(&self) -> HashMap<String, serde_json::Value> {
        let mut map = HashMap::new();
        map.insert(
            "total_cost_usd".into(),
            serde_json::json!(round_f64(self.total_cost_usd, 6)),
        );
        map.insert(
            "total_input_tokens".into(),
            serde_json::json!(self.total_input_tokens),
        );
        map.insert(
            "total_output_tokens".into(),
            serde_json::json!(self.total_output_tokens),
        );
        map.insert("api_call_count".into(), serde_json::json!(self.call_count));
        if let Some(budget) = self.budget_usd {
            map.insert("budget_usd".into(), serde_json::json!(round_f64(budget, 6)));
        }
        map
    }

    /// Restore cost state from session metadata (for `--continue` sessions).
    pub fn restore_from_metadata(&mut self, metadata: &serde_json::Value) {
        let cost_data = match metadata.get("cost_tracking") {
            Some(v) => v,
            None => return,
        };

        self.total_cost_usd = cost_data
            .get("total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        self.total_input_tokens = cost_data
            .get("total_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.total_output_tokens = cost_data
            .get("total_output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.call_count = cost_data
            .get("api_call_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.budget_usd = cost_data.get("budget_usd").and_then(|v| v.as_f64());

        debug!(
            cost = format!("${:.6}", self.total_cost_usd),
            calls = self.call_count,
            "cost_tracker: restored from metadata"
        );
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Round an f64 to N decimal places.
fn round_f64(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

#[cfg(test)]
#[path = "cost_tracker_tests.rs"]
mod tests;
