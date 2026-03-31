use super::RING_BUFFER_CAPACITY;
use super::*;
use tempfile::TempDir;

fn make_state() -> AppState {
    let tmp = TempDir::new().unwrap();
    let tmp_path = tmp.into_path();
    let session_manager = SessionManager::new(tmp_path.clone()).unwrap();
    let config = AppConfig::default();
    let user_store = UserStore::new(tmp_path.clone()).unwrap();
    let model_registry = ModelRegistry::new();
    AppState::new(
        session_manager,
        config,
        "/tmp/test".to_string(),
        user_store,
        model_registry,
    )
}

#[tokio::test]
async fn test_mode_default() {
    let state = make_state();
    assert_eq!(state.mode().await, OperationMode::Normal);
}

#[tokio::test]
async fn test_set_mode() {
    let state = make_state();
    state.set_mode(OperationMode::Plan).await;
    assert_eq!(state.mode().await, OperationMode::Plan);
}

#[tokio::test]
async fn test_autonomy_level() {
    let state = make_state();
    assert_eq!(state.autonomy_level().await, "Manual");
    state.set_autonomy_level("Auto".to_string()).await;
    assert_eq!(state.autonomy_level().await, "Auto");
}

#[tokio::test]
async fn test_interrupt_flag() {
    let state = make_state();
    assert!(!state.is_interrupt_requested().await);
    state.request_interrupt().await;
    assert!(state.is_interrupt_requested().await);
    state.clear_interrupt().await;
    assert!(!state.is_interrupt_requested().await);
}

#[tokio::test]
async fn test_session_running() {
    let state = make_state();
    assert!(!state.is_session_running("s1").await);
    state.set_session_running("s1".to_string()).await;
    assert!(state.is_session_running("s1").await);
    state.set_session_idle("s1").await;
    assert!(!state.is_session_running("s1").await);
}

#[tokio::test]
async fn test_approval_oneshot_lifecycle() {
    let state = make_state();
    let approval = PendingApproval {
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({"command": "ls"}),
        session_id: Some("s1".to_string()),
    };

    // Add approval and get receiver.
    let rx = state.add_pending_approval("a1".to_string(), approval).await;

    // Verify pending.
    let pending = state.get_pending_approval("a1").await;
    assert!(pending.is_some());
    assert_eq!(pending.unwrap().tool_name, "bash");

    // Resolve it.
    let resolved = state.resolve_approval("a1", true, false).await;
    assert!(resolved.is_some());

    // Receiver should get the result.
    let result = rx.await.unwrap();
    assert!(result.approved);
    assert!(!result.auto_approve);

    // Second resolve returns None (already consumed).
    assert!(state.resolve_approval("a1", false, false).await.is_none());
}

#[tokio::test]
async fn test_interrupt_denies_pending_approvals() {
    let state = make_state();
    let approval = PendingApproval {
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({}),
        session_id: Some("s1".to_string()),
    };

    let rx = state.add_pending_approval("a1".to_string(), approval).await;

    // Interrupt should deny all pending approvals.
    state.request_interrupt().await;

    let result = rx.await.unwrap();
    assert!(!result.approved);
}

#[tokio::test]
async fn test_clear_session_approvals() {
    let state = make_state();

    let approval_s1 = PendingApproval {
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({}),
        session_id: Some("s1".to_string()),
    };
    let approval_s2 = PendingApproval {
        tool_name: "edit".to_string(),
        arguments: serde_json::json!({}),
        session_id: Some("s2".to_string()),
    };

    let rx_s1 = state
        .add_pending_approval("a1".to_string(), approval_s1)
        .await;
    let _rx_s2 = state
        .add_pending_approval("a2".to_string(), approval_s2)
        .await;

    // Clear only s1's approvals.
    state.clear_session_approvals("s1").await;

    // s1 approval should be rejected.
    let result = rx_s1.await.unwrap();
    assert!(!result.approved);

    // s2 approval should still be pending.
    assert!(state.get_pending_approval("a2").await.is_some());
}

#[tokio::test]
async fn test_ask_user_oneshot_lifecycle() {
    let state = make_state();
    let ask = PendingAskUser {
        prompt: "What is your name?".to_string(),
        session_id: Some("s1".to_string()),
    };

    let rx = state.add_pending_ask_user("q1".to_string(), ask).await;

    let pending = state.get_pending_ask_user("q1").await;
    assert!(pending.is_some());

    let resolved = state
        .resolve_ask_user("q1", Some(serde_json::json!({"name": "Alice"})), false)
        .await;
    assert!(resolved.is_some());

    let result = rx.await.unwrap();
    assert!(!result.cancelled);
    assert_eq!(
        result.answers.unwrap(),
        serde_json::json!({"name": "Alice"})
    );
}

#[tokio::test]
async fn test_interrupt_cancels_ask_users() {
    let state = make_state();
    let ask = PendingAskUser {
        prompt: "question".to_string(),
        session_id: None,
    };

    let rx = state.add_pending_ask_user("q1".to_string(), ask).await;

    state.request_interrupt().await;

    let result = rx.await.unwrap();
    assert!(result.cancelled);
}

#[tokio::test]
async fn test_plan_approval_oneshot_lifecycle() {
    let state = make_state();
    let plan = PendingPlanApproval {
        data: serde_json::json!({"plan": "do something"}),
        session_id: Some("s1".to_string()),
    };

    let rx = state
        .add_pending_plan_approval("p1".to_string(), plan)
        .await;

    // Verify pending.
    let pending = state.get_pending_plan_approval("p1").await;
    assert!(pending.is_some());

    // Resolve it.
    let resolved = state
        .resolve_plan_approval("p1", "approve".to_string(), "looks good".to_string())
        .await;
    assert!(resolved.is_some());

    // Receiver should get the result.
    let result = rx.await.unwrap();
    assert_eq!(result.action, "approve");
    assert_eq!(result.feedback, "looks good");

    // Second resolve returns None.
    assert!(
        state
            .resolve_plan_approval("p1", "reject".to_string(), String::new())
            .await
            .is_none()
    );
}

#[tokio::test]
async fn test_interrupt_rejects_plan_approvals() {
    let state = make_state();
    let plan = PendingPlanApproval {
        data: serde_json::json!({"plan": "test"}),
        session_id: None,
    };

    let rx = state
        .add_pending_plan_approval("p1".to_string(), plan)
        .await;

    state.request_interrupt().await;

    let result = rx.await.unwrap();
    assert_eq!(result.action, "reject");
}

#[tokio::test]
async fn test_clear_session_plan_approvals() {
    let state = make_state();

    let plan_s1 = PendingPlanApproval {
        data: serde_json::json!({}),
        session_id: Some("s1".to_string()),
    };
    let plan_s2 = PendingPlanApproval {
        data: serde_json::json!({}),
        session_id: Some("s2".to_string()),
    };

    let rx_s1 = state
        .add_pending_plan_approval("p1".to_string(), plan_s1)
        .await;
    let _rx_s2 = state
        .add_pending_plan_approval("p2".to_string(), plan_s2)
        .await;

    state.clear_session_plan_approvals("s1").await;

    let result = rx_s1.await.unwrap();
    assert_eq!(result.action, "reject");

    // s2 should still be pending.
    assert!(state.get_pending_plan_approval("p2").await.is_some());
}

#[tokio::test]
async fn test_bridge_mode() {
    let state = make_state();

    // Initially not in bridge mode.
    assert!(!state.is_bridge_mode().await);
    assert!(state.bridge_session_id().await.is_none());
    assert!(!state.is_bridge_guarded("s1").await);

    // Activate bridge mode.
    state.set_bridge_session("s1".to_string()).await;
    assert!(state.is_bridge_mode().await);
    assert_eq!(state.bridge_session_id().await.unwrap(), "s1");
    assert!(state.is_bridge_guarded("s1").await);
    assert!(!state.is_bridge_guarded("s2").await);

    // Deactivate.
    state.clear_bridge_session().await;
    assert!(!state.is_bridge_mode().await);
    assert!(!state.is_bridge_guarded("s1").await);
}

#[tokio::test]
async fn test_injection_queue() {
    let state = make_state();

    // First call creates the queue and returns the receiver.
    let (tx, rx) = state.get_or_create_injection_queue("s1").await;
    assert!(rx.is_some());
    let mut rx = rx.unwrap();

    // Second call returns the sender but no new receiver.
    let (tx2, rx2) = state.get_or_create_injection_queue("s1").await;
    assert!(rx2.is_none());

    // Send through either sender.
    tx.try_send("hello".to_string()).unwrap();
    tx2.try_send("world".to_string()).unwrap();

    assert_eq!(rx.recv().await.unwrap(), "hello");
    assert_eq!(rx.recv().await.unwrap(), "world");

    // try_inject_message works too.
    state
        .try_inject_message("s1", "via state".to_string())
        .await
        .unwrap();
    assert_eq!(rx.recv().await.unwrap(), "via state");

    // Clear and verify injection fails.
    state.clear_injection_queue("s1").await;
    assert!(
        state
            .try_inject_message("s1", "fail".to_string())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_broadcast() {
    let state = make_state();
    let mut rx = state.ws_subscribe();

    state.broadcast(WsBroadcast {
        msg_type: "test".to_string(),
        data: serde_json::json!({"hello": "world"}),
        seq: 0,
    });

    let msg = rx.recv().await.unwrap();
    assert_eq!(msg.msg_type, "test");
}

#[tokio::test]
async fn test_broadcast_seq_starts_at_1() {
    let state = make_state();
    let mut rx = state.ws_subscribe();

    state.broadcast(WsBroadcast {
        msg_type: "first".to_string(),
        data: serde_json::Value::Null,
        seq: 0,
    });

    let msg = rx.recv().await.unwrap();
    assert_eq!(msg.seq, 1, "first broadcast should get seq=1");
}

#[tokio::test]
async fn test_broadcast_assigns_seq() {
    let state = make_state();
    let mut rx = state.ws_subscribe();

    for _ in 0..3 {
        state.broadcast(WsBroadcast {
            msg_type: "evt".to_string(),
            data: serde_json::Value::Null,
            seq: 0,
        });
    }

    let m1 = rx.recv().await.unwrap();
    let m2 = rx.recv().await.unwrap();
    let m3 = rx.recv().await.unwrap();

    assert_eq!(m1.seq, 1);
    assert_eq!(m2.seq, 2);
    assert_eq!(m3.seq, 3);
}

#[test]
fn test_wsbroadcast_serde_default_seq() {
    // Deserializing JSON without a `seq` field should default to 0.
    let json = r#"{"type":"hello","data":null}"#;
    let msg: WsBroadcast = serde_json::from_str(json).unwrap();
    assert_eq!(msg.seq, 0);
    assert_eq!(msg.msg_type, "hello");
}

#[tokio::test]
async fn test_user_store_access() {
    let state = make_state();
    // Verify user store is accessible.
    assert_eq!(state.user_store().count(), 0);
}

#[tokio::test]
async fn test_model_registry_access() {
    let state = make_state();
    let registry = state.model_registry().await;
    // Empty registry by default.
    assert!(registry.providers.is_empty());
}

#[tokio::test]
async fn test_ring_buffer_stores_broadcasts() {
    let state = make_state();

    for i in 0..5 {
        state.broadcast(WsBroadcast::new(
            format!("evt_{i}"),
            serde_json::Value::Null,
        ));
    }

    let buf = state.inner.recent_broadcasts.lock().await;
    assert_eq!(buf.len(), 5);
    assert_eq!(buf[0].msg_type, "evt_0");
    assert_eq!(buf[4].msg_type, "evt_4");
}

#[tokio::test]
async fn test_ring_buffer_capacity_limit() {
    let state = make_state();

    for i in 0..(RING_BUFFER_CAPACITY + 50) {
        state.broadcast(WsBroadcast::new(
            format!("evt_{i}"),
            serde_json::Value::Null,
        ));
    }

    let buf = state.inner.recent_broadcasts.lock().await;
    assert_eq!(buf.len(), RING_BUFFER_CAPACITY);
    // Oldest should be evt_50 (first 50 were evicted).
    assert_eq!(buf.front().unwrap().msg_type, "evt_50");
}

#[tokio::test]
async fn test_catch_up_since() {
    let state = make_state();

    for _ in 0..10 {
        state.broadcast(WsBroadcast::new("evt", serde_json::Value::Null));
    }

    // Seq numbers are 1..=10. Catch up since seq 5 should return 6..=10.
    let msgs = state.catch_up_since(5).await.unwrap();
    assert_eq!(msgs.len(), 5);
    assert_eq!(msgs[0].seq, 6);
    assert_eq!(msgs[4].seq, 10);
}

#[tokio::test]
async fn test_catch_up_since_too_old() {
    let state = make_state();

    // Broadcast enough to fill the buffer, then some more to evict.
    for i in 0..(RING_BUFFER_CAPACITY + 100) {
        state.broadcast(WsBroadcast::new(
            format!("evt_{i}"),
            serde_json::Value::Null,
        ));
    }

    // The oldest seq in buffer is 101 (seqs 1..=100 were evicted).
    // Requesting catch_up_since(0) is far too old.
    assert!(state.catch_up_since(0).await.is_none());
    // Requesting catch_up_since(99) is also too old (oldest is 101, 99 < 101-1=100).
    assert!(state.catch_up_since(99).await.is_none());
    // Requesting catch_up_since(100) should work (100 >= 101-1).
    assert!(state.catch_up_since(100).await.is_some());
}

#[tokio::test]
async fn test_catch_up_since_empty_buffer() {
    let state = make_state();
    // No broadcasts yet -- should return empty vec, not None.
    let msgs = state.catch_up_since(0).await.unwrap();
    assert!(msgs.is_empty());
}
