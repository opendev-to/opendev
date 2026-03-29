use super::*;

#[test]
fn test_context_category_display() {
    assert_eq!(ContextCategory::SystemPrompt.as_str(), "system_prompt");
    assert_eq!(ContextCategory::FileReference.as_str(), "file_reference");
    assert_eq!(ContextCategory::UserQuery.as_str(), "user_query");
}

#[test]
fn test_context_reason_display() {
    let reason = ContextReason::new("file_reference", "User referenced @main.rs")
        .with_tokens(500)
        .with_score(0.85);
    let display = format!("{}", reason);
    assert!(display.contains("file_reference"));
    assert!(display.contains("0.85"));
    assert!(display.contains("500 tokens"));
}

#[test]
fn test_context_piece_tokens() {
    let reason = ContextReason::new("test", "test").with_tokens(100);
    let piece = ContextPiece::new(
        "hello world".to_string(),
        reason,
        ContextCategory::UserQuery,
    );
    assert_eq!(piece.tokens_estimate(), 100);

    // Without explicit tokens, estimate from content
    let reason2 = ContextReason::new("test", "test");
    let piece2 = ContextPiece::new("x".repeat(400), reason2, ContextCategory::UserQuery);
    assert_eq!(piece2.tokens_estimate(), 100);
}

#[test]
fn test_assembled_context_summary() {
    let ctx = AssembledContext {
        system_prompt: "test".to_string(),
        messages: vec![],
        pieces: vec![
            ContextPiece::new(
                "prompt content".to_string(),
                ContextReason::new("system_prompt", "Agent system prompt").with_tokens(200),
                ContextCategory::SystemPrompt,
            ),
            ContextPiece::new(
                "file content".to_string(),
                ContextReason::new("file_reference", "Referenced file").with_tokens(300),
                ContextCategory::FileReference,
            ),
        ],
        image_blocks: vec![],
        total_tokens_estimate: 500,
    };
    let summary = ctx.summary();
    assert!(summary.contains("500 tokens"));
}

#[test]
fn test_context_piece_ordering() {
    let p1 = ContextPiece::new(
        "system".to_string(),
        ContextReason::new("sys", "sys"),
        ContextCategory::SystemPrompt,
    )
    .with_order(0);

    let p2 = ContextPiece::new(
        "query".to_string(),
        ContextReason::new("query", "query"),
        ContextCategory::UserQuery,
    )
    .with_order(100);

    assert!(p1.order < p2.order);
}

#[test]
fn test_context_reason_serialization() {
    let reason = ContextReason::new("test_source", "test reason")
        .with_tokens(42)
        .with_score(0.9);
    let json = serde_json::to_string(&reason).unwrap();
    let deserialized: ContextReason = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.source, "test_source");
    assert_eq!(deserialized.tokens_estimate, 42);
}
