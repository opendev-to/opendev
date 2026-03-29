//! # opendev-sandbox
//!
//! Sandboxed Python code execution for context analysis with recursive LLM sub-calls.
//!
//! Implements an iterative loop: the LLM generates Python code, which
//! executes in a microsandbox microVM. The sandbox has access to context
//! variables and an `lm_query()` function for sub-LLM calls. The loop
//! repeats until the LLM calls `FINAL(answer)`.

pub mod callback;
pub mod errors;
pub mod models;
pub mod parser;
pub mod prompts;
pub mod sandbox;
pub mod session;

pub use errors::{Result, SandboxError};
pub use models::{SandboxContext, SandboxRequest, SandboxResult};
pub use sandbox::{MicroSandbox, SandboxPool};
pub use session::SandboxSession;
