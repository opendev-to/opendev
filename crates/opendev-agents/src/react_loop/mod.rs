//! ReAct loop: reason → decide tool → execute → observe → loop.
//!
//! Mirrors `opendev/core/agents/main_agent/run_loop.py`.
//! The loop iterates up to a configurable maximum, executing tool calls
//! and feeding results back to the LLM until it completes or is interrupted.

mod compaction;
mod config;
mod emitter;
mod execution;
mod helpers;
mod loop_state;
mod phases;
pub(crate) mod streaming_executor;
mod types;

pub use config::ReactLoopConfig;
pub use types::{IterationMetrics, PARALLELIZABLE_TOOLS, ToolCallMetric, TurnResult};

use std::collections::HashSet;
use std::sync::Mutex;

use crate::response::ResponseCleaner;

use types::PARALLELIZABLE_TOOLS as PARALLEL;

/// The ReAct (Reason-Act) execution loop.
///
/// Orchestrates the cycle of LLM calls and tool executions, handling:
/// - Iteration limits
/// - Interrupt checking
/// - Nudging on failed tools or implicit completion
/// - Parallel execution of read-only tools
/// - Todo completion checking
/// - Doom-loop cycle detection
pub struct ReactLoop {
    pub(super) config: ReactLoopConfig,
    _cleaner: ResponseCleaner,
    pub(super) parallelizable: HashSet<&'static str>,
    /// Accumulated per-iteration metrics over the session.
    pub(super) iteration_metrics: Mutex<Vec<IterationMetrics>>,
}

impl ReactLoop {
    /// Create a new ReAct loop with the given configuration.
    pub fn new(config: ReactLoopConfig) -> Self {
        Self {
            config,
            _cleaner: ResponseCleaner::new(),
            iteration_metrics: Mutex::new(Vec::new()),
            parallelizable: PARALLEL.iter().copied().collect(),
        }
    }

    /// Create a ReAct loop with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(ReactLoopConfig::default())
    }
}
