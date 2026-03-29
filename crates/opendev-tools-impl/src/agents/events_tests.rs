use super::*;
use std::sync::Arc;

#[test]
fn test_subagent_event_variants() {
    let started = SubagentEvent::Started {
        subagent_id: "id-1".into(),
        subagent_name: "Explore".into(),
        task: "Find all TODO comments".into(),
        cancel_token: None,
    };
    assert!(matches!(started, SubagentEvent::Started { .. }));

    let finished = SubagentEvent::Finished {
        subagent_id: "id-1".into(),
        subagent_name: "Explore".into(),
        success: true,
        result_summary: "Found 5 TODOs".into(),
        tool_call_count: 3,
        shallow_warning: None,
    };
    assert!(matches!(finished, SubagentEvent::Finished { .. }));
}

#[tokio::test]
async fn test_channel_progress_callback() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cb = ChannelProgressCallback::new(tx, "test-id".into(), None);

    use opendev_agents::SubagentProgressCallback;
    cb.on_started("test-agent", "do a thing");
    cb.on_tool_call(
        "test-agent",
        "read_file",
        "tc-1",
        &std::collections::HashMap::new(),
    );
    cb.on_tool_complete("test-agent", "read_file", "tc-1", true);
    // on_finished is intentionally a no-op (SpawnSubagentTool sends the real Finished event)
    cb.on_finished("test-agent", true, "Done");

    let evt = rx.recv().await.unwrap();
    assert!(matches!(evt, SubagentEvent::Started { .. }));
    let evt = rx.recv().await.unwrap();
    assert!(matches!(evt, SubagentEvent::ToolCall { .. }));
    let evt = rx.recv().await.unwrap();
    assert!(matches!(evt, SubagentEvent::ToolComplete { .. }));
    // No Finished event expected — on_finished is a no-op
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn test_bridge_to_channel_end_to_end() {
    // Verify the full chain: SubagentEventBridge → ChannelProgressCallback → channel
    let (tx, mut rx) = mpsc::unbounded_channel();
    let subagent_id = "test-sa-id".to_string();
    let cb: Arc<dyn opendev_agents::SubagentProgressCallback> =
        Arc::new(ChannelProgressCallback::new(tx, subagent_id.clone(), None));

    // Create bridge (as SubagentManager::spawn would)
    let bridge = opendev_agents::SubagentEventBridge::new("Explorer".to_string(), cb);

    // Simulate react loop calling the bridge
    use opendev_agents::AgentEventCallback;
    let args = std::collections::HashMap::new();
    bridge.on_tool_started("tc-1", "read_file", &args);
    bridge.on_tool_finished("tc-1", true);
    bridge.on_token_usage(500, 100);

    // Verify events arrive on the channel
    let evt = rx.recv().await.unwrap();
    match evt {
        SubagentEvent::ToolCall {
            subagent_id: id,
            subagent_name,
            tool_name,
            tool_id,
            args: _,
        } => {
            assert_eq!(id, "test-sa-id");
            assert_eq!(subagent_name, "Explorer");
            assert_eq!(tool_name, "read_file");
            assert_eq!(tool_id, "tc-1");
        }
        other => panic!("Expected ToolCall, got {other:?}"),
    }

    let evt = rx.recv().await.unwrap();
    assert!(matches!(evt, SubagentEvent::ToolComplete { .. }));

    let evt = rx.recv().await.unwrap();
    match evt {
        SubagentEvent::TokenUpdate {
            input_tokens,
            output_tokens,
            ..
        } => {
            assert_eq!(input_tokens, 500);
            assert_eq!(output_tokens, 100);
        }
        other => panic!("Expected TokenUpdate, got {other:?}"),
    }
}
