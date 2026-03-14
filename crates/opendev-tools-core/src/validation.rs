//! Basic JSON Schema validation for tool parameters.
//!
//! Validates tool arguments against the tool's `parameter_schema()` before
//! execution. Checks required fields, type correctness, and enum constraints.

use std::collections::HashMap;

/// Validate tool arguments against a JSON Schema.
///
/// Performs basic validation:
/// - Checks that all `required` fields are present
/// - Validates `type` for each property (string, number, integer, boolean, array, object)
/// - Validates `enum` constraints
///
/// Returns `Ok(())` if valid, or `Err(message)` describing the first violation.
pub fn validate_args(
    args: &HashMap<String, serde_json::Value>,
    schema: &serde_json::Value,
) -> Result<(), String> {
    // Check required fields
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for req in required {
            if let Some(field_name) = req.as_str()
                && !args.contains_key(field_name)
            {
                return Err(format!("Missing required parameter: '{field_name}'"));
            }
        }
    }

    // Check property types
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (key, value) in args {
            if let Some(prop_schema) = properties.get(key) {
                validate_value_type(key, value, prop_schema)?;
            }
            // Extra properties not in schema are allowed (no additionalProperties check)
        }
    }

    Ok(())
}

/// Validate a single value against its property schema.
fn validate_value_type(
    key: &str,
    value: &serde_json::Value,
    prop_schema: &serde_json::Value,
) -> Result<(), String> {
    // Check enum constraint
    if let Some(enum_values) = prop_schema.get("enum").and_then(|e| e.as_array())
        && !enum_values.contains(value)
    {
        return Err(format!(
            "Parameter '{key}' value {value} is not one of the allowed values: {enum_values:?}"
        ));
    }

    // Check type constraint
    if let Some(expected_type) = prop_schema.get("type").and_then(|t| t.as_str()) {
        let type_ok = match expected_type {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.is_i64() || value.is_u64(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            "null" => value.is_null(),
            _ => true, // Unknown type, allow
        };

        if !type_ok {
            // Allow integer where number is expected
            if expected_type == "number" && (value.is_i64() || value.is_u64()) {
                return Ok(());
            }
            return Err(format!(
                "Parameter '{key}' expected type '{expected_type}', got {}",
                json_type_name(value)
            ));
        }
    }

    Ok(())
}

/// Get a human-readable type name for a JSON value.
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
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
}
