//! Factory functions that create pre-configured [`PromptComposer`] instances.
//!
//! Each factory registers the appropriate set of prompt sections for a
//! particular agent mode (default, thinking, etc.).

use std::path::Path;

use super::PromptComposer;
use super::conditions::{ctx_bool, ctx_eq, ctx_in, ctx_present};

/// Create a default composer with standard sections registered.
///
/// Priority ranges:
/// - 10-30: Core identity and policies (always loaded)
/// - 40-50: Tool guidance and interaction patterns
/// - 55-65: Code quality and workflows
/// - 70-80: Conditional sections (git, MCP, etc.)
/// - 85-95: Context-specific additions
pub fn create_default_composer(templates_dir: impl AsRef<Path>) -> PromptComposer {
    let mut composer = PromptComposer::new(templates_dir.as_ref());

    // Core sections (always included) - Priority 10-30
    composer.register_section(
        "mode_awareness",
        "system/main/main-mode-awareness.md",
        None,
        12,
        true,
    );
    composer.register_section(
        "security_policy",
        "system/main/main-security-policy.md",
        None,
        15,
        true,
    );
    composer.register_section(
        "tone_and_style",
        "system/main/main-tone-and-style.md",
        None,
        20,
        true,
    );
    composer.register_section(
        "no_time_estimates",
        "system/main/main-no-time-estimates.md",
        None,
        25,
        true,
    );

    // Interaction patterns - Priority 40-50
    composer.register_section(
        "interaction_pattern",
        "system/main/main-interaction-pattern.md",
        None,
        40,
        true,
    );
    composer.register_section(
        "available_tools",
        "system/main/main-available-tools.md",
        None,
        45,
        true,
    );
    composer.register_section(
        "tool_selection",
        "system/main/main-tool-selection.md",
        None,
        50,
        true,
    );

    // Code quality and workflows - Priority 55-65
    composer.register_section(
        "code_quality",
        "system/main/main-code-quality.md",
        None,
        55,
        true,
    );
    composer.register_section(
        "action_safety",
        "system/main/main-action-safety.md",
        None,
        56,
        true,
    );
    composer.register_section(
        "read_before_edit",
        "system/main/main-read-before-edit.md",
        None,
        58,
        true,
    );
    composer.register_section(
        "error_recovery",
        "system/main/main-error-recovery.md",
        None,
        60,
        true,
    );

    // Conditional sections - Priority 65-80
    composer.register_section(
        "subagent_guide",
        "system/main/main-subagent-guide.md",
        Some(ctx_bool("has_subagents")),
        65,
        true,
    );
    composer.register_section(
        "git_workflow",
        "system/main/main-git-workflow.md",
        Some(ctx_bool("in_git_repo")),
        70,
        true,
    );
    composer.register_section(
        "verification",
        "system/main/main-verification.md",
        None,
        72,
        true,
    );
    composer.register_section(
        "task_tracking",
        "system/main/main-task-tracking.md",
        Some(ctx_bool("todo_tracking_enabled")),
        75,
        true,
    );

    // Provider-specific sections - Priority 80
    composer.register_section(
        "provider_openai",
        "system/main/main-provider-openai.md",
        Some(ctx_eq("model_provider", "openai")),
        80,
        true,
    );
    composer.register_section(
        "provider_anthropic",
        "system/main/main-provider-anthropic.md",
        Some(ctx_eq("model_provider", "anthropic")),
        80,
        true,
    );
    composer.register_section(
        "provider_fireworks",
        "system/main/main-provider-fireworks.md",
        Some(ctx_in("model_provider", &["fireworks", "fireworks-ai"])),
        80,
        true,
    );

    // Context awareness - Priority 85-95
    composer.register_section(
        "output_awareness",
        "system/main/main-output-awareness.md",
        None,
        85,
        true,
    );
    composer.register_section(
        "scratchpad",
        "system/main/main-scratchpad.md",
        Some(ctx_present("session_id")),
        87,
        false, // Dynamic
    );
    composer.register_section(
        "code_references",
        "system/main/main-code-references.md",
        None,
        90,
        true,
    );
    composer.register_section(
        "system_reminders_note",
        "system/main/main-reminders-note.md",
        None,
        95,
        false, // Dynamic
    );

    composer
}

/// Create a thinking-mode composer.
pub fn create_thinking_composer(templates_dir: impl AsRef<Path>) -> PromptComposer {
    let mut composer = PromptComposer::new(templates_dir.as_ref());

    // Core thinking identity - MUST be first (matches Python's core_prompt loading)
    composer.register_section("thinking_core", "system/thinking.md", None, 10, true);

    composer.register_section(
        "available_tools",
        "system/thinking/thinking-available-tools.md",
        None,
        45,
        true,
    );
    composer.register_section(
        "subagent_guide",
        "system/thinking/thinking-subagent-guide.md",
        None,
        50,
        true,
    );
    composer.register_section(
        "code_references",
        "system/thinking/thinking-code-references.md",
        None,
        85,
        true,
    );
    composer.register_section(
        "output_rules",
        "system/thinking/thinking-output-rules.md",
        None,
        90,
        true,
    );

    composer
}

/// Create the appropriate composer for a given mode.
pub fn create_composer(templates_dir: impl AsRef<Path>, mode: &str) -> PromptComposer {
    if mode == "system/thinking" {
        create_thinking_composer(templates_dir)
    } else {
        create_default_composer(templates_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_default_composer_section_count() {
        let dir = tempfile::TempDir::new().unwrap();
        let composer = create_default_composer(dir.path());
        // Should have many sections registered
        assert!(composer.section_count() > 15);
    }

    #[test]
    fn test_create_thinking_composer() {
        let dir = tempfile::TempDir::new().unwrap();
        let composer = create_thinking_composer(dir.path());
        assert_eq!(composer.section_count(), 5);
    }

    #[test]
    fn test_create_composer_dispatch() {
        let dir = tempfile::TempDir::new().unwrap();

        let main = create_composer(dir.path(), "system/main");
        assert!(main.section_count() > 15);

        let thinking = create_composer(dir.path(), "system/thinking");
        assert_eq!(thinking.section_count(), 5);
    }
}
