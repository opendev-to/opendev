//! Config version migration support.
//!
//! Adds a `config_version` field to config files and applies migration
//! functions when loading configs with older versions.

use tracing::{debug, info};

/// Current config version. Increment this when adding migrations.
pub const CURRENT_CONFIG_VERSION: u32 = 2;

/// Key used to store the config version in JSON.
pub const VERSION_KEY: &str = "config_version";

/// Migrate a JSON config value from its stored version to the current version.
///
/// Returns the migrated JSON value and whether any migrations were applied.
pub fn migrate_config(mut value: serde_json::Value) -> (serde_json::Value, bool) {
    let stored_version = value
        .as_object()
        .and_then(|obj| obj.get(VERSION_KEY))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0);

    if stored_version >= CURRENT_CONFIG_VERSION {
        // Already at current version, just ensure version field is set
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                VERSION_KEY.to_string(),
                serde_json::Value::Number(CURRENT_CONFIG_VERSION.into()),
            );
        }
        return (value, false);
    }

    let mut version = stored_version;
    let mut migrated = false;

    // Apply migrations in order
    while version < CURRENT_CONFIG_VERSION {
        match version {
            0 => {
                // Migration from version 0 (no version field) to version 1:
                // - Add config_version field
                // - No structural changes needed for v1; this just stamps the version.
                value = migrate_v0_to_v1(value);
                version = 1;
                migrated = true;
                info!("Migrated config from v0 to v1");
            }
            1 => {
                // Migration from version 1 to version 2:
                // - Move model_compact/model_compact_provider into agents.compact
                // - Remove model_critique/model_critique_provider (dead code)
                value = migrate_v1_to_v2(value);
                version = 2;
                migrated = true;
                info!("Migrated config from v1 to v2");
            }
            _ => {
                // Unknown version — skip remaining migrations
                debug!(
                    "Unknown config version {}, skipping further migrations",
                    version
                );
                break;
            }
        }
    }

    (value, migrated)
}

/// Migrate from version 0 (unversioned) to version 1.
///
/// Version 1 simply stamps the config_version field. Future migrations
/// can add structural changes here.
fn migrate_v0_to_v1(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        obj.insert(VERSION_KEY.to_string(), serde_json::Value::Number(1.into()));
    }
    value
}

/// Migrate from version 1 to version 2.
///
/// - Moves `model_compact` / `model_compact_provider` into `agents.compact`
/// - Removes `model_critique` / `model_critique_provider` (dead code, never
///   consumed by any runtime path)
fn migrate_v1_to_v2(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        // Extract compact fields before removing them
        let compact_model = obj
            .remove("model_compact")
            .and_then(|v| v.as_str().map(String::from));
        let compact_provider = obj
            .remove("model_compact_provider")
            .and_then(|v| v.as_str().map(String::from));

        // Remove critique fields (dead code)
        obj.remove("model_critique");
        obj.remove("model_critique_provider");

        // If either compact field was set, write into agents.compact
        if compact_model.is_some() || compact_provider.is_some() {
            let agents = obj.entry("agents").or_insert_with(|| serde_json::json!({}));
            if let Some(agents_obj) = agents.as_object_mut() {
                let mut compact_entry = serde_json::Map::new();
                if let Some(model) = compact_model {
                    compact_entry.insert("model".to_string(), serde_json::Value::String(model));
                }
                if let Some(provider) = compact_provider {
                    compact_entry
                        .insert("provider".to_string(), serde_json::Value::String(provider));
                }
                agents_obj.insert(
                    "compact".to_string(),
                    serde_json::Value::Object(compact_entry),
                );
            }
        }

        // Stamp version
        obj.insert(VERSION_KEY.to_string(), serde_json::Value::Number(2.into()));
    }
    value
}

/// Check whether a config value needs migration.
pub fn needs_migration(value: &serde_json::Value) -> bool {
    let stored_version = value
        .as_object()
        .and_then(|obj| obj.get(VERSION_KEY))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0);

    stored_version < CURRENT_CONFIG_VERSION
}

/// Get the version of a config value.
pub fn config_version(value: &serde_json::Value) -> u32 {
    value
        .as_object()
        .and_then(|obj| obj.get(VERSION_KEY))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
