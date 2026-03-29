use super::*;

/// A simple test adapter that records sent messages.
struct TestAdapter {
    name: String,
    sent: Arc<RwLock<Vec<OutboundMessage>>>,
}

impl TestAdapter {
    fn new(name: &str) -> (Arc<Self>, Arc<RwLock<Vec<OutboundMessage>>>) {
        let sent = Arc::new(RwLock::new(Vec::new()));
        let adapter = Arc::new(Self {
            name: name.to_string(),
            sent: sent.clone(),
        });
        (adapter, sent)
    }
}

#[async_trait]
impl ChannelAdapter for TestAdapter {
    fn channel_name(&self) -> &str {
        &self.name
    }

    async fn send(
        &self,
        _delivery_context: &DeliveryContext,
        message: OutboundMessage,
    ) -> ChannelResult<()> {
        let mut sent = self.sent.write().await;
        sent.push(message);
        Ok(())
    }
}

/// A test executor that echoes messages.
struct EchoExecutor;

#[async_trait]
impl AgentExecutor for EchoExecutor {
    async fn execute(&self, _session_id: &str, message_text: &str) -> ChannelResult<String> {
        Ok(format!("Echo: {}", message_text))
    }
}

#[tokio::test]
async fn test_router_new() {
    let router = MessageRouter::new();
    assert_eq!(router.adapter_count().await, 0);
    assert_eq!(router.session_count().await, 0);
}

#[tokio::test]
async fn test_register_adapter() {
    let router = MessageRouter::new();
    let (adapter, _) = TestAdapter::new("test-channel");

    router.register_adapter(adapter).await;
    assert_eq!(router.adapter_count().await, 1);
    assert!(router.get_adapter("test-channel").await.is_some());
    assert!(router.get_adapter("nonexistent").await.is_none());
}

#[tokio::test]
async fn test_channel_names() {
    let router = MessageRouter::new();
    let (adapter1, _) = TestAdapter::new("cli");
    let (adapter2, _) = TestAdapter::new("web");

    router.register_adapter(adapter1).await;
    router.register_adapter(adapter2).await;

    let mut names = router.channel_names().await;
    names.sort();
    assert_eq!(names, vec!["cli", "web"]);
}

#[tokio::test]
async fn test_handle_inbound_no_adapter() {
    let router = MessageRouter::new();

    let message = InboundMessage {
        channel: "unknown".to_string(),
        user_id: "user1".to_string(),
        thread_id: None,
        text: "hello".to_string(),
        timestamp: Utc::now(),
        chat_type: "direct".to_string(),
        reply_to_message_id: None,
        metadata: HashMap::new(),
    };

    let result = router.handle_inbound(message).await;
    assert!(matches!(result, Err(ChannelError::AdapterNotFound(_))));
}

#[tokio::test]
async fn test_handle_inbound_with_executor() {
    let router = MessageRouter::new();
    let (adapter, sent) = TestAdapter::new("test");
    router.register_adapter(adapter).await;
    router.set_executor(Arc::new(EchoExecutor)).await;

    let message = InboundMessage {
        channel: "test".to_string(),
        user_id: "user1".to_string(),
        thread_id: None,
        text: "hello world".to_string(),
        timestamp: Utc::now(),
        chat_type: "direct".to_string(),
        reply_to_message_id: None,
        metadata: HashMap::new(),
    };

    router.handle_inbound(message).await.unwrap();

    let sent = sent.read().await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].text, "Echo: hello world");
}

#[tokio::test]
async fn test_session_reuse() {
    let router = MessageRouter::new();
    let (adapter, sent) = TestAdapter::new("test");
    router.register_adapter(adapter).await;
    router.set_executor(Arc::new(EchoExecutor)).await;

    // Send two messages from the same user
    for text in &["msg1", "msg2"] {
        let message = InboundMessage {
            channel: "test".to_string(),
            user_id: "user1".to_string(),
            thread_id: None,
            text: text.to_string(),
            timestamp: Utc::now(),
            chat_type: "direct".to_string(),
            reply_to_message_id: None,
            metadata: HashMap::new(),
        };
        router.handle_inbound(message).await.unwrap();
    }

    // Should still be 1 session (reused)
    assert_eq!(router.session_count().await, 1);

    let sent = sent.read().await;
    assert_eq!(sent.len(), 2);
}

#[tokio::test]
async fn test_different_users_different_sessions() {
    let router = MessageRouter::new();
    let (adapter, _) = TestAdapter::new("test");
    router.register_adapter(adapter).await;
    router.set_executor(Arc::new(EchoExecutor)).await;

    for user in &["user1", "user2"] {
        let message = InboundMessage {
            channel: "test".to_string(),
            user_id: user.to_string(),
            thread_id: None,
            text: "hello".to_string(),
            timestamp: Utc::now(),
            chat_type: "direct".to_string(),
            reply_to_message_id: None,
            metadata: HashMap::new(),
        };
        router.handle_inbound(message).await.unwrap();
    }

    assert_eq!(router.session_count().await, 2);
}

#[tokio::test]
async fn test_inbound_message_serialization() {
    let msg = InboundMessage {
        channel: "telegram".to_string(),
        user_id: "12345".to_string(),
        thread_id: Some("thread-1".to_string()),
        text: "Hello bot".to_string(),
        timestamp: Utc::now(),
        chat_type: "group".to_string(),
        reply_to_message_id: None,
        metadata: HashMap::new(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: InboundMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.channel, "telegram");
    assert_eq!(deserialized.user_id, "12345");
    assert_eq!(deserialized.thread_id.as_deref(), Some("thread-1"));
}
