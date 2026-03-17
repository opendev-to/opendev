//! Tool framework foundation for OpenDev.
//!
//! This crate provides:
//! - [`traits`] — `BaseTool` async trait, `ToolResult`, `ToolContext`
//! - [`registry`] — `ToolRegistry` for tool discovery and dispatch
//! - [`middleware`] — `ToolMiddleware` trait for execution pipeline hooks
//! - [`validation`] — JSON Schema parameter validation
//! - [`normalizer`] — Parameter normalization (camelCase, path resolution)
//! - [`sanitizer`] — Result truncation to prevent context bloat
//! - [`policy`] — Tool access profiles and group-based permissions
//! - [`parallel`] — Parallel execution policy for read-only tools

pub mod middleware;
pub mod normalizer;
pub mod parallel;
pub mod path;
pub mod policy;
pub mod registry;
pub mod sanitizer;
pub mod traits;
pub mod validation;

pub use middleware::ToolMiddleware;
pub use policy::ToolPolicy;
pub use registry::ToolRegistry;
pub use sanitizer::{ToolResultSanitizer, cleanup_overflow_dir};
pub use traits::{
    BaseTool, DiagnosticProvider, FileDiagnostic, ToolContext, ToolError, ToolResult,
    ToolTimeoutConfig, ValidationError,
};
