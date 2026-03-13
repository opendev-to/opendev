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
mod tests {
    use super::*;

    fn test_pricing() -> PricingInfo {
        PricingInfo {
            input_price_per_million: 3.0,   // $3 per 1M input tokens
            output_price_per_million: 15.0, // $15 per 1M output tokens
        }
    }

    #[test]
    fn test_basic_cost_tracking() {
        let mut tracker = CostTracker::new();
        let usage = TokenUsage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            ..Default::default()
        };
        let cost = tracker.record_usage(&usage, Some(&test_pricing()));

        // input: 1000/1M * $3 = $0.003
        // output: 500/1M * $15 = $0.0075
        let expected = 0.003 + 0.0075;
        assert!((cost - expected).abs() < 1e-9);
        assert_eq!(tracker.total_input_tokens, 1000);
        assert_eq!(tracker.total_output_tokens, 500);
        assert_eq!(tracker.call_count, 1);
    }

    #[test]
    fn test_tiered_pricing_over_200k() {
        let mut tracker = CostTracker::new();
        let usage = TokenUsage {
            prompt_tokens: 250_000,
            completion_tokens: 100,
            ..Default::default()
        };
        let cost = tracker.record_usage(&usage, Some(&test_pricing()));

        // First 200K at base rate: 200000/1M * $3 = $0.60
        // Remaining 50K at 1.5x: 50000/1M * $4.5 = $0.225
        // Output: 100/1M * $15 = $0.0015
        let expected = 0.60 + 0.225 + 0.0015;
        assert!((cost - expected).abs() < 1e-9);
    }

    #[test]
    fn test_cache_read_tokens() {
        let mut tracker = CostTracker::new();
        let usage = TokenUsage {
            prompt_tokens: 1000,
            completion_tokens: 100,
            cache_read_input_tokens: 5000,
            ..Default::default()
        };
        let cost = tracker.record_usage(&usage, Some(&test_pricing()));

        // input: 1000/1M * $3 = $0.003
        // cache: 5000/1M * $0.3 = $0.0015
        // output: 100/1M * $15 = $0.0015
        let expected = 0.003 + 0.0015 + 0.0015;
        assert!((cost - expected).abs() < 1e-9);
    }

    #[test]
    fn test_no_pricing_tracks_tokens_only() {
        let mut tracker = CostTracker::new();
        let usage = TokenUsage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            ..Default::default()
        };
        let cost = tracker.record_usage(&usage, None);
        assert_eq!(cost, 0.0);
        assert_eq!(tracker.total_input_tokens, 1000);
        assert_eq!(tracker.total_output_tokens, 500);
        assert_eq!(tracker.total_cost_usd, 0.0);
    }

    #[test]
    fn test_cumulative_tracking() {
        let mut tracker = CostTracker::new();
        let pricing = test_pricing();
        let usage1 = TokenUsage {
            prompt_tokens: 1000,
            completion_tokens: 200,
            ..Default::default()
        };
        let usage2 = TokenUsage {
            prompt_tokens: 2000,
            completion_tokens: 300,
            ..Default::default()
        };
        tracker.record_usage(&usage1, Some(&pricing));
        tracker.record_usage(&usage2, Some(&pricing));

        assert_eq!(tracker.total_input_tokens, 3000);
        assert_eq!(tracker.total_output_tokens, 500);
        assert_eq!(tracker.call_count, 2);
    }

    #[test]
    fn test_format_cost_small() {
        let mut tracker = CostTracker::new();
        tracker.total_cost_usd = 0.005;
        assert_eq!(tracker.format_cost(), "$0.0050");
    }

    #[test]
    fn test_format_cost_large() {
        let mut tracker = CostTracker::new();
        tracker.total_cost_usd = 1.234;
        assert_eq!(tracker.format_cost(), "$1.23");
    }

    #[test]
    fn test_to_metadata_and_restore() {
        let mut tracker = CostTracker::new();
        tracker.total_input_tokens = 5000;
        tracker.total_output_tokens = 2000;
        tracker.total_cost_usd = 0.123456;
        tracker.call_count = 3;

        let metadata = tracker.to_metadata();

        let mut restored = CostTracker::new();
        let meta_json = serde_json::json!({
            "cost_tracking": metadata,
        });
        restored.restore_from_metadata(&meta_json);

        assert_eq!(restored.total_input_tokens, 5000);
        assert_eq!(restored.total_output_tokens, 2000);
        assert!((restored.total_cost_usd - 0.123456).abs() < 1e-9);
        assert_eq!(restored.call_count, 3);
    }

    #[test]
    fn test_restore_missing_cost_tracking() {
        let mut tracker = CostTracker::new();
        tracker.total_input_tokens = 100;
        // No cost_tracking key — should be a no-op
        tracker.restore_from_metadata(&serde_json::json!({}));
        assert_eq!(tracker.total_input_tokens, 100);
    }

    #[test]
    fn test_token_usage_from_json() {
        let json = serde_json::json!({
            "prompt_tokens": 1500,
            "completion_tokens": 300,
            "cache_read_input_tokens": 800,
        });
        let usage = TokenUsage::from_json(&json);
        assert_eq!(usage.prompt_tokens, 1500);
        assert_eq!(usage.completion_tokens, 300);
        assert_eq!(usage.cache_read_input_tokens, 800);
        assert_eq!(usage.cache_creation_input_tokens, 0);
    }

    #[test]
    fn test_round_f64() {
        assert_eq!(round_f64(1.23456789, 6), 1.234568);
        assert_eq!(round_f64(0.0, 2), 0.0);
    }

    // ---------------------------------------------------------------
    // Cost tracker accuracy tests
    // ---------------------------------------------------------------

    /// Anthropic Claude 3.5 Sonnet pricing: $3/1M input, $15/1M output.
    fn anthropic_sonnet_pricing() -> PricingInfo {
        PricingInfo {
            input_price_per_million: 3.0,
            output_price_per_million: 15.0,
        }
    }

    /// OpenAI GPT-4o pricing: $2.50/1M input, $10/1M output.
    fn openai_gpt4o_pricing() -> PricingInfo {
        PricingInfo {
            input_price_per_million: 2.50,
            output_price_per_million: 10.0,
        }
    }

    #[test]
    fn test_anthropic_cache_discount_accuracy() {
        // Simulate a typical Anthropic call with prompt caching:
        // 10K prompt tokens (fresh), 50K cached reads, 1K output tokens
        let mut tracker = CostTracker::new();
        let usage = TokenUsage {
            prompt_tokens: 10_000,
            completion_tokens: 1_000,
            cache_read_input_tokens: 50_000,
            cache_creation_input_tokens: 0,
        };
        let cost = tracker.record_usage(&usage, Some(&anthropic_sonnet_pricing()));

        // Fresh input: 10_000 / 1M * $3 = $0.03
        // Cached input: 50_000 / 1M * ($3 * 0.1) = 50_000 / 1M * $0.30 = $0.015
        // Output: 1_000 / 1M * $15 = $0.015
        let expected = 0.03 + 0.015 + 0.015;
        assert!(
            (cost - expected).abs() < 1e-9,
            "Anthropic cache cost mismatch: got {cost}, expected {expected}"
        );
    }

    #[test]
    fn test_over_200k_tier_multiplier() {
        // Verify the 1.5x multiplier kicks in exactly at the 200K boundary.
        let mut tracker = CostTracker::new();
        let pricing = anthropic_sonnet_pricing();

        // Exactly at threshold: should NOT trigger multiplier
        let at_threshold = TokenUsage {
            prompt_tokens: 200_000,
            completion_tokens: 0,
            ..Default::default()
        };
        let cost_at = tracker.record_usage(&at_threshold, Some(&pricing));
        let expected_at = 200_000.0 / 1_000_000.0 * 3.0; // $0.60
        assert!(
            (cost_at - expected_at).abs() < 1e-9,
            "At 200K: got {cost_at}, expected {expected_at}"
        );

        // 1 token over threshold: multiplier applies to that 1 token
        let mut tracker2 = CostTracker::new();
        let over_threshold = TokenUsage {
            prompt_tokens: 200_001,
            completion_tokens: 0,
            ..Default::default()
        };
        let cost_over = tracker2.record_usage(&over_threshold, Some(&pricing));
        let expected_over = 0.60 + (1.0 / 1_000_000.0 * 3.0 * 1.5);
        assert!(
            (cost_over - expected_over).abs() < 1e-9,
            "At 200_001: got {cost_over}, expected {expected_over}"
        );
    }

    #[test]
    fn test_openai_pricing_accuracy() {
        let mut tracker = CostTracker::new();
        let pricing = openai_gpt4o_pricing();

        // Typical GPT-4o call: 5K input, 2K output
        let usage = TokenUsage {
            prompt_tokens: 5_000,
            completion_tokens: 2_000,
            // OpenAI has no cache tokens
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
        };
        let cost = tracker.record_usage(&usage, Some(&pricing));

        // Input: 5_000 / 1M * $2.50 = $0.0125
        // Output: 2_000 / 1M * $10 = $0.02
        let expected = 0.0125 + 0.02;
        assert!(
            (cost - expected).abs() < 1e-9,
            "OpenAI cost mismatch: got {cost}, expected {expected}"
        );
    }

    #[test]
    fn test_cost_sum_across_multiple_calls() {
        // Verify cumulative cost accuracy across 5 heterogeneous calls
        let mut tracker = CostTracker::new();
        let pricing = anthropic_sonnet_pricing();

        let calls = vec![
            TokenUsage {
                prompt_tokens: 1_000,
                completion_tokens: 500,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            },
            TokenUsage {
                prompt_tokens: 10_000,
                completion_tokens: 2_000,
                cache_read_input_tokens: 30_000,
                cache_creation_input_tokens: 0,
            },
            TokenUsage {
                prompt_tokens: 50_000,
                completion_tokens: 5_000,
                cache_read_input_tokens: 100_000,
                cache_creation_input_tokens: 0,
            },
            TokenUsage {
                prompt_tokens: 250_000, // triggers tiered pricing
                completion_tokens: 3_000,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            },
            TokenUsage {
                prompt_tokens: 500,
                completion_tokens: 100,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
            },
        ];

        let mut sum = 0.0;
        for usage in &calls {
            let incremental = tracker.record_usage(usage, Some(&pricing));
            sum += incremental;
        }

        // Verify total_cost_usd matches the sum of incremental costs
        assert!(
            (tracker.total_cost_usd - sum).abs() < 1e-9,
            "Cumulative cost {:.6} != sum of incremental costs {:.6}",
            tracker.total_cost_usd,
            sum
        );

        // Verify token totals
        let expected_input: u64 = calls.iter().map(|u| u.prompt_tokens).sum();
        let expected_output: u64 = calls.iter().map(|u| u.completion_tokens).sum();
        assert_eq!(tracker.total_input_tokens, expected_input);
        assert_eq!(tracker.total_output_tokens, expected_output);
        assert_eq!(tracker.call_count, 5);

        // Verify the total is within a reasonable range for the given token volumes
        // Total input: 311,500 tokens, total output: 10,600 tokens, cache: 130,000 tokens
        // The total cost should be positive and non-trivial
        assert!(
            tracker.total_cost_usd > 0.5,
            "Total cost should be > $0.50 for this volume"
        );
        assert!(
            tracker.total_cost_usd < 5.0,
            "Total cost should be < $5.00 for this volume"
        );
    }

    #[test]
    fn test_zero_price_model() {
        // Some local models have $0 pricing — cost should be zero
        let mut tracker = CostTracker::new();
        let pricing = PricingInfo {
            input_price_per_million: 0.0,
            output_price_per_million: 0.0,
        };
        let usage = TokenUsage {
            prompt_tokens: 100_000,
            completion_tokens: 50_000,
            ..Default::default()
        };
        let cost = tracker.record_usage(&usage, Some(&pricing));
        assert_eq!(cost, 0.0);
        assert_eq!(tracker.total_cost_usd, 0.0);
        // Token counts should still be tracked
        assert_eq!(tracker.total_input_tokens, 100_000);
        assert_eq!(tracker.total_output_tokens, 50_000);
    }
}
