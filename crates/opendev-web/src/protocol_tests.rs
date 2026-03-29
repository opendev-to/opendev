use super::*;

#[test]
fn test_message_type_display() {
    assert_eq!(WsMessageType::ToolCall.to_string(), "tool_call");
    assert_eq!(
        WsMessageType::PlanApprovalResolved.to_string(),
        "plan_approval_resolved"
    );
    assert_eq!(
        WsMessageType::McpStatusChanged.to_string(),
        "mcp:status_changed"
    );
}

#[test]
fn test_message_type_roundtrip() {
    for mt in [
        WsMessageType::ToolCall,
        WsMessageType::PlanApprovalResponse,
        WsMessageType::McpServersUpdated,
        WsMessageType::Interrupt,
    ] {
        let s = mt.as_str();
        let parsed = WsMessageType::from_str_opt(s).unwrap();
        assert_eq!(parsed, mt);
    }
}

#[test]
fn test_from_str_opt_unknown() {
    assert!(WsMessageType::from_str_opt("unknown_type").is_none());
}

#[test]
fn test_ws_message_envelope() {
    let msg = ws_message(WsMessageType::Error, serde_json::json!({"message": "oops"}));
    assert_eq!(msg["type"], "error");
    assert_eq!(msg["data"]["message"], "oops");
}

#[test]
fn test_serde_roundtrip() {
    let mt = WsMessageType::PlanApprovalRequired;
    let json = serde_json::to_string(&mt).unwrap();
    assert_eq!(json, "\"plan_approval_required\"");
    let parsed: WsMessageType = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, mt);
}

#[test]
fn test_all_variants_have_as_str() {
    // Ensure every variant can round-trip through as_str -> from_str_opt.
    let variants = vec![
        WsMessageType::ToolCall,
        WsMessageType::ToolResult,
        WsMessageType::ApprovalRequired,
        WsMessageType::ApprovalResolved,
        WsMessageType::AskUserRequired,
        WsMessageType::AskUserResolved,
        WsMessageType::PlanContent,
        WsMessageType::PlanApprovalRequired,
        WsMessageType::PlanApprovalResolved,
        WsMessageType::StatusUpdate,
        WsMessageType::TaskCompleted,
        WsMessageType::SubagentStart,
        WsMessageType::SubagentComplete,
        WsMessageType::ParallelAgentsStart,
        WsMessageType::ParallelAgentsDone,
        WsMessageType::ThinkingBlock,
        WsMessageType::Progress,
        WsMessageType::NestedToolCall,
        WsMessageType::NestedToolResult,
        WsMessageType::MessageChunk,
        WsMessageType::MessageStart,
        WsMessageType::MessageComplete,
        WsMessageType::SessionActivity,
        WsMessageType::UserMessage,
        WsMessageType::McpStatusChanged,
        WsMessageType::McpServersUpdated,
        WsMessageType::Error,
        WsMessageType::Pong,
        WsMessageType::Query,
        WsMessageType::Approve,
        WsMessageType::AskUserResponse,
        WsMessageType::PlanApprovalResponse,
        WsMessageType::Ping,
        WsMessageType::Interrupt,
    ];
    for v in variants {
        assert_eq!(
            WsMessageType::from_str_opt(v.as_str()),
            Some(v),
            "Round-trip failed for {:?}",
            v
        );
    }
}
