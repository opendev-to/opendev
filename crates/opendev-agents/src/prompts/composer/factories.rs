//! Factory functions that create pre-configured [`PromptComposer`] instances.
//!
//! Each factory registers the appropriate set of prompt sections for
//! the default agent mode.

use std::path::Path;

use super::CachePolicy;
use super::PromptComposer;
use super::conditions::{ctx_bool, ctx_eq, ctx_in, ctx_present};

/// Create a default composer with standard sections registered.
///
/// Priority ranges:
/// - 10-30: Core identity and policies (always loaded, `Static`)
/// - 40-50: Tool guidance and interaction patterns (`Static`)
/// - 55-65: Code quality and workflows (`Static`)
/// - 70-80: Conditional sections (git, MCP, etc.)
/// - 85-95: Context-specific additions (`Cached` for dynamic content)
pub fn create_default_composer(templates_dir: impl AsRef<Path>) -> PromptComposer {
    let mut composer = PromptComposer::new(templates_dir.as_ref());

    // Core sections (always included) - Priority 10-30, Static
    composer.register_section_with_policy(
        "mode_awareness",
        "system/main/main-mode-awareness.md",
        None,
        12,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "security_policy",
        "system/main/main-security-policy.md",
        None,
        15,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "tone_and_style",
        "system/main/main-tone-and-style.md",
        None,
        20,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "output_efficiency",
        "system/main/main-output-efficiency.md",
        None,
        22,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "no_time_estimates",
        "system/main/main-no-time-estimates.md",
        None,
        25,
        CachePolicy::Static,
    );

    // Tool guidance - Priority 45, Static
    composer.register_section_with_policy(
        "tool_selection",
        "system/main/main-tool-selection.md",
        None,
        45,
        CachePolicy::Static,
    );

    // Code quality and workflows - Priority 55-65, Static
    composer.register_section_with_policy(
        "code_quality",
        "system/main/main-code-quality.md",
        None,
        55,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "action_safety",
        "system/main/main-action-safety.md",
        None,
        56,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "error_recovery",
        "system/main/main-error-recovery.md",
        None,
        60,
        CachePolicy::Static,
    );

    // Conditional sections - Priority 65-80, Static
    composer.register_section_with_policy(
        "subagent_guide",
        "system/main/main-subagent-guide.md",
        Some(ctx_bool("has_subagents")),
        65,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "agent_team_guide",
        "system/main/main-agent-team-guide.md",
        Some(ctx_bool("has_agent_teams")),
        66,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "git_workflow",
        "system/main/main-git-workflow.md",
        Some(ctx_bool("in_git_repo")),
        70,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "verification",
        "system/main/main-verification.md",
        None,
        72,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "task_tracking",
        "system/main/main-task-tracking.md",
        Some(ctx_bool("todo_tracking_enabled")),
        75,
        CachePolicy::Static,
    );

    // Provider-specific sections - Priority 80, Static
    composer.register_section_with_policy(
        "provider_openai",
        "system/main/main-provider-openai.md",
        Some(ctx_eq("model_provider", "openai")),
        80,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "provider_anthropic",
        "system/main/main-provider-anthropic.md",
        Some(ctx_eq("model_provider", "anthropic")),
        80,
        CachePolicy::Static,
    );
    composer.register_section_with_policy(
        "provider_fireworks",
        "system/main/main-provider-fireworks.md",
        Some(ctx_in("model_provider", &["fireworks", "fireworks-ai"])),
        80,
        CachePolicy::Static,
    );

    // Context awareness - Priority 85-95
    composer.register_section_with_policy(
        "output_awareness",
        "system/main/main-output-awareness.md",
        None,
        85,
        CachePolicy::Static,
    );
    // Auto memory: Cached — content may change if memory files are updated mid-session
    composer.register_section_with_policy(
        "auto_memory",
        "system/main/main-auto-memory.md",
        None,
        86,
        CachePolicy::Cached,
    );
    // Scratchpad: Cached — session-specific, refreshed on /clear or /compact
    composer.register_section_with_policy(
        "scratchpad",
        "system/main/main-scratchpad.md",
        Some(ctx_present("session_id")),
        87,
        CachePolicy::Cached,
    );
    composer.register_section_with_policy(
        "code_references",
        "system/main/main-code-references.md",
        None,
        90,
        CachePolicy::Static,
    );
    // System reminders note: Cached — refreshed on /clear or /compact
    composer.register_section_with_policy(
        "system_reminders_note",
        "system/main/main-reminders-note.md",
        None,
        95,
        CachePolicy::Cached,
    );

    composer
}

/// Create the appropriate composer for a given mode.
pub fn create_composer(templates_dir: impl AsRef<Path>, _mode: &str) -> PromptComposer {
    create_default_composer(templates_dir)
}

#[cfg(test)]
#[path = "factories_tests.rs"]
mod tests;
