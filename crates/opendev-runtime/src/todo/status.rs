use serde::{Deserialize, Serialize};

/// Status of a todo item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

impl std::fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "todo"),
            Self::InProgress => write!(f, "doing"),
            Self::Completed => write!(f, "done"),
        }
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
