use super::*;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn test_web_search_missing_query() {
    let tool = WebSearchTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("query is required"));
}

#[tokio::test]
async fn test_web_search_empty_query() {
    let tool = WebSearchTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("query", serde_json::json!("  "))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("query is required"));
}
