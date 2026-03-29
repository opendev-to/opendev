//! Command classification patterns and environment variable filtering.
//!
//! Contains regex-based detection for dangerous, server, and interactive
//! commands, plus sensitive environment variable filtering.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

// ---------------------------------------------------------------------------
// Sensitive environment variable patterns (stripped from child processes)
// ---------------------------------------------------------------------------

/// Env var name suffixes that indicate API keys, tokens, or secrets.
/// These are removed from child process environments to prevent leakage.
const SENSITIVE_ENV_SUFFIXES: &[&str] = &[
    "_API_KEY",
    "_SECRET_KEY",
    "_SECRET",
    "_TOKEN",
    "_PASSWORD",
    "_CREDENTIALS",
];

/// Specific env var names to always strip (case-sensitive).
const SENSITIVE_ENV_EXACT: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "AZURE_OPENAI_API_KEY",
    "GROQ_API_KEY",
    "MISTRAL_API_KEY",
    "DEEPINFRA_API_KEY",
    "OPENROUTER_API_KEY",
    "FIREWORKS_API_KEY",
    "GOOGLE_API_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "NPM_TOKEN",
    "PYPI_TOKEN",
];

/// Check if an environment variable name is sensitive and should be stripped.
pub(crate) fn is_sensitive_env(name: &str) -> bool {
    let upper = name.to_uppercase();
    if SENSITIVE_ENV_EXACT.iter().any(|&e| upper == e) {
        return true;
    }
    SENSITIVE_ENV_SUFFIXES
        .iter()
        .any(|suffix| upper.ends_with(suffix))
}

/// Build a filtered environment map: inherits all env vars except sensitive ones.
pub(crate) fn filtered_env() -> HashMap<String, String> {
    std::env::vars()
        .filter(|(key, _)| !is_sensitive_env(key))
        .collect()
}

// ---------------------------------------------------------------------------
// Dangerous-command regex patterns
// ---------------------------------------------------------------------------

const DANGEROUS_REGEX_PATTERNS: &[&str] = &[
    r"rm\s+-rf\s+/",
    r"curl.*\|\s*(ba)?sh",
    r"wget.*\|\s*(ba)?sh",
    r"sudo\s+",
    r"mkfs",
    r"dd\s+.*of=",
    r"chmod\s+-R\s+777\s+/",
    r":\(\)\{.*:\|:&\s*\};:",
    r"mv\s+/",
    r">\s*/dev/sd[a-z]",
    // Git destructive operations (--force but not --force-with-lease)
    r"git\s+push\s+.*--force\b",
    r"git\s+push\s+-f\b",
    r"git\s+reset\s+--hard",
    r"git\s+clean\s+-[a-zA-Z]*f",
    r"git\s+checkout\s+--\s+\.",
    r"git\s+branch\s+-D\b",
];

// ---------------------------------------------------------------------------
// Interactive-command patterns (auto-confirm with `yes |`)
// ---------------------------------------------------------------------------

const INTERACTIVE_PATTERNS: &[&str] = &[
    r"\bnpx\b",
    r"\bnpm\s+(init|create)\b",
    r"\byarn\s+create\b",
    r"\bng\s+new\b",
    r"\bvue\s+create\b",
    r"\bcreate-react-app\b",
    r"\bnext\s+create\b",
    r"\bvite\s+create\b",
    r"\bpnpm\s+create\b",
    r"\bpip\s+install\b",
];

// ---------------------------------------------------------------------------
// Server-command patterns (auto-promote to background)
// ---------------------------------------------------------------------------

const SERVER_PATTERNS: &[&str] = &[
    // Python web servers
    r"flask\s+run",
    r"python.*manage\.py\s+runserver",
    r"uvicorn",
    r"gunicorn",
    r"python.*-m\s+http\.server",
    r"hypercorn",
    r"daphne",
    r"waitress",
    r"fastapi",
    // Node.js
    r"npm\s+(run\s+)?(start|dev|serve)",
    r"yarn\s+(run\s+)?(start|dev|serve)",
    r"pnpm\s+(run\s+)?(start|dev|serve)",
    r"bun\s+(run\s+)?(start|dev|serve)",
    r"node.*server",
    r"nodemon",
    r"next\s+(dev|start)",
    r"nuxt\s+(dev|start)",
    r"vite(\s+dev)?$",
    r"webpack.*(dev.?server|serve)",
    // Ruby / PHP / Other
    r"rails\s+server",
    r"php.*artisan\s+serve",
    r"php\s+-S\s+",
    r"hugo\s+server",
    r"jekyll\s+serve",
    // Go
    r"go\s+run.*server",
    // Rust
    r"cargo\s+(run|watch)",
    // Java
    r"mvn.*spring-boot:run",
    r"gradle.*bootRun",
    // Generic
    r"live-server",
    r"http-server",
    r"serve\s+-",
    r"browser-sync",
    r"docker\s+compose\s+up",
];

// ---------------------------------------------------------------------------
// Regex cache helpers
// ---------------------------------------------------------------------------

/// Pre-compiled regex set for pattern matching. Avoids recompiling on every call.
struct CompiledPatterns {
    regexes: Vec<Regex>,
}

impl CompiledPatterns {
    fn new(patterns: &[&str]) -> Self {
        Self {
            regexes: patterns.iter().filter_map(|p| Regex::new(p).ok()).collect(),
        }
    }

    fn matches(&self, text: &str) -> bool {
        self.regexes.iter().any(|re| re.is_match(text))
    }
}

static DANGEROUS_COMPILED: LazyLock<CompiledPatterns> =
    LazyLock::new(|| CompiledPatterns::new(DANGEROUS_REGEX_PATTERNS));

static SERVER_COMPILED: LazyLock<CompiledPatterns> =
    LazyLock::new(|| CompiledPatterns::new(SERVER_PATTERNS));

static INTERACTIVE_COMPILED: LazyLock<CompiledPatterns> =
    LazyLock::new(|| CompiledPatterns::new(INTERACTIVE_PATTERNS));

pub(crate) fn is_dangerous(command: &str) -> bool {
    if DANGEROUS_COMPILED.matches(command) {
        // Allow `git push --force-with-lease` (safe force push)
        if command.contains("--force-with-lease") && !command.contains("--force ") {
            return false;
        }
        return true;
    }
    false
}

pub(crate) fn is_server_command(command: &str) -> bool {
    SERVER_COMPILED.matches(command)
}

pub(crate) fn needs_auto_confirm(command: &str) -> bool {
    INTERACTIVE_COMPILED.matches(command)
}

#[cfg(test)]
#[path = "patterns_tests.rs"]
mod tests;
