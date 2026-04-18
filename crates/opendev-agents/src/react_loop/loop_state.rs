//! Cross-iteration mutable state for the ReAct loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::attachments::CollectorRunner;
use crate::attachments::collectors::{
    CompactionCollector, DateChangeCollector, GitStatusCollector, PlanModeCollector,
    SemanticMemoryCollector, SessionMemoryCollector, TodoStateCollector,
};
use crate::doom_loop::DoomLoopDetector;
use crate::prompts::reminders::{
    MessageClass, ProactiveReminderConfig, ProactiveReminderScheduler,
};

/// Mutable state that persists across iterations of the ReAct loop.
///
/// Bundled into a struct to keep the orchestrator loop clean and make
/// dependencies explicit when passing to phase functions.
pub(super) struct LoopState {
    pub iteration: usize,
    pub consecutive_no_tool_calls: usize,
    pub consecutive_truncations: usize,
    pub doom_detector: DoomLoopDetector,

    /// Per-subdirectory instruction injection tracker.
    pub subdir_tracker: opendev_context::SubdirInstructionTracker,
    /// Startup instruction paths — kept for `reset_after_compaction()`.
    pub startup_paths: Vec<PathBuf>,

    /// Skill-driven model override from frontmatter.
    pub skill_model_override: Option<String>,

    /// Session-level auto-approved command prefixes / MCP tool names.
    pub auto_approved_patterns: HashSet<String>,

    /// When true, Write/Edit tool calls require user approval during plan
    /// implementation ("review edits" mode selected at plan approval).
    pub plan_edit_review_mode: bool,

    // Nudge/reminder state
    pub todo_nudge_count: usize,
    pub all_todos_complete_nudged: bool,
    pub completion_nudge_sent: bool,
    pub consecutive_reads: usize,

    /// Number of background tasks spawned (SpawnTeammate or background subagent).
    pub bg_tasks_spawned: usize,
    /// How many times we've nudged the agent about pending background tasks.
    pub bg_wait_nudge_count: usize,

    /// Tool names activated via ToolSearch (deferred tools become callable).
    /// Core tools are always active; this tracks additionally activated ones.
    pub activated_tools: HashSet<String>,
    pub proactive_reminders: ProactiveReminderScheduler,

    /// Per-turn context attachment collectors.
    pub collector_runner: CollectorRunner,
    /// Shared flag set by safety phase after compaction.
    pub compaction_flag: Arc<AtomicBool>,

    /// Per-tool result budget policy. Caps each tool result at append-time
    /// so a single oversized output cannot push the conversation past
    /// compaction thresholds in one turn.
    pub tool_budget_policy: opendev_context::ToolBudgetPolicy,
    /// On-disk store for content overflowed by `tool_budget_policy`.
    pub overflow_store: opendev_context::OverflowStore,
}

impl LoopState {
    /// Create a new `LoopState` for a fresh react loop execution.
    pub fn new(working_dir: &std::path::Path) -> Self {
        let startup_paths: Vec<PathBuf> =
            opendev_context::discover_instruction_files(working_dir, &[], &[])
                .into_iter()
                .map(|f| f.path)
                .collect();
        let subdir_tracker = opendev_context::SubdirInstructionTracker::new(
            working_dir.to_path_buf(),
            &startup_paths,
        );

        // Build compaction flag shared between collector and LoopState
        let compaction_flag = Arc::new(AtomicBool::new(false));
        let collectors: Vec<Box<dyn crate::attachments::ContextCollector>> = vec![
            Box::new(TodoStateCollector::new(10)),
            Box::new(PlanModeCollector::new(5)),
            Box::new(DateChangeCollector::new()),
            Box::new(GitStatusCollector::new(5)),
            Box::new(CompactionCollector::new(Arc::clone(&compaction_flag))),
            Box::new(SemanticMemoryCollector::new(15)),
            Box::new(SessionMemoryCollector::new()),
        ];

        Self {
            iteration: 0,
            consecutive_no_tool_calls: 0,
            consecutive_truncations: 0,
            doom_detector: DoomLoopDetector::new(),
            subdir_tracker,
            startup_paths,
            skill_model_override: None,
            auto_approved_patterns: HashSet::new(),
            plan_edit_review_mode: false,
            todo_nudge_count: 0,
            all_todos_complete_nudged: false,
            completion_nudge_sent: false,
            consecutive_reads: 0,
            bg_tasks_spawned: 0,
            bg_wait_nudge_count: 0,
            activated_tools: HashSet::new(),
            collector_runner: CollectorRunner::new(collectors),
            compaction_flag,
            tool_budget_policy: opendev_context::ToolBudgetPolicy::default(),
            overflow_store: opendev_context::OverflowStore::new(working_dir),
            // Note: todo reminders are handled by TodoStateCollector (live data).
            // Only task_proactive_reminder remains here as a static template nudge.
            proactive_reminders: ProactiveReminderScheduler::new(vec![ProactiveReminderConfig {
                name: "task_proactive_reminder",
                turns_since_reset: 10,
                turns_between: 10,
                class: MessageClass::Nudge,
            }]),
        }
    }
}
