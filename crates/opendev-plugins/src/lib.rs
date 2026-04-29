//! Plugin and marketplace management for opendev.
//!
//! This crate provides:
//! - Plugin discovery from project and global plugin directories
//! - Plugin install/uninstall/enable/disable lifecycle
//! - Marketplace management: add, remove, sync, catalog browsing
//! - HTTP-based remote catalog fetching

pub mod manager;
pub mod marketplace;
pub mod models;

pub use manager::{PluginError, PluginManager, PluginPaths};
pub use models::{
    InstalledPlugins, KnownMarketplaces, MarketplaceCatalog, MarketplaceInfo, PluginConfig,
    PluginManifest, PluginMetadata, PluginScope, PluginSource, PluginStatus, PromptTemplate,
    ToolDefinition,
};

pub mod fs_utils;
