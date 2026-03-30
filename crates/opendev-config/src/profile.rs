//! Environment-specific configuration profiles.
//!
//! Supports `OPENDEV_PROFILE` env var or `--profile` flag with values:
//! - `dev` — enables debug logging and verbose output
//! - `prod` — disables debug logging, conservative settings
//! - `fast` — reduces thinking level, lowers max tokens for speed

use opendev_models::AppConfig;
use tracing::debug;

/// Known profile names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Prod,
    Fast,
}

impl Profile {
    /// Parse a profile name from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dev" | "development" => Some(Self::Dev),
            "prod" | "production" => Some(Self::Prod),
            "fast" | "quick" => Some(Self::Fast),
            _ => None,
        }
    }

    /// Display name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Prod => "prod",
            Self::Fast => "fast",
        }
    }

    /// All known profiles.
    pub fn all() -> &'static [Profile] {
        &[Self::Dev, Self::Prod, Self::Fast]
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Apply profile-specific overrides to an AppConfig.
///
/// Returns true if a valid profile was applied, false if the name was unrecognized.
pub fn apply_profile(config: &mut AppConfig, profile_name: &str) -> bool {
    let Some(profile) = Profile::from_str_loose(profile_name) else {
        tracing::warn!("Unknown profile '{}', ignoring", profile_name);
        return false;
    };

    debug!("Applying config profile: {}", profile);

    match profile {
        Profile::Dev => {
            config.verbose = true;
            config.debug_logging = true;
        }
        Profile::Prod => {
            config.verbose = false;
        }
        Profile::Fast => {
            config.verbose = false;
            // Reduce token limits for faster responses
            if config.max_tokens > 4096 {
                config.max_tokens = 4096;
            }
            // Increase temperature slightly for faster generation
            config.temperature = 0.8;
        }
    }

    true
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
