//! Unit tests for `tool_budget`.

use super::*;
use crate::compaction::{TOOL_RESULT_BUDGET_DEFAULT_CHARS, TOOL_RESULT_BUDGET_PREVIEW_CHARS};

fn store_in(dir: &std::path::Path) -> OverflowStore {
    OverflowStore::with_dir(dir, dir.join("overflow"))
}

#[test]
fn under_cap_passes_through_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::default();

    let raw = "small output".to_string();
    let result = apply_tool_result_budget("custom_tool", "tc-1", &raw, &policy, &store);

    assert!(!result.truncated);
    assert!(result.overflow_ref.is_none());
    assert_eq!(result.displayed_content, raw);
    assert_eq!(result.original_len, raw.chars().count());
}

#[test]
fn at_cap_passes_through_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(100);

    let raw = "x".repeat(100);
    let result = apply_tool_result_budget("custom_tool", "tc-2", &raw, &policy, &store);

    assert!(!result.truncated);
    assert_eq!(result.displayed_content.chars().count(), 100);
}

#[test]
fn over_cap_truncates_with_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::default();

    let raw = "x".repeat(TOOL_RESULT_BUDGET_DEFAULT_CHARS * 2);
    let result = apply_tool_result_budget("custom_tool", "tc-3", &raw, &policy, &store);

    assert!(result.truncated);
    assert!(result.overflow_ref.is_some(), "expected an overflow ref");
    assert!(result.displayed_content.contains("[truncated:"));
    assert!(result.displayed_content.contains("[full output:"));
    assert!(
        result.displayed_content.chars().count() < TOOL_RESULT_BUDGET_DEFAULT_CHARS,
        "displayed content must stay under the cap",
    );
}

#[test]
fn per_tool_override_applies() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::default();

    // `list_files` cap is 2_000; 5_000 chars must truncate.
    let raw = "y".repeat(5_000);
    let result = apply_tool_result_budget("list_files", "tc-4", &raw, &policy, &store);

    assert!(result.truncated);
    // `read_file` cap is 12_000; 5_000 chars must NOT truncate.
    let result_read = apply_tool_result_budget("read_file", "tc-5", &raw, &policy, &store);
    assert!(!result_read.truncated);
}

#[test]
fn unbounded_tool_bypasses_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::default();

    assert!(policy.is_unbounded("web_screenshot"));
    let raw = "z".repeat(100_000);
    let result = apply_tool_result_budget("web_screenshot", "tc-6", &raw, &policy, &store);

    assert!(!result.truncated);
    assert!(result.overflow_ref.is_none());
    assert_eq!(result.displayed_content.chars().count(), 100_000);
}

#[test]
fn multibyte_truncation_is_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    // Cap below the input size but well above one preview's worth.
    let policy = ToolBudgetPolicy::with_default_chars(50);

    // Each "🚀" is 1 char / 4 bytes — naive byte slicing would panic.
    let raw: String = "🚀".repeat(200);
    let result = apply_tool_result_budget("custom_tool", "tc-7", &raw, &policy, &store);

    assert!(result.truncated);
    // Must not panic; preview must be a valid UTF-8 string with rocket chars.
    assert!(result.displayed_content.starts_with('🚀'));
}

#[test]
fn overflow_file_round_trips_full_content() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(100);

    let raw = "abcdefghij".repeat(50); // 500 chars
    let result = apply_tool_result_budget("custom_tool", "tc-8", &raw, &policy, &store);

    let ref_path = result.overflow_ref.expect("overflow ref must exist");
    let abs_path = tmp.path().join(&ref_path);
    let on_disk = std::fs::read_to_string(&abs_path).unwrap();
    assert_eq!(on_disk, raw);
}

#[test]
fn override_can_opt_out_per_tool() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let mut policy = ToolBudgetPolicy::with_default_chars(50);
    policy.set_override("noisy_but_required", usize::MAX);

    let raw = "q".repeat(10_000);
    let result = apply_tool_result_budget("noisy_but_required", "tc-9", &raw, &policy, &store);

    assert!(!result.truncated);
}

#[test]
fn preview_size_is_bounded_by_cap() {
    // Sanity: when the cap is smaller than the configured preview length,
    // the preview shrinks rather than overflowing the cap.
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let small_cap = TOOL_RESULT_BUDGET_PREVIEW_CHARS / 2;
    let policy = ToolBudgetPolicy::with_default_chars(small_cap);

    let raw = "p".repeat(small_cap * 4);
    let result = apply_tool_result_budget("custom_tool", "tc-10", &raw, &policy, &store);

    assert!(result.truncated);
    // The preview is the leading section of displayed_content up to the
    // first "\n\n…" separator. Verify it fits within the cap and is not
    // empty.
    let preview = result
        .displayed_content
        .split("\n\n…")
        .next()
        .expect("displayed_content must contain truncation separator");
    let preview_chars = preview.chars().count();
    assert!(
        preview_chars <= small_cap,
        "preview {preview_chars} > cap {small_cap}"
    );
    assert!(preview_chars > 0);
}

// ─── A. Boundary arithmetic ───────────────────────────────────────────────

/// A1: An empty input must pass through cleanly with `truncated=false`,
/// `original_len=0`, and no overflow ref. Catches off-by-one bugs in
/// `char_len_within` at the zero boundary.
#[test]
fn empty_input_passes_through_with_zero_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::default();

    let result = apply_tool_result_budget("custom_tool", "tc-empty", "", &policy, &store);

    assert!(!result.truncated);
    assert!(result.overflow_ref.is_none());
    assert_eq!(result.original_len, 0);
    assert_eq!(result.displayed_content, "");
}

/// A2: `cap=0` is degenerate but must not panic. Empty input still
/// passes through; non-empty input always truncates with an empty
/// preview body but a valid marker.
#[test]
fn cap_zero_handles_both_empty_and_non_empty_input() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(0);

    let empty = apply_tool_result_budget("custom_tool", "tc-zero-1", "", &policy, &store);
    assert!(
        !empty.truncated,
        "empty input within cap=0 must not truncate"
    );

    let non_empty =
        apply_tool_result_budget("custom_tool", "tc-zero-2", "anything", &policy, &store);
    assert!(
        non_empty.truncated,
        "any non-empty input must truncate at cap=0"
    );
    assert!(non_empty.displayed_content.contains("[truncated:"));
}

/// A5: One char over the cap fires the truncation path. Boundary
/// regression guard against off-by-one in the equality check.
#[test]
fn one_char_over_cap_triggers_truncation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let cap = 100;
    let policy = ToolBudgetPolicy::with_default_chars(cap);

    let raw = "x".repeat(cap + 1);
    let result = apply_tool_result_budget("custom_tool", "tc-over1", &raw, &policy, &store);

    assert!(result.truncated, "input of cap+1 chars must truncate");
    assert_eq!(result.original_len, cap + 1);
}

// ─── B. Truncation correctness ────────────────────────────────────────────

/// B1: The `truncated` flag is the single source of truth for callers
/// that gate downstream behavior (telemetry, debug logs, audit trails).
/// Pin it explicitly across both branches.
#[test]
fn truncated_flag_matches_actual_truncation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(50);

    let under = apply_tool_result_budget("t", "id", "x", &policy, &store);
    let over = apply_tool_result_budget("t", "id", &"y".repeat(200), &policy, &store);

    assert!(!under.truncated);
    assert!(under.overflow_ref.is_none());

    assert!(over.truncated);
    assert!(over.overflow_ref.is_some());
}

/// B2: `original_len` is character count, not byte count. Easy to
/// regress to `.len()` (byte count) which would mislead consumers about
/// how much was omitted for multibyte content.
#[test]
fn original_len_is_char_count_not_byte_count() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(5);

    // 10 rocket emoji = 10 chars / 40 bytes.
    let raw: String = "🚀".repeat(10);
    let result = apply_tool_result_budget("t", "id", &raw, &policy, &store);

    assert_eq!(
        result.original_len, 10,
        "must report char count, not byte count"
    );
    // And the truncation marker echoes the same number.
    assert!(result.displayed_content.contains("/ 10 chars omitted"));
}

/// B3: For caps comfortably larger than the preview, the full
/// displayed content (preview + marker + reference) stays under the
/// cap. Marker overhead must remain a small constant.
#[test]
fn displayed_content_stays_within_cap_for_normal_caps() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    // Default cap (8000) is well above PREVIEW_CHARS (1500), so
    // overhead-after-preview must fit comfortably.
    let policy = ToolBudgetPolicy::default();

    let raw = "x".repeat(50_000);
    let result = apply_tool_result_budget("custom_tool", "tc-b3", &raw, &policy, &store);

    let displayed_chars = result.displayed_content.chars().count();
    assert!(result.truncated);
    assert!(
        displayed_chars <= TOOL_RESULT_BUDGET_DEFAULT_CHARS,
        "displayed content {displayed_chars} exceeded cap {TOOL_RESULT_BUDGET_DEFAULT_CHARS}",
    );
    // And the preview portion alone is exactly PREVIEW_CHARS.
    let preview = result.displayed_content.split("\n\n…").next().unwrap();
    assert_eq!(preview.chars().count(), TOOL_RESULT_BUDGET_PREVIEW_CHARS);
}

// ─── D. Per-tool policy ───────────────────────────────────────────────────

/// D2: `read_file` (snake_case) and `Read` (PascalCase) are aliases for
/// the same capability. Their caps must stay in sync — a divergence
/// would silently differ in budgeting depending on which alias the
/// registry uses.
#[test]
fn read_file_and_read_aliases_share_a_cap() {
    let policy = ToolBudgetPolicy::default();
    assert_eq!(
        policy.cap_for("read_file"),
        policy.cap_for("Read"),
        "read_file and Read must share a budget",
    );
}

/// D3: Same parity invariant for the bash family. Three names, one
/// underlying tool.
#[test]
fn bash_family_aliases_share_a_cap() {
    let policy = ToolBudgetPolicy::default();
    let bash = policy.cap_for("Bash");
    assert_eq!(policy.cap_for("run_command"), bash);
    assert_eq!(policy.cap_for("bash_execute"), bash);
}

/// D4: Re-overriding an existing tool updates in place (last-write
/// wins). Catches a regression where `set_override` would push
/// duplicates instead of replacing — duplicates would shadow each
/// other arbitrarily depending on iteration order.
#[test]
fn set_override_updates_existing_entry_last_write_wins() {
    let mut policy = ToolBudgetPolicy::default();

    // The default cap for read_file is 12_000. Re-override twice.
    policy.set_override("read_file", 100);
    assert_eq!(policy.cap_for("read_file"), 100);

    policy.set_override("read_file", 200);
    assert_eq!(
        policy.cap_for("read_file"),
        200,
        "last set_override must win"
    );

    policy.set_override("read_file", usize::MAX);
    assert!(policy.is_unbounded("read_file"));
}

// ─── E. Overflow store I/O ────────────────────────────────────────────────

/// E1: When the overflow directory does not exist yet, the store
/// creates it on first write rather than failing. Standard
/// `create_dir_all` semantics, pinned so a refactor cannot silently
/// regress to "must pre-create".
#[test]
fn overflow_dir_is_auto_created_on_first_write() {
    let tmp = tempfile::tempdir().unwrap();
    let nested_dir = tmp
        .path()
        .join("does")
        .join("not")
        .join("exist")
        .join("yet");
    assert!(!nested_dir.exists());

    let store = OverflowStore::with_dir(tmp.path(), &nested_dir);
    let policy = ToolBudgetPolicy::with_default_chars(10);

    let result = apply_tool_result_budget("t", "id", &"x".repeat(100), &policy, &store);

    assert!(
        result.overflow_ref.is_some(),
        "write should succeed and return ref"
    );
    assert!(nested_dir.exists(), "overflow dir should have been created");
    assert!(nested_dir.is_dir());
}

/// E5: A tool name containing path-unsafe characters must be sanitized
/// in the resulting filename. No `/`, `\`, or whitespace should leak
/// into the filename component — those would either escape the
/// overflow directory (security) or corrupt the path (correctness).
#[test]
fn tool_name_with_unsafe_chars_is_sanitized() {
    let tmp = tempfile::tempdir().unwrap();
    let store = store_in(tmp.path());
    let policy = ToolBudgetPolicy::with_default_chars(10);

    let weird_name = "evil/../tool name 🚀";
    let result = apply_tool_result_budget(weird_name, "id-1", &"x".repeat(50), &policy, &store);

    let rel = result.overflow_ref.expect("must overflow");
    let filename = std::path::Path::new(&rel)
        .file_name()
        .expect("ref must have a filename")
        .to_string_lossy()
        .into_owned();

    assert!(!filename.contains('/'), "filename must not contain /");
    assert!(
        !filename.contains('\\'),
        "filename must not contain backslash"
    );
    assert!(
        !filename.contains(' '),
        "filename must not contain whitespace"
    );
    // And the file actually lives under the overflow dir.
    let abs = tmp.path().join(&rel);
    assert!(abs.exists(), "file must land at the displayed path");
}

/// E6: The same protection applies to `tool_call_id`, which is
/// LLM-influenced and could contain path-traversal sequences. The
/// resulting file must land inside the overflow directory regardless
/// of what the id contains.
#[test]
fn tool_call_id_with_path_traversal_stays_in_overflow_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let overflow_dir = tmp.path().join("overflow");
    let store = OverflowStore::with_dir(tmp.path(), &overflow_dir);
    let policy = ToolBudgetPolicy::with_default_chars(10);

    let evil_id = "../../etc/passwd";
    let result = apply_tool_result_budget("t", evil_id, &"x".repeat(50), &policy, &store);

    let rel = result.overflow_ref.expect("must overflow");
    let abs = tmp.path().join(&rel).canonicalize().unwrap();
    let overflow_canonical = overflow_dir.canonicalize().unwrap();

    assert!(
        abs.starts_with(&overflow_canonical),
        "overflow file {} escaped overflow dir {}",
        abs.display(),
        overflow_canonical.display(),
    );
}

/// E9: When the input fits within the budget, no file may be written.
/// Avoids polluting the overflow directory with one file per
/// well-behaved tool call — that would defeat the entire purpose
/// (and produce thousands of useless artifacts per session).
#[test]
fn no_file_written_when_under_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let overflow_dir = tmp.path().join("overflow");
    let store = OverflowStore::with_dir(tmp.path(), &overflow_dir);
    let policy = ToolBudgetPolicy::with_default_chars(1_000);

    let result = apply_tool_result_budget("t", "id", "small output", &policy, &store);

    assert!(!result.truncated);
    assert!(result.overflow_ref.is_none());
    // The overflow dir must not have been created (or must be empty).
    if overflow_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&overflow_dir).unwrap().collect();
        assert!(entries.is_empty(), "no files should be written under cap");
    }
}
