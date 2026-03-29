use super::*;
use serde_json::json;

fn make_schema(properties: serde_json::Value, required: Vec<&str>) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

#[test]
fn test_validate_required_present() {
    let schema = make_schema(json!({"name": {"type": "string"}}), vec!["name"]);
    let mut args = HashMap::new();
    args.insert("name".into(), json!("hello"));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_required_missing() {
    let schema = make_schema(json!({"name": {"type": "string"}}), vec!["name"]);
    let args = HashMap::new();
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Missing required parameter: 'name'")
    );
}

#[test]
fn test_validate_type_string_ok() {
    let schema = make_schema(json!({"name": {"type": "string"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("name".into(), json!("hello"));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_type_string_wrong() {
    let schema = make_schema(json!({"name": {"type": "string"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("name".into(), json!(42));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("expected type 'string'"));
}

#[test]
fn test_validate_type_number_ok() {
    let schema = make_schema(json!({"count": {"type": "number"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("count".into(), json!(3.14));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_type_integer_as_number() {
    let schema = make_schema(json!({"count": {"type": "number"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("count".into(), json!(42));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_type_integer_ok() {
    let schema = make_schema(json!({"count": {"type": "integer"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("count".into(), json!(42));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_type_integer_rejects_float() {
    let schema = make_schema(json!({"count": {"type": "integer"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("count".into(), json!(3.14));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
}

#[test]
fn test_validate_type_boolean_ok() {
    let schema = make_schema(json!({"flag": {"type": "boolean"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("flag".into(), json!(true));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_type_array_ok() {
    let schema = make_schema(json!({"items": {"type": "array"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("items".into(), json!([1, 2, 3]));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_type_object_ok() {
    let schema = make_schema(json!({"meta": {"type": "object"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("meta".into(), json!({"key": "val"}));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_enum_ok() {
    let schema = make_schema(
        json!({"mode": {"type": "string", "enum": ["fast", "slow"]}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("mode".into(), json!("fast"));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_enum_invalid() {
    let schema = make_schema(
        json!({"mode": {"type": "string", "enum": ["fast", "slow"]}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("mode".into(), json!("turbo"));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("not one of the allowed values")
    );
}

#[test]
fn test_validate_extra_properties_allowed() {
    let schema = make_schema(json!({"name": {"type": "string"}}), vec![]);
    let mut args = HashMap::new();
    args.insert("name".into(), json!("hello"));
    args.insert("extra".into(), json!("world"));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_empty_schema() {
    let schema = json!({});
    let mut args = HashMap::new();
    args.insert("anything".into(), json!("goes"));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_no_required() {
    let schema = make_schema(json!({"name": {"type": "string"}}), vec![]);
    let args = HashMap::new();
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_multiple_required_one_missing() {
    let schema = make_schema(
        json!({
            "name": {"type": "string"},
            "age": {"type": "integer"}
        }),
        vec!["name", "age"],
    );
    let mut args = HashMap::new();
    args.insert("name".into(), json!("Alice"));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("age"));
}

// --- validate_args_detailed tests ---

#[test]
fn test_detailed_returns_all_errors() {
    let schema = make_schema(
        json!({
            "name": {"type": "string"},
            "age": {"type": "integer"},
            "email": {"type": "string"}
        }),
        vec!["name", "age", "email"],
    );
    let args = HashMap::new(); // all three missing
    let errors = super::validate_args_detailed(&args, &schema);
    assert_eq!(errors.len(), 3);
    let paths: Vec<&str> = errors.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"name"));
    assert!(paths.contains(&"age"));
    assert!(paths.contains(&"email"));
}

#[test]
fn test_detailed_mixed_missing_and_wrong_type() {
    let schema = make_schema(
        json!({
            "name": {"type": "string"},
            "count": {"type": "integer"}
        }),
        vec!["name"],
    );
    let mut args = HashMap::new();
    // missing "name" (required) + wrong type for "count"
    args.insert("count".into(), json!("not_a_number"));
    let errors = super::validate_args_detailed(&args, &schema);
    assert_eq!(errors.len(), 2);
}

#[test]
fn test_detailed_empty_on_valid() {
    let schema = make_schema(json!({"name": {"type": "string"}}), vec!["name"]);
    let mut args = HashMap::new();
    args.insert("name".into(), json!("Alice"));
    let errors = super::validate_args_detailed(&args, &schema);
    assert!(errors.is_empty());
}

#[test]
fn test_detailed_error_has_path_and_message() {
    let schema = make_schema(json!({"file": {"type": "string"}}), vec!["file"]);
    let args = HashMap::new();
    let errors = super::validate_args_detailed(&args, &schema);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].path, "file");
    assert!(errors[0].message.contains("Missing required"));
}

// --- minLength / maxLength tests ---

#[test]
fn test_validate_min_length_ok() {
    let schema = make_schema(
        json!({"name": {"type": "string", "minLength": 1}}),
        vec!["name"],
    );
    let mut args = HashMap::new();
    args.insert("name".into(), json!("hello"));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_min_length_empty_string_rejected() {
    let schema = make_schema(
        json!({"name": {"type": "string", "minLength": 1}}),
        vec!["name"],
    );
    let mut args = HashMap::new();
    args.insert("name".into(), json!(""));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("below minimum"));
}

#[test]
fn test_validate_max_length_rejected() {
    let schema = make_schema(json!({"name": {"type": "string", "maxLength": 5}}), vec![]);
    let mut args = HashMap::new();
    args.insert("name".into(), json!("toolong"));
    assert!(validate_args(&args, &schema).is_err());
}

// --- minItems / maxItems tests ---

#[test]
fn test_validate_max_items_ok() {
    let schema = make_schema(json!({"items": {"type": "array", "maxItems": 3}}), vec![]);
    let mut args = HashMap::new();
    args.insert("items".into(), json!(["a", "b"]));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_max_items_exceeded() {
    let schema = make_schema(json!({"items": {"type": "array", "maxItems": 2}}), vec![]);
    let mut args = HashMap::new();
    args.insert("items".into(), json!(["a", "b", "c"]));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("maximum is 2"));
}

#[test]
fn test_validate_min_items_rejected() {
    let schema = make_schema(json!({"items": {"type": "array", "minItems": 1}}), vec![]);
    let mut args = HashMap::new();
    args.insert("items".into(), json!([]));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("minimum is 1"));
}

// --- Array items validation ---

#[test]
fn test_validate_array_items_string_type() {
    let schema = make_schema(
        json!({"tags": {"type": "array", "items": {"type": "string"}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("tags".into(), json!(["a", "b"]));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_array_items_wrong_type() {
    let schema = make_schema(
        json!({"tags": {"type": "array", "items": {"type": "string"}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("tags".into(), json!(["a", 42]));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("tags[1]"));
}

#[test]
fn test_validate_array_items_min_length() {
    let schema = make_schema(
        json!({"tags": {"type": "array", "items": {"type": "string", "minLength": 1}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("tags".into(), json!(["ok", ""]));
    let result = validate_args(&args, &schema);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("tags[1]"), "Expected tags[1] in: {err}");
    assert!(
        err.contains("below minimum"),
        "Expected 'below minimum' in: {err}"
    );
}

// --- oneOf tests ---

#[test]
fn test_validate_array_one_of_string_accepted() {
    let schema = make_schema(
        json!({"todos": {"type": "array", "items": {"oneOf": [
            {"type": "string", "minLength": 1},
            {"type": "object", "properties": {"content": {"type": "string", "minLength": 1}}, "required": ["content"]}
        ]}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("todos".into(), json!(["Step A", "Step B"]));
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_array_one_of_object_accepted() {
    let schema = make_schema(
        json!({"todos": {"type": "array", "items": {"oneOf": [
            {"type": "string", "minLength": 1},
            {"type": "object", "properties": {"content": {"type": "string", "minLength": 1}}, "required": ["content"]}
        ]}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert(
        "todos".into(),
        json!([{"content": "Step A"}, {"content": "Step B"}]),
    );
    assert!(validate_args(&args, &schema).is_ok());
}

#[test]
fn test_validate_array_one_of_empty_string_rejected() {
    let schema = make_schema(
        json!({"todos": {"type": "array", "items": {"oneOf": [
            {"type": "string", "minLength": 1},
            {"type": "object", "properties": {"content": {"type": "string", "minLength": 1}}, "required": ["content"]}
        ]}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("todos".into(), json!(["Valid", ""]));
    let result = validate_args(&args, &schema);
    assert!(result.is_err(), "Empty string should be rejected by oneOf");
}

#[test]
fn test_validate_array_one_of_empty_content_rejected() {
    let schema = make_schema(
        json!({"todos": {"type": "array", "items": {"oneOf": [
            {"type": "string", "minLength": 1},
            {"type": "object", "properties": {"content": {"type": "string", "minLength": 1}}, "required": ["content"]}
        ]}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert(
        "todos".into(),
        json!([{"content": "Valid"}, {"content": ""}]),
    );
    let result = validate_args(&args, &schema);
    assert!(
        result.is_err(),
        "Object with empty content should be rejected"
    );
}

#[test]
fn test_validate_array_one_of_missing_content_rejected() {
    let schema = make_schema(
        json!({"todos": {"type": "array", "items": {"oneOf": [
            {"type": "string", "minLength": 1},
            {"type": "object", "properties": {"content": {"type": "string", "minLength": 1}}, "required": ["content"]}
        ]}}}),
        vec![],
    );
    let mut args = HashMap::new();
    args.insert("todos".into(), json!([{"status": "in_progress"}]));
    let result = validate_args(&args, &schema);
    assert!(
        result.is_err(),
        "Object missing required 'content' should be rejected"
    );
}
