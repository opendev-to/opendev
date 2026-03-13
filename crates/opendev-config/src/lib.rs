//! Configuration and path management for OpenDev.
//!
//! This crate handles:
//! - Hierarchical config loading (project > user > env > defaults)
//! - Path management for all application directories
//! - Model/provider registry with models.dev API cache

pub mod loader;
pub mod models_dev;
pub mod paths;

pub use loader::ConfigLoader;
pub use models_dev::{ModelInfo, ModelRegistry, ProviderInfo, sync_provider_cache_async};
pub use paths::Paths;
