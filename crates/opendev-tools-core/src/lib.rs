//! Tool framework foundation for OpenDev.
//!
//! This crate provides:
//! - [`traits`] — `BaseTool` async trait, `ToolResult`, `ToolContext`
//! - [`registry`] — `ToolRegistry` for tool discovery and dispatch
//! - [`normalizer`] — Parameter normalization (camelCase, path resolution)
//! - [`sanitizer`] — Result truncation to prevent context bloat
//! - [`policy`] — Tool access profiles and group-based permissions
//! - [`parallel`] — Parallel execution policy for read-only tools

pub mod normalizer;
pub mod parallel;
pub mod policy;
pub mod registry;
pub mod sanitizer;
pub mod traits;

pub use policy::ToolPolicy;
pub use registry::ToolRegistry;
pub use sanitizer::ToolResultSanitizer;
pub use traits::{BaseTool, ToolContext, ToolError, ToolResult, ToolTimeoutConfig};
