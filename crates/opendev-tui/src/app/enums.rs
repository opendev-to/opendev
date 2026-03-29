//! Operation mode and autonomy level enums.

/// Operation mode — mirrors `OperationMode` from the Python side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationMode {
    Normal,
    Plan,
}

impl std::fmt::Display for OperationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "Normal"),
            Self::Plan => write!(f, "Plan"),
        }
    }
}

impl OperationMode {
    /// Parse from string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "plan" => Some(Self::Plan),
            _ => None,
        }
    }
}

/// Autonomy level — mirrors Python `StatusBar.autonomy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyLevel {
    Manual,
    SemiAuto,
    Auto,
}

impl std::fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "Manual"),
            Self::SemiAuto => write!(f, "Semi-Auto"),
            Self::Auto => write!(f, "Auto"),
        }
    }
}

impl AutonomyLevel {
    /// Parse from string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "manual" => Some(Self::Manual),
            "semi-auto" | "semiauto" | "semi" => Some(Self::SemiAuto),
            "auto" | "full" => Some(Self::Auto),
            _ => None,
        }
    }
}

/// Reasoning effort level for native provider thinking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningLevel {
    Off,
    Low,
    Medium,
    High,
}

impl std::fmt::Display for ReasoningLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "Off"),
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
        }
    }
}

impl ReasoningLevel {
    /// Parse from config string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "off" | "none" => Self::Off,
            "low" => Self::Low,
            "high" => Self::High,
            _ => Self::Medium,
        }
    }

    /// Convert to the config string used by LlmCallConfig.
    pub fn to_config_string(&self) -> Option<String> {
        match self {
            Self::Off => None,
            Self::Low => Some("low".to_string()),
            Self::Medium => Some("medium".to_string()),
            Self::High => Some("high".to_string()),
        }
    }

    /// Cycle to the next level: Off → Low → Medium → High → Off.
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Off,
        }
    }
}

#[cfg(test)]
#[path = "enums_tests.rs"]
mod tests;
