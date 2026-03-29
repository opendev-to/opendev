use super::*;
use serde_json::json;

fn make_tool_schema(name: &str, params: Value) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": "A test tool",
            "parameters": params
        }
    })
}

#[test]
fn test_no_adaptation_for_openai() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({"type": "object", "properties": {"a": {"type": "string"}}}),
    )];
    let result = adapt_for_provider(&schemas, "openai");
    assert_eq!(result, schemas);
}

#[test]
fn test_no_adaptation_for_anthropic() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({"type": "object", "properties": {}}),
    )];
    let result = adapt_for_provider(&schemas, "anthropic");
    assert_eq!(result, schemas);
}

#[test]
fn test_gemini_strips_additional_properties() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({
            "type": "object",
            "properties": {
                "a": {"type": "string", "default": "hello", "format": "uri"}
            },
            "additionalProperties": false,
            "$schema": "http://json-schema.org/draft-07/schema#"
        }),
    )];
    let result = adapt_for_provider(&schemas, "gemini");
    let params = result[0].pointer("/function/parameters").unwrap();
    assert!(params.get("additionalProperties").is_none());
    assert!(params.get("$schema").is_none());
    let prop_a = &params["properties"]["a"];
    assert!(prop_a.get("default").is_none());
    assert!(prop_a.get("format").is_none());
    assert_eq!(prop_a["type"], "string");
}

#[test]
fn test_gemini_strips_nested_keys() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({
            "type": "object",
            "properties": {
                "nested": {
                    "type": "object",
                    "properties": {
                        "deep": {"type": "number", "default": 42}
                    },
                    "additionalProperties": false
                }
            }
        }),
    )];
    let result = adapt_for_provider(&schemas, "gemini");
    let nested = &result[0]["function"]["parameters"]["properties"]["nested"];
    assert!(nested.get("additionalProperties").is_none());
    assert!(nested["properties"]["deep"].get("default").is_none());
}

#[test]
fn test_xai_filters_web_search() {
    let schemas = vec![
        make_tool_schema("web_search", json!({"type": "object", "properties": {}})),
        make_tool_schema("read_file", json!({"type": "object", "properties": {}})),
    ];
    let result = adapt_for_provider(&schemas, "xai");
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].pointer("/function/name").unwrap().as_str(),
        Some("read_file")
    );
}

#[test]
fn test_xai_no_web_search_unchanged() {
    let schemas = vec![make_tool_schema(
        "read_file",
        json!({"type": "object", "properties": {}}),
    )];
    let result = adapt_for_provider(&schemas, "xai");
    assert_eq!(result.len(), 1);
}

#[test]
fn test_mistral_flattens_any_of() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({
            "type": "object",
            "properties": {
                "value": {
                    "anyOf": [
                        {"type": "string"},
                        {"type": "number"}
                    ]
                }
            }
        }),
    )];
    let result = adapt_for_provider(&schemas, "mistral");
    let prop = &result[0]["function"]["parameters"]["properties"]["value"];
    assert!(prop.get("anyOf").is_none());
    assert_eq!(prop["type"], "string");
}

#[test]
fn test_mistral_flattens_one_of() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({
            "type": "object",
            "properties": {
                "value": {
                    "oneOf": [
                        {"type": "integer"},
                        {"type": "boolean"}
                    ]
                }
            }
        }),
    )];
    let result = adapt_for_provider(&schemas, "mistral");
    let prop = &result[0]["function"]["parameters"]["properties"]["value"];
    assert!(prop.get("oneOf").is_none());
    assert_eq!(prop["type"], "integer");
}

#[test]
fn test_mistral_merges_all_of() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({
            "type": "object",
            "properties": {
                "value": {
                    "allOf": [
                        {"type": "string"},
                        {"minLength": 1}
                    ]
                }
            }
        }),
    )];
    let result = adapt_for_provider(&schemas, "mistral");
    let prop = &result[0]["function"]["parameters"]["properties"]["value"];
    assert!(prop.get("allOf").is_none());
    assert_eq!(prop["type"], "string");
    assert_eq!(prop["minLength"], 1);
}

#[test]
fn test_general_cleanup_adds_type() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({"properties": {"a": {"type": "string"}}}),
    )];
    let result = adapt_for_provider(&schemas, "fireworks");
    let params = &result[0]["function"]["parameters"];
    assert_eq!(params["type"], "object");
}

#[test]
fn test_general_cleanup_adds_properties() {
    let schemas = vec![make_tool_schema("test", json!({"type": "object"}))];
    let result = adapt_for_provider(&schemas, "fireworks");
    let params = &result[0]["function"]["parameters"];
    assert!(params.get("properties").is_some());
}

#[test]
fn test_case_insensitive_provider() {
    let schemas = vec![make_tool_schema(
        "web_search",
        json!({"type": "object", "properties": {}}),
    )];
    let result = adapt_for_provider(&schemas, "XAI");
    assert_eq!(result.len(), 0);
}

#[test]
fn test_does_not_mutate_input() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({
            "type": "object",
            "properties": {"a": {"type": "string", "default": "hi"}},
            "additionalProperties": false
        }),
    )];
    let original = schemas[0].clone();
    let _result = adapt_for_provider(&schemas, "gemini");
    assert_eq!(schemas[0], original);
}

#[test]
fn test_empty_schemas() {
    let schemas: Vec<Value> = vec![];
    let result = adapt_for_provider(&schemas, "gemini");
    assert!(result.is_empty());
}

#[test]
fn test_google_alias_for_gemini() {
    let schemas = vec![make_tool_schema(
        "test",
        json!({
            "type": "object",
            "properties": {"a": {"type": "string", "default": "x"}},
        }),
    )];
    let result = adapt_for_provider(&schemas, "google");
    let prop = &result[0]["function"]["parameters"]["properties"]["a"];
    assert!(prop.get("default").is_none());
}

#[test]
fn test_grok_alias_for_xai() {
    let schemas = vec![make_tool_schema(
        "web_search",
        json!({"type": "object", "properties": {}}),
    )];
    let result = adapt_for_provider(&schemas, "grok");
    assert!(result.is_empty());
}
