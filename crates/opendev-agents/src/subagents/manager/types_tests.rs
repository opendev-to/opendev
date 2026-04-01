use super::*;

#[test]
fn test_subagent_type_from_name() {
    assert_eq!(
        SubagentType::from_name("Explore"),
        SubagentType::CodeExplorer
    );
    assert_eq!(
        SubagentType::from_name("Code-Explorer"),
        SubagentType::CodeExplorer
    );
    assert_eq!(SubagentType::from_name("Planner"), SubagentType::Planner);
    assert_eq!(SubagentType::from_name("General"), SubagentType::General);
    assert_eq!(SubagentType::from_name("general"), SubagentType::General);
    assert_eq!(SubagentType::from_name("Build"), SubagentType::Build);
    assert_eq!(SubagentType::from_name("build"), SubagentType::Build);
    assert_eq!(
        SubagentType::from_name("Verification"),
        SubagentType::Verification
    );
    assert_eq!(
        SubagentType::from_name("verification"),
        SubagentType::Verification
    );
    assert_eq!(SubagentType::from_name("unknown"), SubagentType::Custom);
}

#[test]
fn test_subagent_type_canonical_name() {
    assert_eq!(SubagentType::CodeExplorer.canonical_name(), "Explore");
    assert_eq!(SubagentType::General.canonical_name(), "General");
    assert_eq!(SubagentType::Build.canonical_name(), "Build");
    assert_eq!(SubagentType::Verification.canonical_name(), "Verification");
}

// --- SubagentEventBridge tests ---

/// Mock progress callback that records events.
struct RecordingProgressCallback {
    events: std::sync::Mutex<Vec<String>>,
}

impl RecordingProgressCallback {
    fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl SubagentProgressCallback for RecordingProgressCallback {
    fn on_started(&self, name: &str, task: &str) {
        self.events
            .lock()
            .unwrap()
            .push(format!("started:{name}:{task}"));
    }
    fn on_tool_call(
        &self,
        name: &str,
        tool: &str,
        id: &str,
        _args: &HashMap<String, serde_json::Value>,
    ) {
        self.events
            .lock()
            .unwrap()
            .push(format!("tool_call:{name}:{tool}:{id}"));
    }
    fn on_tool_complete(&self, name: &str, _tool: &str, id: &str, success: bool) {
        self.events
            .lock()
            .unwrap()
            .push(format!("tool_complete:{name}:{id}:{success}"));
    }
    fn on_finished(&self, name: &str, success: bool, _summary: &str) {
        self.events
            .lock()
            .unwrap()
            .push(format!("finished:{name}:{success}"));
    }
    fn on_token_usage(&self, name: &str, input: u64, output: u64) {
        self.events
            .lock()
            .unwrap()
            .push(format!("tokens:{name}:{input}:{output}"));
    }
}

#[test]
fn test_event_bridge_forwards_tool_started() {
    let recorder = Arc::new(RecordingProgressCallback::new());
    let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
    let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

    use crate::traits::AgentEventCallback;
    let args = std::collections::HashMap::new();
    bridge.on_tool_started("tc-1", "read_file", &args);

    let events = recorder.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], "tool_call:test-agent:read_file:tc-1");
}

#[test]
fn test_event_bridge_forwards_tool_finished() {
    let recorder = Arc::new(RecordingProgressCallback::new());
    let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
    let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

    use crate::traits::AgentEventCallback;
    bridge.on_tool_finished("tc-1", true);

    let events = recorder.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], "tool_complete:test-agent:tc-1:true");
}

#[test]
fn test_event_bridge_forwards_token_usage() {
    let recorder = Arc::new(RecordingProgressCallback::new());
    let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
    let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

    use crate::traits::AgentEventCallback;
    bridge.on_token_usage(1000, 500);

    let events = recorder.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], "tokens:test-agent:1000:500");
}

#[test]
fn test_event_bridge_noop_methods() {
    let recorder = Arc::new(RecordingProgressCallback::new());
    let progress: Arc<dyn SubagentProgressCallback> = Arc::clone(&recorder) as _;
    let bridge = SubagentEventBridge::new("test-agent".to_string(), progress);

    use crate::traits::AgentEventCallback;
    // These should not produce any events
    bridge.on_agent_chunk("hello");

    assert!(recorder.events().is_empty());
}
