//! Event types for the event bus system.
//!
//! Contains [`EventTopic`] for topic-based filtering, [`RuntimeEvent`] for
//! typed events, and the legacy [`Event`] struct for backward compatibility.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session_status::SessionStatus;

// ---------------------------------------------------------------------------
// Event topic -- used for subscriber interest filtering (#94)
// ---------------------------------------------------------------------------

/// Identifies the category (topic) of a [`RuntimeEvent`].
///
/// Subscribers declare which topics they care about; the bus only delivers
/// matching events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventTopic {
    /// Tool execution lifecycle events.
    Tool,
    /// LLM request / response events.
    Llm,
    /// Agent lifecycle events (start, stop, error).
    Agent,
    /// Session lifecycle events.
    Session,
    /// Cost / token usage events.
    Cost,
    /// System-level events (config reload, shutdown).
    System,
    /// Custom / user-defined events.
    Custom,
}

// ---------------------------------------------------------------------------
// RuntimeEvent -- typed event variants (#93)
// ---------------------------------------------------------------------------

/// A strongly-typed event published on the bus.
///
/// Each variant carries only the data relevant to that event kind, replacing
/// the previous stringly-typed `Event` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeEvent {
    // -- Tool events --
    /// A tool call is about to start.
    ToolCallStart {
        tool_name: String,
        call_id: String,
        timestamp_ms: u64,
    },
    /// A tool call completed.
    ToolCallEnd {
        tool_name: String,
        call_id: String,
        duration_ms: u64,
        success: bool,
        timestamp_ms: u64,
    },

    // -- LLM events --
    /// An LLM request was sent.
    LlmRequestStart {
        model: String,
        request_id: String,
        timestamp_ms: u64,
    },
    /// An LLM response was received.
    LlmResponseEnd {
        model: String,
        request_id: String,
        input_tokens: u64,
        output_tokens: u64,
        duration_ms: u64,
        timestamp_ms: u64,
    },

    // -- Agent events --
    /// An agent started working.
    AgentStart {
        agent_id: String,
        task: String,
        timestamp_ms: u64,
    },
    /// An agent finished.
    AgentEnd {
        agent_id: String,
        success: bool,
        timestamp_ms: u64,
    },
    /// An agent encountered an error.
    AgentError {
        agent_id: String,
        error: String,
        timestamp_ms: u64,
    },

    // -- Session events --
    /// Session started.
    SessionStart {
        session_id: String,
        timestamp_ms: u64,
    },
    /// Session ended.
    SessionEnd {
        session_id: String,
        timestamp_ms: u64,
    },
    /// Session status changed (idle -> busy -> retry -> idle).
    SessionStatusChanged {
        session_id: String,
        status: SessionStatus,
        timestamp_ms: u64,
    },

    // -- Cost events --
    /// Token usage was recorded.
    TokenUsage {
        model: String,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        timestamp_ms: u64,
    },

    // -- Cost events --
    /// Session cost budget has been exhausted.
    ///
    /// Published when [`CostTracker::is_over_budget`] returns `true` after
    /// recording token usage. The agent loop should pause and notify the user.
    BudgetExhausted {
        budget_usd: f64,
        total_cost_usd: f64,
        timestamp_ms: u64,
    },

    // -- System events --
    /// Configuration was reloaded.
    ConfigReloaded { timestamp_ms: u64 },
    /// Graceful shutdown requested.
    ShutdownRequested { reason: String, timestamp_ms: u64 },

    // -- Custom --
    /// Escape hatch for events not covered by the typed variants.
    Custom {
        event_type: String,
        source: String,
        data: Value,
        timestamp_ms: u64,
    },
}

impl RuntimeEvent {
    /// Return the [`EventTopic`] for this event.
    pub fn topic(&self) -> EventTopic {
        match self {
            Self::ToolCallStart { .. } | Self::ToolCallEnd { .. } => EventTopic::Tool,
            Self::LlmRequestStart { .. } | Self::LlmResponseEnd { .. } => EventTopic::Llm,
            Self::AgentStart { .. } | Self::AgentEnd { .. } | Self::AgentError { .. } => {
                EventTopic::Agent
            }
            Self::SessionStart { .. }
            | Self::SessionEnd { .. }
            | Self::SessionStatusChanged { .. } => EventTopic::Session,
            Self::TokenUsage { .. } | Self::BudgetExhausted { .. } => EventTopic::Cost,
            Self::ConfigReloaded { .. } | Self::ShutdownRequested { .. } => EventTopic::System,
            Self::Custom { .. } => EventTopic::Custom,
        }
    }

    /// Return the timestamp in milliseconds since epoch.
    pub fn timestamp_ms(&self) -> u64 {
        match self {
            Self::ToolCallStart { timestamp_ms, .. }
            | Self::ToolCallEnd { timestamp_ms, .. }
            | Self::LlmRequestStart { timestamp_ms, .. }
            | Self::LlmResponseEnd { timestamp_ms, .. }
            | Self::AgentStart { timestamp_ms, .. }
            | Self::AgentEnd { timestamp_ms, .. }
            | Self::AgentError { timestamp_ms, .. }
            | Self::SessionStart { timestamp_ms, .. }
            | Self::SessionEnd { timestamp_ms, .. }
            | Self::SessionStatusChanged { timestamp_ms, .. }
            | Self::TokenUsage { timestamp_ms, .. }
            | Self::BudgetExhausted { timestamp_ms, .. }
            | Self::ConfigReloaded { timestamp_ms, .. }
            | Self::ShutdownRequested { timestamp_ms, .. }
            | Self::Custom { timestamp_ms, .. } => *timestamp_ms,
        }
    }
}

/// Helper: current time as milliseconds since UNIX epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Legacy Event -- kept for backward compatibility
// ---------------------------------------------------------------------------

/// A legacy untyped event (kept for backward compatibility).
///
/// New code should prefer [`RuntimeEvent`] variants.
#[derive(Debug, Clone)]
pub struct Event {
    /// Event type identifier (e.g., "tool_call_start", "llm_response").
    pub event_type: String,
    /// Component that published the event.
    pub source: String,
    /// Event payload.
    pub data: Value,
    /// Timestamp (milliseconds since epoch).
    pub timestamp_ms: u64,
}

impl Event {
    /// Create a new event.
    pub fn new(event_type: impl Into<String>, source: impl Into<String>, data: Value) -> Self {
        Self {
            event_type: event_type.into(),
            source: source.into(),
            data,
            timestamp_ms: now_ms(),
        }
    }

    /// Convert a legacy `Event` into a [`RuntimeEvent::Custom`].
    pub fn into_runtime_event(self) -> RuntimeEvent {
        RuntimeEvent::Custom {
            event_type: self.event_type,
            source: self.source,
            data: self.data,
            timestamp_ms: self.timestamp_ms,
        }
    }
}
