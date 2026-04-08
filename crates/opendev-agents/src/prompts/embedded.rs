//! Embedded prompt templates (compile-time via `include_str!`).
//!
//! All `.md` template files are embedded into the binary at compile time.
//! This eliminates runtime filesystem dependencies for prompt loading.

use std::collections::HashMap;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Embedded template constants
// ---------------------------------------------------------------------------

// generators/
pub const GENERATORS_GENERATOR_AGENT: &str =
    include_str!("../../templates/generators/generator-agent.md");
pub const GENERATORS_GENERATOR_SKILL: &str =
    include_str!("../../templates/generators/generator-skill.md");

// top-level
pub const REMINDERS: &str = include_str!("../../templates/reminders.md");

// subagents/
pub const SUBAGENTS_SUBAGENT_BUILD: &str =
    include_str!("../../templates/subagents/subagent-build.md");
pub const SUBAGENTS_SUBAGENT_CODE_EXPLORER: &str =
    include_str!("../../templates/subagents/subagent-code-explorer.md");
pub const SUBAGENTS_SUBAGENT_GENERAL: &str =
    include_str!("../../templates/subagents/subagent-general.md");
pub const SUBAGENTS_SUBAGENT_PLANNER: &str =
    include_str!("../../templates/subagents/subagent-planner.md");
pub const SUBAGENTS_SUBAGENT_PROJECT_INIT: &str =
    include_str!("../../templates/subagents/subagent-project-init.md");
pub const SUBAGENTS_SUBAGENT_VERIFICATION: &str =
    include_str!("../../templates/subagents/subagent-verification.md");

// system/
pub const SYSTEM_COMPACTION: &str = include_str!("../../templates/system/compaction.md");
pub const SYSTEM_INIT: &str = include_str!("../../templates/system/init.md");
pub const SYSTEM_MAIN: &str = include_str!("../../templates/system/main.md");

// system/main/
pub const SYSTEM_MAIN_MAIN_ACTION_SAFETY: &str =
    include_str!("../../templates/system/main/main-action-safety.md");
pub const SYSTEM_MAIN_MAIN_AUTO_MEMORY: &str =
    include_str!("../../templates/system/main/main-auto-memory.md");
pub const SYSTEM_MAIN_MAIN_CODE_QUALITY: &str =
    include_str!("../../templates/system/main/main-code-quality.md");
pub const SYSTEM_MAIN_MAIN_CODE_REFERENCES: &str =
    include_str!("../../templates/system/main/main-code-references.md");
pub const SYSTEM_MAIN_MAIN_ERROR_RECOVERY: &str =
    include_str!("../../templates/system/main/main-error-recovery.md");
pub const SYSTEM_MAIN_MAIN_GIT_WORKFLOW: &str =
    include_str!("../../templates/system/main/main-git-workflow.md");
pub const SYSTEM_MAIN_MAIN_MODE_AWARENESS: &str =
    include_str!("../../templates/system/main/main-mode-awareness.md");
pub const SYSTEM_MAIN_MAIN_NO_TIME_ESTIMATES: &str =
    include_str!("../../templates/system/main/main-no-time-estimates.md");
pub const SYSTEM_MAIN_MAIN_OUTPUT_AWARENESS: &str =
    include_str!("../../templates/system/main/main-output-awareness.md");
pub const SYSTEM_MAIN_MAIN_OUTPUT_EFFICIENCY: &str =
    include_str!("../../templates/system/main/main-output-efficiency.md");
pub const SYSTEM_MAIN_MAIN_PROVIDER_ANTHROPIC: &str =
    include_str!("../../templates/system/main/main-provider-anthropic.md");
pub const SYSTEM_MAIN_MAIN_PROVIDER_FIREWORKS: &str =
    include_str!("../../templates/system/main/main-provider-fireworks.md");
pub const SYSTEM_MAIN_MAIN_PROVIDER_OPENAI: &str =
    include_str!("../../templates/system/main/main-provider-openai.md");
pub const SYSTEM_MAIN_MAIN_REMINDERS_NOTE: &str =
    include_str!("../../templates/system/main/main-reminders-note.md");
pub const SYSTEM_MAIN_MAIN_SCRATCHPAD: &str =
    include_str!("../../templates/system/main/main-scratchpad.md");
pub const SYSTEM_MAIN_MAIN_SECURITY_POLICY: &str =
    include_str!("../../templates/system/main/main-security-policy.md");
pub const SYSTEM_MAIN_MAIN_SUBAGENT_GUIDE: &str =
    include_str!("../../templates/system/main/main-subagent-guide.md");
pub const SYSTEM_MAIN_MAIN_AGENT_TEAM_GUIDE: &str =
    include_str!("../../templates/system/main/main-agent-team-guide.md");
pub const SYSTEM_MAIN_MAIN_TASK_TRACKING: &str =
    include_str!("../../templates/system/main/main-task-tracking.md");
pub const SYSTEM_MAIN_MAIN_TONE_AND_STYLE: &str =
    include_str!("../../templates/system/main/main-tone-and-style.md");
pub const SYSTEM_MAIN_MAIN_TOOL_SELECTION: &str =
    include_str!("../../templates/system/main/main-tool-selection.md");
pub const SYSTEM_MAIN_MAIN_VERIFICATION: &str =
    include_str!("../../templates/system/main/main-verification.md");

// tools/
pub const TOOLS_TOOL_ANALYZE_IMAGE: &str =
    include_str!("../../templates/tools/tool-analyze-image.md");
pub const TOOLS_TOOL_APPLY_PATCH: &str = include_str!("../../templates/tools/tool-apply-patch.md");
pub const TOOLS_TOOL_ASK_USER: &str = include_str!("../../templates/tools/tool-ask-user.md");
pub const TOOLS_TOOL_BROWSER: &str = include_str!("../../templates/tools/tool-browser.md");
pub const TOOLS_TOOL_CAPTURE_SCREENSHOT: &str =
    include_str!("../../templates/tools/tool-capture-screenshot.md");
pub const TOOLS_TOOL_CAPTURE_WEB_SCREENSHOT: &str =
    include_str!("../../templates/tools/tool-capture-web-screenshot.md");
pub const TOOLS_TOOL_CLEAR_TODOS: &str = include_str!("../../templates/tools/tool-clear-todos.md");
pub const TOOLS_TOOL_COMPLETE_TODO: &str =
    include_str!("../../templates/tools/tool-complete-todo.md");
pub const TOOLS_TOOL_EDIT_FILE: &str = include_str!("../../templates/tools/tool-edit-file.md");
pub const TOOLS_TOOL_FETCH_URL: &str = include_str!("../../templates/tools/tool-fetch-url.md");
pub const TOOLS_TOOL_FIND_REFERENCING_SYMBOLS: &str =
    include_str!("../../templates/tools/tool-find-referencing-symbols.md");
pub const TOOLS_TOOL_FIND_SYMBOL: &str = include_str!("../../templates/tools/tool-find-symbol.md");
pub const TOOLS_TOOL_GET_SESSION_HISTORY: &str =
    include_str!("../../templates/tools/tool-get-session-history.md");
pub const TOOLS_TOOL_GET_SUBAGENT_OUTPUT: &str =
    include_str!("../../templates/tools/tool-get-subagent-output.md");
pub const TOOLS_TOOL_GIT: &str = include_str!("../../templates/tools/tool-git.md");
pub const TOOLS_TOOL_INSERT_AFTER_SYMBOL: &str =
    include_str!("../../templates/tools/tool-insert-after-symbol.md");
pub const TOOLS_TOOL_INSERT_BEFORE_SYMBOL: &str =
    include_str!("../../templates/tools/tool-insert-before-symbol.md");
pub const TOOLS_TOOL_INVOKE_SKILL: &str =
    include_str!("../../templates/tools/tool-invoke-skill.md");
pub const TOOLS_TOOL_LIST_AGENTS: &str = include_str!("../../templates/tools/tool-list-agents.md");
pub const TOOLS_TOOL_LIST_FILES: &str = include_str!("../../templates/tools/tool-list-files.md");
pub const TOOLS_TOOL_MULTI_EDIT: &str = include_str!("../../templates/tools/tool-multi-edit.md");
pub const TOOLS_TOOL_LIST_SESSIONS: &str =
    include_str!("../../templates/tools/tool-list-sessions.md");
pub const TOOLS_TOOL_LIST_SUBAGENTS: &str =
    include_str!("../../templates/tools/tool-list-subagents.md");
pub const TOOLS_TOOL_LIST_TODOS: &str = include_str!("../../templates/tools/tool-list-todos.md");
pub const TOOLS_TOOL_MEMORY_SEARCH: &str =
    include_str!("../../templates/tools/tool-memory-search.md");
pub const TOOLS_TOOL_MEMORY_WRITE: &str =
    include_str!("../../templates/tools/tool-memory-write.md");
pub const TOOLS_TOOL_NOTEBOOK_EDIT: &str =
    include_str!("../../templates/tools/tool-notebook-edit.md");
pub const TOOLS_TOOL_OPEN_BROWSER: &str =
    include_str!("../../templates/tools/tool-open-browser.md");
pub const TOOLS_TOOL_PRESENT_PLAN: &str =
    include_str!("../../templates/tools/tool-present-plan.md");
pub const TOOLS_TOOL_READ_FILE: &str = include_str!("../../templates/tools/tool-read-file.md");
pub const TOOLS_TOOL_RENAME_SYMBOL: &str =
    include_str!("../../templates/tools/tool-rename-symbol.md");
pub const TOOLS_TOOL_REPLACE_SYMBOL_BODY: &str =
    include_str!("../../templates/tools/tool-replace-symbol-body.md");
pub const TOOLS_TOOL_RUN_COMMAND: &str = include_str!("../../templates/tools/tool-run-command.md");
pub const TOOLS_TOOL_SCHEDULE: &str = include_str!("../../templates/tools/tool-schedule.md");
pub const TOOLS_TOOL_SEARCH_TOOLS: &str =
    include_str!("../../templates/tools/tool-search-tools.md");
pub const TOOLS_TOOL_SEARCH: &str = include_str!("../../templates/tools/tool-search.md");
pub const TOOLS_TOOL_GREP: &str = include_str!("../../templates/tools/tool-grep.md");
pub const TOOLS_TOOL_AST_GREP: &str = include_str!("../../templates/tools/tool-ast-grep.md");
pub const TOOLS_TOOL_SEND_MESSAGE: &str =
    include_str!("../../templates/tools/tool-send-message.md");
pub const TOOLS_TOOL_SPAWN_AGENT: &str = include_str!("../../templates/tools/tool-spawn-agent.md");
pub const TOOLS_TOOL_TASK_COMPLETE: &str =
    include_str!("../../templates/tools/tool-task-complete.md");
pub const TOOLS_TOOL_UPDATE_TODO: &str = include_str!("../../templates/tools/tool-update-todo.md");
pub const TOOLS_TOOL_WEB_SEARCH: &str = include_str!("../../templates/tools/tool-web-search.md");
pub const TOOLS_TOOL_WRITE_FILE: &str = include_str!("../../templates/tools/tool-write-file.md");
pub const TOOLS_TOOL_WRITE_TODOS: &str = include_str!("../../templates/tools/tool-write-todos.md");

// ---------------------------------------------------------------------------
// Template registry
// ---------------------------------------------------------------------------

/// Total number of embedded templates.
pub const TEMPLATE_COUNT: usize = 78;

/// All embedded templates indexed by their relative path.
///
/// Keys match the `file_path` strings used in [`super::PromptSection`] registrations,
/// e.g. `"system/main/main-security-policy.md"`.
pub static TEMPLATES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::with_capacity(TEMPLATE_COUNT);

    // generators
    m.insert("generators/generator-agent.md", GENERATORS_GENERATOR_AGENT);
    m.insert("generators/generator-skill.md", GENERATORS_GENERATOR_SKILL);

    // top-level
    m.insert("reminders.md", REMINDERS);

    // subagents
    m.insert("subagents/subagent-build.md", SUBAGENTS_SUBAGENT_BUILD);
    m.insert(
        "subagents/subagent-code-explorer.md",
        SUBAGENTS_SUBAGENT_CODE_EXPLORER,
    );
    m.insert("subagents/subagent-general.md", SUBAGENTS_SUBAGENT_GENERAL);
    m.insert("subagents/subagent-planner.md", SUBAGENTS_SUBAGENT_PLANNER);
    m.insert(
        "subagents/subagent-project-init.md",
        SUBAGENTS_SUBAGENT_PROJECT_INIT,
    );
    m.insert(
        "subagents/subagent-verification.md",
        SUBAGENTS_SUBAGENT_VERIFICATION,
    );

    // system
    m.insert("system/compaction.md", SYSTEM_COMPACTION);
    m.insert("system/init.md", SYSTEM_INIT);
    m.insert("system/main.md", SYSTEM_MAIN);

    // system/main
    m.insert(
        "system/main/main-action-safety.md",
        SYSTEM_MAIN_MAIN_ACTION_SAFETY,
    );
    m.insert(
        "system/main/main-auto-memory.md",
        SYSTEM_MAIN_MAIN_AUTO_MEMORY,
    );
    m.insert(
        "system/main/main-code-quality.md",
        SYSTEM_MAIN_MAIN_CODE_QUALITY,
    );
    m.insert(
        "system/main/main-code-references.md",
        SYSTEM_MAIN_MAIN_CODE_REFERENCES,
    );
    m.insert(
        "system/main/main-error-recovery.md",
        SYSTEM_MAIN_MAIN_ERROR_RECOVERY,
    );
    m.insert(
        "system/main/main-git-workflow.md",
        SYSTEM_MAIN_MAIN_GIT_WORKFLOW,
    );
    m.insert(
        "system/main/main-mode-awareness.md",
        SYSTEM_MAIN_MAIN_MODE_AWARENESS,
    );
    m.insert(
        "system/main/main-no-time-estimates.md",
        SYSTEM_MAIN_MAIN_NO_TIME_ESTIMATES,
    );
    m.insert(
        "system/main/main-output-awareness.md",
        SYSTEM_MAIN_MAIN_OUTPUT_AWARENESS,
    );
    m.insert(
        "system/main/main-output-efficiency.md",
        SYSTEM_MAIN_MAIN_OUTPUT_EFFICIENCY,
    );
    m.insert(
        "system/main/main-provider-anthropic.md",
        SYSTEM_MAIN_MAIN_PROVIDER_ANTHROPIC,
    );
    m.insert(
        "system/main/main-provider-fireworks.md",
        SYSTEM_MAIN_MAIN_PROVIDER_FIREWORKS,
    );
    m.insert(
        "system/main/main-provider-openai.md",
        SYSTEM_MAIN_MAIN_PROVIDER_OPENAI,
    );
    m.insert(
        "system/main/main-reminders-note.md",
        SYSTEM_MAIN_MAIN_REMINDERS_NOTE,
    );
    m.insert(
        "system/main/main-scratchpad.md",
        SYSTEM_MAIN_MAIN_SCRATCHPAD,
    );
    m.insert(
        "system/main/main-security-policy.md",
        SYSTEM_MAIN_MAIN_SECURITY_POLICY,
    );
    m.insert(
        "system/main/main-subagent-guide.md",
        SYSTEM_MAIN_MAIN_SUBAGENT_GUIDE,
    );
    m.insert(
        "system/main/main-agent-team-guide.md",
        SYSTEM_MAIN_MAIN_AGENT_TEAM_GUIDE,
    );
    m.insert(
        "system/main/main-task-tracking.md",
        SYSTEM_MAIN_MAIN_TASK_TRACKING,
    );
    m.insert(
        "system/main/main-tone-and-style.md",
        SYSTEM_MAIN_MAIN_TONE_AND_STYLE,
    );
    m.insert(
        "system/main/main-tool-selection.md",
        SYSTEM_MAIN_MAIN_TOOL_SELECTION,
    );
    m.insert(
        "system/main/main-verification.md",
        SYSTEM_MAIN_MAIN_VERIFICATION,
    );

    // tools
    m.insert("tools/tool-analyze-image.md", TOOLS_TOOL_ANALYZE_IMAGE);
    m.insert("tools/tool-apply-patch.md", TOOLS_TOOL_APPLY_PATCH);
    m.insert("tools/tool-ask-user.md", TOOLS_TOOL_ASK_USER);
    m.insert("tools/tool-browser.md", TOOLS_TOOL_BROWSER);
    m.insert(
        "tools/tool-capture-screenshot.md",
        TOOLS_TOOL_CAPTURE_SCREENSHOT,
    );
    m.insert(
        "tools/tool-capture-web-screenshot.md",
        TOOLS_TOOL_CAPTURE_WEB_SCREENSHOT,
    );
    m.insert("tools/tool-clear-todos.md", TOOLS_TOOL_CLEAR_TODOS);
    m.insert("tools/tool-complete-todo.md", TOOLS_TOOL_COMPLETE_TODO);
    m.insert("tools/tool-edit-file.md", TOOLS_TOOL_EDIT_FILE);
    m.insert("tools/tool-fetch-url.md", TOOLS_TOOL_FETCH_URL);
    m.insert(
        "tools/tool-find-referencing-symbols.md",
        TOOLS_TOOL_FIND_REFERENCING_SYMBOLS,
    );
    m.insert("tools/tool-find-symbol.md", TOOLS_TOOL_FIND_SYMBOL);
    m.insert(
        "tools/tool-get-session-history.md",
        TOOLS_TOOL_GET_SESSION_HISTORY,
    );
    m.insert(
        "tools/tool-get-subagent-output.md",
        TOOLS_TOOL_GET_SUBAGENT_OUTPUT,
    );
    m.insert("tools/tool-git.md", TOOLS_TOOL_GIT);
    m.insert(
        "tools/tool-insert-after-symbol.md",
        TOOLS_TOOL_INSERT_AFTER_SYMBOL,
    );
    m.insert(
        "tools/tool-insert-before-symbol.md",
        TOOLS_TOOL_INSERT_BEFORE_SYMBOL,
    );
    m.insert("tools/tool-invoke-skill.md", TOOLS_TOOL_INVOKE_SKILL);
    m.insert("tools/tool-list-agents.md", TOOLS_TOOL_LIST_AGENTS);
    m.insert("tools/tool-list-files.md", TOOLS_TOOL_LIST_FILES);
    m.insert("tools/tool-multi-edit.md", TOOLS_TOOL_MULTI_EDIT);
    m.insert("tools/tool-list-sessions.md", TOOLS_TOOL_LIST_SESSIONS);
    m.insert("tools/tool-list-subagents.md", TOOLS_TOOL_LIST_SUBAGENTS);
    m.insert("tools/tool-list-todos.md", TOOLS_TOOL_LIST_TODOS);
    m.insert("tools/tool-memory-search.md", TOOLS_TOOL_MEMORY_SEARCH);
    m.insert("tools/tool-memory-write.md", TOOLS_TOOL_MEMORY_WRITE);
    m.insert("tools/tool-notebook-edit.md", TOOLS_TOOL_NOTEBOOK_EDIT);
    m.insert("tools/tool-open-browser.md", TOOLS_TOOL_OPEN_BROWSER);
    m.insert("tools/tool-present-plan.md", TOOLS_TOOL_PRESENT_PLAN);
    m.insert("tools/tool-read-file.md", TOOLS_TOOL_READ_FILE);
    m.insert("tools/tool-rename-symbol.md", TOOLS_TOOL_RENAME_SYMBOL);
    m.insert(
        "tools/tool-replace-symbol-body.md",
        TOOLS_TOOL_REPLACE_SYMBOL_BODY,
    );
    m.insert("tools/tool-run-command.md", TOOLS_TOOL_RUN_COMMAND);
    m.insert("tools/tool-schedule.md", TOOLS_TOOL_SCHEDULE);
    m.insert("tools/tool-search-tools.md", TOOLS_TOOL_SEARCH_TOOLS);
    m.insert("tools/tool-search.md", TOOLS_TOOL_SEARCH);
    m.insert("tools/tool-grep.md", TOOLS_TOOL_GREP);
    m.insert("tools/tool-ast-grep.md", TOOLS_TOOL_AST_GREP);
    m.insert("tools/tool-send-message.md", TOOLS_TOOL_SEND_MESSAGE);
    m.insert("tools/tool-spawn-agent.md", TOOLS_TOOL_SPAWN_AGENT);
    m.insert("tools/tool-task-complete.md", TOOLS_TOOL_TASK_COMPLETE);
    m.insert("tools/tool-update-todo.md", TOOLS_TOOL_UPDATE_TODO);
    m.insert("tools/tool-web-search.md", TOOLS_TOOL_WEB_SEARCH);
    m.insert("tools/tool-write-file.md", TOOLS_TOOL_WRITE_FILE);
    m.insert("tools/tool-write-todos.md", TOOLS_TOOL_WRITE_TODOS);

    m
});

/// Build the `/init` prompt, substituting optional user arguments.
pub fn build_init_prompt(args: &str) -> String {
    SYSTEM_INIT.replace("{args}", args)
}

/// Look up an embedded template by its relative path.
///
/// Returns `None` if the path is not a known embedded template.
pub fn get_embedded(path: &str) -> Option<&'static str> {
    TEMPLATES.get(path).copied()
}

/// Get all embedded templates in the `system/main/` category.
pub fn system_main_templates() -> Vec<(&'static str, &'static str)> {
    TEMPLATES
        .iter()
        .filter(|(k, _)| k.starts_with("system/main/"))
        .map(|(&k, &v)| (k, v))
        .collect()
}

/// Get all embedded templates in the `tools/` category.
pub fn tool_templates() -> Vec<(&'static str, &'static str)> {
    TEMPLATES
        .iter()
        .filter(|(k, _)| k.starts_with("tools/"))
        .map(|(&k, &v)| (k, v))
        .collect()
}

/// Get all embedded templates in the `subagents/` category.
pub fn subagent_templates() -> Vec<(&'static str, &'static str)> {
    TEMPLATES
        .iter()
        .filter(|(k, _)| k.starts_with("subagents/"))
        .map(|(&k, &v)| (k, v))
        .collect()
}

/// Get all embedded templates in the `generators/` category.
pub fn generator_templates() -> Vec<(&'static str, &'static str)> {
    TEMPLATES
        .iter()
        .filter(|(k, _)| k.starts_with("generators/"))
        .map(|(&k, &v)| (k, v))
        .collect()
}

#[cfg(test)]
#[path = "embedded_tests.rs"]
mod tests;
