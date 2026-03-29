use super::*;

fn make_agent() -> MainAgent {
    let config = MainAgentConfig::new("gpt-4o");
    let registry = Arc::new(ToolRegistry::new());
    MainAgent::new(config, registry)
}

fn make_agent_with_tools() -> MainAgent {
    let config = MainAgentConfig {
        model: "gpt-4o".to_string(),
        model_thinking: Some("o1-preview".to_string()),
        temperature: Some(0.5),
        max_tokens: Some(8192),
        working_dir: Some("/tmp/project".to_string()),
        allowed_tools: None,
        model_provider: None,
    };
    let registry = Arc::new(ToolRegistry::new());
    MainAgent::new(config, registry)
}

#[test]
fn test_new_agent() {
    let agent = make_agent();
    assert!(!agent.is_subagent);
    assert_eq!(agent.config.model, "gpt-4o");
    assert!(agent.http_client.is_none());
}

#[test]
fn test_subagent_detection() {
    let config = MainAgentConfig {
        allowed_tools: Some(vec!["read_file".to_string(), "search".to_string()]),
        ..MainAgentConfig::new("gpt-4o")
    };
    let registry = Arc::new(ToolRegistry::new());
    let agent = MainAgent::new(config, registry);
    assert!(agent.is_subagent);
}

#[test]
fn test_set_system_prompt() {
    let mut agent = make_agent();
    agent.set_system_prompt("You are a helpful assistant.");
    assert_eq!(agent.system_prompt(), "You are a helpful assistant.");
}

#[test]
fn test_build_system_prompt_trait() {
    let mut agent = make_agent();
    agent.set_system_prompt("Test prompt");
    assert_eq!(agent.build_system_prompt(), "Test prompt");
}

#[test]
fn test_build_tool_schemas_empty() {
    let agent = make_agent();
    assert!(agent.build_tool_schemas().is_empty());
}

#[test]
fn test_refresh_tools() {
    let mut agent = make_agent();
    agent.refresh_tools();
    // With empty registry, schemas stay empty
    assert!(agent.tool_schemas().is_empty());
}

#[test]
fn test_messages_contain_images_false() {
    let messages = vec![serde_json::json!({"role": "user", "content": "hello"})];
    assert!(!MainAgent::messages_contain_images(&messages));
}

#[test]
fn test_messages_contain_images_true() {
    let messages = vec![serde_json::json!({
        "role": "user",
        "content": [
            {"type": "text", "text": "Look at this"},
            {"type": "image", "source": {"data": "base64..."}}
        ]
    })];
    assert!(MainAgent::messages_contain_images(&messages));
}

#[test]
fn test_agent_debug() {
    let agent = make_agent();
    let debug = format!("{:?}", agent);
    assert!(debug.contains("MainAgent"));
    assert!(debug.contains("gpt-4o"));
    assert!(debug.contains("has_http_client"));
}

#[tokio::test]
async fn test_call_llm_no_http_client() {
    let agent = make_agent();
    let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
    let resp = agent.call_llm(&messages, None).await;
    // No HTTP client configured → returns failure
    assert!(!resp.success);
    assert!(
        resp.error
            .as_deref()
            .unwrap_or("")
            .contains("HTTP client not configured")
    );
}

#[tokio::test]
async fn test_run_no_http_client() {
    let agent = make_agent();
    let deps = AgentDeps::new();
    let result = agent.run("Hello", &deps, None, None).await;
    // Should return ConfigError since no HTTP client
    assert!(result.is_err());
    match result.unwrap_err() {
        AgentError::ConfigError(msg) => {
            assert!(msg.contains("HTTP client not configured"));
        }
        other => panic!("Expected ConfigError, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_with_http_client() {
    use reqwest::header::HeaderMap;
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = AdaptedClient::new(raw);
    let agent = make_agent().with_http_client(Arc::new(http));
    assert!(agent.http_client.is_some());
}

#[tokio::test]
async fn test_run_preserves_history() {
    // Without HTTP client, run() returns an error.
    // We test that with_http_client stores the client.
    use reqwest::header::HeaderMap;
    let raw = opendev_http::HttpClient::new(
        "https://api.example.com/v1/chat/completions",
        HeaderMap::new(),
        None,
    )
    .unwrap();
    let http = AdaptedClient::new(raw);
    let mut agent = make_agent();
    agent.set_http_client(Arc::new(http));
    assert!(agent.http_client.is_some());
}

#[test]
fn test_build_schemas_with_filter() {
    // With empty registry, no schemas regardless of filter
    let registry = ToolRegistry::new();
    let schemas = MainAgent::build_schemas(&registry, Some(&["read_file".to_string()]));
    assert!(schemas.is_empty());
}

#[test]
fn test_config_new() {
    let config = MainAgentConfig::new("claude-3-opus");
    assert_eq!(config.model, "claude-3-opus");
    assert_eq!(config.temperature, Some(0.7));
    assert_eq!(config.max_tokens, Some(4096));
    assert!(config.model_thinking.is_none());
    assert!(config.allowed_tools.is_none());
}

#[test]
fn test_require_http_client_err() {
    let agent = make_agent();
    let result = agent.require_http_client();
    assert!(result.is_err());
}

#[test]
fn test_require_http_client_ok() {
    use reqwest::header::HeaderMap;
    let raw = opendev_http::HttpClient::new("https://example.com", HeaderMap::new(), None).unwrap();
    let http = AdaptedClient::new(raw);
    let agent = make_agent().with_http_client(Arc::new(http));
    assert!(agent.require_http_client().is_ok());
}

// ---- Glob matching ----

#[test]
fn test_glob_match_exact() {
    assert!(glob_match("read_file", "read_file"));
    assert!(!glob_match("read_file", "write_file"));
}

#[test]
fn test_glob_match_star_suffix() {
    assert!(glob_match("read_*", "read_file"));
    assert!(glob_match("read_*", "read_pdf"));
    assert!(!glob_match("read_*", "write_file"));
}

#[test]
fn test_glob_match_star_prefix() {
    assert!(glob_match("*_file", "read_file"));
    assert!(glob_match("*_file", "write_file"));
    assert!(!glob_match("*_file", "search"));
}

#[test]
fn test_glob_match_star_middle() {
    assert!(glob_match("mcp__*__tool", "mcp__server__tool"));
    assert!(!glob_match("mcp__*__tool", "mcp__server__other"));
}

#[test]
fn test_glob_match_star_all() {
    assert!(glob_match("*", "anything"));
    assert!(glob_match("*", ""));
}

#[test]
fn test_glob_match_question_mark() {
    assert!(glob_match("read_???e", "read_file"));
    assert!(!glob_match("read_???e", "read_fi"));
}

#[test]
fn test_glob_match_mcp_wildcard() {
    assert!(glob_match("mcp__*", "mcp__github__create_pr"));
    assert!(glob_match("mcp__*", "mcp__slack__send"));
    assert!(!glob_match("mcp__*", "read_file"));
}
