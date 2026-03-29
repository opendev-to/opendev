use thiserror::Error;

pub type Result<T> = std::result::Result<T, SandboxError>;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error(
        "Microsandbox server not available: {0}. Start with `msb server start` or install from https://microsandbox.dev"
    )]
    ServerUnavailable(String),

    #[error("Sandbox creation failed: {0}")]
    SandboxCreation(String),

    #[error("Python execution failed: {0}")]
    Execution(String),

    #[error("Execution timed out after {seconds}s")]
    Timeout { seconds: f64 },

    #[error("Sandbox loop exceeded {max_iterations} iterations without producing a FINAL() answer")]
    MaxIterations { max_iterations: u32 },

    #[error("Cost limit exceeded: ${spent:.4} of ${budget:.4} budget")]
    CostLimitExceeded { spent: f64, budget: f64 },

    #[error("LM query limit exceeded: {count}/{max} sub-LLM calls")]
    LmQueryLimitExceeded { count: u32, max: u32 },

    #[error("Callback server error: {0}")]
    CallbackServer(String),

    #[error("LLM call failed: {0}")]
    LlmCall(String),

    #[error("Cancelled by user")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}
