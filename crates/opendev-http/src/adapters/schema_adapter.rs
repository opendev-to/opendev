//! Provider-specific schema adaptation.
//!
//! Different LLM providers have different JSON Schema requirements for tool
//! definitions. This module applies provider-specific transformations to tool
//! schemas before they are sent to the LLM.

use serde_json::Value;
use tracing::debug;

/// Apply provider-specific schema transformations.
///
/// This is a pure function — does not mutate the input schemas.
/// Returns a (possibly deep-copied) list of adapted schemas.
pub fn adapt_for_provider(schemas: &[Value], provider: &str) -> Vec<Value> {
    let provider = provider.to_lowercase();

    // No adaptation needed for standard providers
    if matches!(provider.as_str(), "openai" | "anthropic" | "openrouter") {
        return schemas.to_vec();
    }

    // Deep copy to avoid mutating originals
    let mut adapted: Vec<Value> = schemas.to_vec();
    let mut modified = false;

    #[allow(clippy::collapsible_match)]
    match provider.as_str() {
        "gemini" | "google" => {
            if adapt_gemini(&mut adapted) {
                modified = true;
            }
        }
        "xai" | "grok" => {
            if adapt_xai(&mut adapted) {
                modified = true;
            }
        }
        "mistral" => {
            if adapt_mistral(&mut adapted) {
                modified = true;
            }
        }
        _ => {}
    }

    // General cleanup for all non-standard providers
    if general_cleanup(&mut adapted) {
        modified = true;
    }

    if modified {
        debug!(
            count = adapted.len(),
            provider = provider.as_str(),
            "Adapted tool schemas for provider"
        );
    }

    adapted
}

/// Gemini rejects `additionalProperties`, `default`, `$schema`, `format`
/// in nested schemas.
fn adapt_gemini(schemas: &mut [Value]) -> bool {
    const KEYS_TO_STRIP: &[&str] = &["additionalProperties", "default", "$schema", "format"];
    let mut changed = false;
    for schema in schemas.iter_mut() {
        if let Some(params) = schema.pointer_mut("/function/parameters")
            && strip_keys_recursive(params, KEYS_TO_STRIP)
        {
            changed = true;
        }
    }
    changed
}

/// xAI/Grok has a native `web_search` that conflicts with our tool.
fn adapt_xai(schemas: &mut Vec<Value>) -> bool {
    let before = schemas.len();
    schemas.retain(|schema| {
        let name = schema
            .pointer("/function/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        name != "web_search"
    });
    let removed = before != schemas.len();
    if removed {
        debug!("Filtered out web_search tool for xAI provider (native conflict)");
    }
    removed
}

/// Mistral doesn't support `anyOf`/`oneOf`/`allOf` — flatten to simple types.
fn adapt_mistral(schemas: &mut [Value]) -> bool {
    let mut changed = false;
    for schema in schemas.iter_mut() {
        if let Some(params) = schema.pointer_mut("/function/parameters")
            && flatten_union_types(params)
        {
            changed = true;
        }
    }
    changed
}

/// Ensure schemas follow basic requirements for all providers.
fn general_cleanup(schemas: &mut [Value]) -> bool {
    let mut changed = false;
    for schema in schemas.iter_mut() {
        if let Some(params) = schema.pointer_mut("/function/parameters")
            && let Some(obj) = params.as_object_mut()
        {
            if !obj.contains_key("type") {
                obj.insert("type".to_string(), Value::String("object".to_string()));
                changed = true;
            }
            if !obj.contains_key("properties") {
                obj.insert(
                    "properties".to_string(),
                    Value::Object(serde_json::Map::new()),
                );
                changed = true;
            }
        }
    }
    changed
}

/// Recursively remove specified keys from a JSON value.
fn strip_keys_recursive(obj: &mut Value, keys: &[&str]) -> bool {
    match obj {
        Value::Object(map) => {
            let mut changed = false;
            let keys_present: Vec<String> = map
                .keys()
                .filter(|k| keys.contains(&k.as_str()))
                .cloned()
                .collect();
            for key in keys_present {
                map.remove(&key);
                changed = true;
            }
            for value in map.values_mut() {
                if strip_keys_recursive(value, keys) {
                    changed = true;
                }
            }
            changed
        }
        Value::Array(arr) => {
            let mut changed = false;
            for item in arr.iter_mut() {
                if strip_keys_recursive(item, keys) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

/// Replace `anyOf`/`oneOf`/`allOf` with flattened variants (lossy but compatible).
fn flatten_union_types(obj: &mut Value) -> bool {
    let Some(map) = obj.as_object_mut() else {
        return false;
    };

    let mut changed = false;

    // Handle anyOf/oneOf: take first variant
    for union_key in &["anyOf", "oneOf"] {
        if let Some(variants) = map.remove(*union_key) {
            if let Some(arr) = variants.as_array()
                && let Some(first) = arr.first()
                && let Some(first_obj) = first.as_object()
            {
                for (k, v) in first_obj {
                    map.insert(k.clone(), v.clone());
                }
            }
            changed = true;
        }
    }

    // Handle allOf: merge all variants
    if let Some(variants) = map.remove("allOf") {
        if let Some(arr) = variants.as_array() {
            for variant in arr {
                if let Some(variant_obj) = variant.as_object() {
                    for (k, v) in variant_obj {
                        map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        changed = true;
    }

    // Recurse into nested objects and arrays
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        if let Some(value) = map.get_mut(&key) {
            #[allow(clippy::collapsible_match)]
            match value {
                Value::Object(_) => {
                    if flatten_union_types(value) {
                        changed = true;
                    }
                }
                Value::Array(arr) => {
                    for item in arr.iter_mut() {
                        if flatten_union_types(item) {
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    changed
}

#[cfg(test)]
#[path = "schema_adapter_tests.rs"]
mod tests;
