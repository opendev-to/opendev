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

// ---------------------------------------------------------------
// Budget tests
// ---------------------------------------------------------------

#[test]
fn test_no_budget_not_over() {
    let tracker = CostTracker::new();
    assert!(!tracker.is_over_budget());
    assert_eq!(tracker.remaining_budget(), None);
}

#[test]
fn test_set_budget() {
    let mut tracker = CostTracker::new();
    tracker.set_budget(1.0);
    assert_eq!(tracker.budget_usd, Some(1.0));
    assert!(!tracker.is_over_budget());
    assert!((tracker.remaining_budget().unwrap() - 1.0).abs() < 1e-9);
}

#[test]
fn test_budget_not_exceeded() {
    let mut tracker = CostTracker::new();
    tracker.set_budget(1.0);
    tracker.total_cost_usd = 0.5;
    assert!(!tracker.is_over_budget());
    assert!((tracker.remaining_budget().unwrap() - 0.5).abs() < 1e-9);
}

#[test]
fn test_budget_exactly_met() {
    let mut tracker = CostTracker::new();
    tracker.set_budget(1.0);
    tracker.total_cost_usd = 1.0;
    assert!(tracker.is_over_budget());
    assert!((tracker.remaining_budget().unwrap()).abs() < 1e-9);
}

#[test]
fn test_budget_exceeded() {
    let mut tracker = CostTracker::new();
    tracker.set_budget(0.50);
    tracker.total_cost_usd = 0.75;
    assert!(tracker.is_over_budget());
    assert_eq!(tracker.remaining_budget().unwrap(), 0.0);
}

#[test]
fn test_budget_exceeded_after_usage() {
    let mut tracker = CostTracker::new();
    tracker.set_budget(0.05);
    let pricing = test_pricing();
    // Record usage that exceeds the $0.05 budget
    let usage = TokenUsage {
        prompt_tokens: 10_000,
        completion_tokens: 5_000,
        ..Default::default()
    };
    tracker.record_usage(&usage, Some(&pricing));
    // input: 10_000/1M * $3 = $0.03
    // output: 5_000/1M * $15 = $0.075
    // total: $0.105 > $0.05
    assert!(tracker.is_over_budget());
    assert_eq!(tracker.remaining_budget().unwrap(), 0.0);
}

#[test]
fn test_budget_serialization() {
    let mut tracker = CostTracker::new();
    tracker.set_budget(2.50);
    tracker.total_cost_usd = 0.75;
    let json = serde_json::to_string(&tracker).unwrap();
    let restored: CostTracker = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.budget_usd, Some(2.50));
    assert!(!restored.is_over_budget());
    assert!((restored.remaining_budget().unwrap() - 1.75).abs() < 1e-9);
}
