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
