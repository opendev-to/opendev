use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A request to run a sandboxed analysis session.
#[derive(Debug, Clone)]
pub struct SandboxRequest {
    /// The task/question to accomplish.
    pub query: String,
    /// Optional context to inject into the sandbox.
    pub context: Option<SandboxContext>,
}

/// Context to inject into the sandbox as a Python variable.
#[derive(Debug, Clone)]
pub enum SandboxContext {
    /// Raw text content injected as a string variable.
    Text { name: String, content: String },
    /// File path — contents are read and injected as a string variable.
    File { path: PathBuf },
}

/// Result from a completed sandboxed analysis session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    /// The final answer extracted via FINAL() or FINAL_VAR().
    pub answer: String,
    /// Number of sandbox loop iterations executed.
    pub iterations: u32,
    /// Number of lm_query() sub-LLM calls made from the sandbox.
    pub lm_query_count: u32,
    /// Total tokens consumed (root LLM + sub-LLM calls).
    pub total_tokens: u64,
}
