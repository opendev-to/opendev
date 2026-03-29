//! Shared constants for the approval system.
//!
//! Provides canonical definitions for safe commands and autonomy levels
//! used by both TUI and Web UI approval managers.
//!
//! Ported from `opendev/core/runtime/approval/constants.py`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Autonomy levels for command approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AutonomyLevel {
    /// Every command requires manual approval.
    #[serde(rename = "Manual")]
    Manual,
    /// Safe commands auto-approved; others require approval.
    #[serde(rename = "Semi-Auto")]
    #[default]
    SemiAuto,
    /// All commands auto-approved (dangerous still flagged).
    #[serde(rename = "Auto")]
    Auto,
}

impl fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AutonomyLevel::Manual => write!(f, "Manual"),
            AutonomyLevel::SemiAuto => write!(f, "Semi-Auto"),
            AutonomyLevel::Auto => write!(f, "Auto"),
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

/// Safe commands that can be auto-approved in Semi-Auto mode.
///
/// Shared between TUI and Web approval managers.
/// Uses prefix matching: `cargo test` matches `cargo test --workspace`.
pub const SAFE_COMMANDS: &[&str] = &[
    // ── File inspection & text processing ──
    "cd",
    "ls",
    "cat",
    "head",
    "tail",
    "grep",
    "find",
    "wc",
    "pwd",
    "echo",
    "which",
    "type",
    "file",
    "stat",
    "du",
    "df",
    "tree",
    "diff",
    "md5sum",
    "sha256sum",
    "readlink",
    "basename",
    "dirname",
    "realpath",
    "sort",
    "uniq",
    "cut",
    "awk",
    "sed",
    "tr",
    "jq",
    "yq",
    "column",
    "hexdump",
    "xxd",
    "strings",
    "nm",
    "objdump",
    "ldd",
    "tar tf",
    "zip -l",
    "unzip -l",
    // ── Git (read-only) ──
    "git status",
    "git log",
    "git diff",
    "git branch",
    "git show",
    "git remote",
    "git tag",
    "git stash list",
    "git blame",
    "git rev-parse",
    "git ls-files",
    "git config --get",
    "git stash show",
    "git shortlog",
    "git describe",
    // ── Build & test tools ──
    "cargo check",
    "cargo build",
    "cargo test",
    "cargo clippy",
    "cargo fmt",
    "cargo doc",
    "cargo add",
    "cargo update",
    "cargo install",
    "npm run",
    "npm test",
    "npm ci",
    "npm install",
    "npm list",
    "npm outdated",
    "npx",
    "yarn install",
    "yarn list",
    "pnpm install",
    "bun install",
    "make",
    "cmake",
    "ninja",
    "go build",
    "go test",
    "go vet",
    "go get",
    "go mod tidy",
    "go mod download",
    "pip install",
    "pip list",
    "pip show",
    "pip freeze",
    "pipenv install",
    "poetry install",
    "poetry show",
    "gem install",
    "gem list",
    "bundle install",
    "bundle list",
    "composer install",
    "composer show",
    "brew install",
    "brew list",
    "brew info",
    "bazel build",
    "bazel test",
    "gradle build",
    "gradle test",
    "mvn compile",
    "mvn test",
    "sbt compile",
    "sbt test",
    // ── Language runtimes & version checks ──
    "python --version",
    "python3 --version",
    "node --version",
    "npm --version",
    "cargo --version",
    "go version",
    "ruby --version",
    "ruby -v",
    "java --version",
    "javac --version",
    "dotnet --version",
    "php --version",
    "perl --version",
    "swift --version",
    "kotlin -version",
    "scala -version",
    "elixir --version",
    "lua -v",
    "deno --version",
    "bun --version",
    "rustc --version",
    "rustup show",
    // ── Linters & formatters ──
    "eslint",
    "prettier",
    "black",
    "ruff",
    "flake8",
    "mypy",
    "pylint",
    "rubocop",
    "gofmt",
    "golangci-lint",
    "shellcheck",
    "tsc",
    "biome",
    // ── Testing frameworks ──
    "pytest",
    "jest",
    "vitest",
    "mocha",
    "rspec",
    "phpunit",
    "dotnet test",
    "flutter test",
    // ── CI/CD & containers (read-only) ──
    "docker ps",
    "docker images",
    "docker logs",
    "docker inspect",
    "docker compose ps",
    "kubectl get",
    "kubectl describe",
    "kubectl logs",
    "gh pr list",
    "gh pr view",
    "gh issue list",
    "gh issue view",
    "gh run list",
    "gh run view",
    "terraform plan",
    "terraform show",
    // ── System info ──
    "uname",
    "env",
    "printenv",
    "whoami",
    "hostname",
    "date",
    "uptime",
    "id",
    "lsof",
    "netstat",
    "ss",
    "dig",
    "nslookup",
    "ping",
    "traceroute",
    "ifconfig",
    "ip addr",
    "ps",
    "pgrep",
    "free",
    "vmstat",
    "iostat",
    "top -l 1",
    "curl",
    "wget",
];

/// Check if a command is considered safe for auto-approval.
///
/// Performs shell-aware parsing:
/// 1. Rejects commands containing dangerous shell constructs (`$(...)`, backticks)
/// 2. Splits on shell operators (`&&`, `||`, `;`, `|`) and checks **every** segment
/// 3. For each segment, strips leading env vars (`KEY=val`) and path prefixes (`/usr/bin/git`)
/// 4. Matches the normalized command against `SAFE_COMMANDS` using prefix matching
pub fn is_safe_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Reject commands with shell injection constructs.
    if contains_shell_injection(trimmed) {
        return false;
    }

    // Split on shell operators and verify ALL segments are safe.
    let segments = split_shell_segments(trimmed);
    if segments.is_empty() {
        return false;
    }
    segments.iter().all(|seg| is_segment_safe(seg))
}

/// Returns true if the command string contains dangerous shell constructs.
fn contains_shell_injection(cmd: &str) -> bool {
    // Command substitution: $(...) or `...`
    if cmd.contains("$(") || cmd.contains('`') {
        return true;
    }
    // Process substitution: <(...) or >(...)
    if cmd.contains("<(") || cmd.contains(">(") {
        return true;
    }
    // File output redirects (but allow fd redirects like 2>&1)
    if contains_file_redirect(cmd) {
        return true;
    }
    false
}

/// Check if command contains a file output redirect (e.g. `> file`, `>> file`).
/// Allows fd redirects like `2>&1`, `>&2`.
fn contains_file_redirect(cmd: &str) -> bool {
    let bytes = cmd.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'\'' {
            i += 1;
            while i < len && bytes[i] != b'\'' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'>' {
            if i + 1 < len && bytes[i + 1] == b'&' {
                i += 2;
                continue;
            }
            if i > 0 && bytes[i - 1].is_ascii_digit() && i + 1 < len && bytes[i + 1] == b'&' {
                i += 2;
                continue;
            }
            return true;
        }
        i += 1;
    }
    false
}

/// Split a command string on shell operators: `&&`, `||`, `;`, `|`.
fn split_shell_segments(cmd: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let bytes = cmd.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'\'' {
            i += 1;
            while i < len && bytes[i] != b'\'' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            while i < len {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }

        if i + 1 < len
            && ((bytes[i] == b'&' && bytes[i + 1] == b'&')
                || (bytes[i] == b'|' && bytes[i + 1] == b'|'))
        {
            let seg = cmd[start..i].trim();
            if !seg.is_empty() {
                segments.push(seg);
            }
            i += 2;
            start = i;
            continue;
        }
        if bytes[i] == b';' || (bytes[i] == b'|' && (i + 1 >= len || bytes[i + 1] != b'|')) {
            let seg = cmd[start..i].trim();
            if !seg.is_empty() {
                segments.push(seg);
            }
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }

    let seg = cmd[start..].trim();
    if !seg.is_empty() {
        segments.push(seg);
    }
    segments
}

/// Check if a single command segment (no shell operators) is safe.
fn is_segment_safe(segment: &str) -> bool {
    let normalized = normalize_segment(segment);
    if normalized.is_empty() {
        return false;
    }
    let cmd_lower = normalized.to_lowercase();
    SAFE_COMMANDS.iter().any(|safe| {
        let safe_lower = safe.to_lowercase();
        cmd_lower == safe_lower || cmd_lower.starts_with(&format!("{safe_lower} "))
    })
}

/// Normalize a command segment by stripping leading env vars and path prefixes.
fn normalize_segment(segment: &str) -> String {
    let mut parts: Vec<&str> = segment.split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }

    while !parts.is_empty() && is_env_assignment(parts[0]) {
        parts.remove(0);
    }
    if parts.is_empty() {
        return String::new();
    }

    if let Some(basename) = parts[0].rsplit('/').next()
        && !basename.is_empty()
    {
        parts[0] = basename;
    }

    parts.join(" ")
}

/// Check if a token looks like a shell env var assignment: `KEY=VALUE`.
fn is_env_assignment(token: &str) -> bool {
    if let Some(eq_pos) = token.find('=') {
        if eq_pos == 0 {
            return false;
        }
        let name = &token[..eq_pos];
        let mut chars = name.chars();
        if let Some(first) = chars.next()
            && (first.is_ascii_alphabetic() || first == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return true;
        }
    }
    false
}

/// Tools that use subcommands — matching should include the subcommand.
const MULTI_WORD_TOOLS: &[&str] = &[
    "cargo",
    "git",
    "npm",
    "yarn",
    "pnpm",
    "go",
    "pip",
    "pipenv",
    "poetry",
    "gem",
    "bundle",
    "composer",
    "brew",
    "bazel",
    "gradle",
    "mvn",
    "sbt",
    "docker",
    "kubectl",
    "gh",
    "terraform",
    "dotnet",
    "flutter",
];

/// Extract a command prefix for auto-approval patterns.
///
/// For multi-word tools (e.g. `cargo test`, `git status`), returns the
/// first two tokens. For single-word tools (e.g. `eslint`), returns one.
/// Strips leading env var assignments and path prefixes.
pub fn extract_command_prefix(command: &str) -> String {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return String::new();
    }

    let mut start = 0;
    while start < parts.len() && is_env_assignment(parts[start]) {
        start += 1;
    }

    if start >= parts.len() {
        return String::new();
    }

    let binary = parts[start].rsplit('/').next().unwrap_or(parts[start]);

    let bin_lower = binary.to_lowercase();
    if MULTI_WORD_TOOLS.contains(&bin_lower.as_str())
        && start + 1 < parts.len()
        && !parts[start + 1].starts_with('-')
    {
        return format!("{} {}", binary, parts[start + 1]);
    }

    binary.to_string()
}

#[cfg(test)]
#[path = "constants_tests.rs"]
mod tests;
