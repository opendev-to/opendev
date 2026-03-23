//! Integration tests for tool implementations.
//!
//! These tests exercise the real tool implementations against the filesystem
//! and process subsystem. Each test uses temp directories for isolation.

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext};
use opendev_tools_impl::{BashTool, FileEditTool, FileReadTool, FileWriteTool, GitTool, GrepTool};
use tempfile::TempDir;

fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

// ========================================================================
// Bash tool integration tests
// ========================================================================

/// Verify that BashTool actually executes a command and captures stdout.
#[tokio::test]
async fn bash_executes_command_and_captures_stdout() {
    let tmp = TempDir::new().unwrap();
    let tool = BashTool::new();
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("command", serde_json::json!("echo integration_test_marker"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "bash should succeed: {:?}", result.error);
    assert!(
        result
            .output
            .as_ref()
            .unwrap()
            .contains("integration_test_marker"),
        "stdout should contain marker"
    );
}

/// Verify that BashTool captures stderr alongside stdout.
#[tokio::test]
async fn bash_captures_stderr() {
    let tmp = TempDir::new().unwrap();
    let tool = BashTool::new();
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("command", serde_json::json!("echo err_marker >&2"))]);

    let result = tool.execute(args, &ctx).await;
    // The command succeeds (exit 0) even though it writes to stderr
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("err_marker"), "stderr should be captured");
}

/// Verify that BashTool respects the working directory.
#[tokio::test]
async fn bash_runs_in_correct_working_directory() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("sentinel.txt"), "found_it").unwrap();

    let tool = BashTool::new();
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("command", serde_json::json!("cat sentinel.txt"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("found_it"));
}

/// Verify that BashTool times out long-running commands.
#[tokio::test]
async fn bash_timeout_kills_long_running_command() {
    let tool = BashTool::new();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("command", serde_json::json!("sleep 30")),
        ("timeout", serde_json::json!(1)),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success, "timed-out command should fail");
    assert!(
        result.error.as_ref().unwrap().contains("timed out"),
        "error should mention timeout"
    );
}

/// Verify that BashTool reports non-zero exit codes.
#[tokio::test]
async fn bash_nonzero_exit_code_is_failure() {
    let tool = BashTool::new();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("command", serde_json::json!("exit 7"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert_eq!(
        result.metadata.get("exit_code"),
        Some(&serde_json::json!(7))
    );
}

/// Verify that BashTool blocks dangerous patterns.
#[tokio::test]
async fn bash_blocks_dangerous_rm_rf() {
    let tool = BashTool::new();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("command", serde_json::json!("rm -rf /"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Blocked dangerous"));
}

/// Verify that BashTool pipes work correctly.
#[tokio::test]
async fn bash_pipes_work() {
    let tool = BashTool::new();
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[("command", serde_json::json!("echo 'a\nb\nc' | wc -l"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap().trim().to_string();
    assert!(
        output.contains("3"),
        "pipe should count 3 lines, got: {output}"
    );
}

// ========================================================================
// File write -> read -> edit lifecycle
// ========================================================================

/// Full file lifecycle: write a file, read it back, edit it, read again.
#[tokio::test]
async fn file_write_read_edit_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("lifecycle.txt");
    let file_str = file_path.to_str().unwrap();

    // Step 1: Write
    let write_tool = FileWriteTool;
    let ctx = ToolContext::new(tmp.path());
    let write_args = make_args(&[
        ("file_path", serde_json::json!(file_str)),
        (
            "content",
            serde_json::json!("line one\nline two\nline three\n"),
        ),
    ]);
    let result = write_tool.execute(write_args, &ctx).await;
    assert!(result.success, "write should succeed: {:?}", result.error);
    assert_eq!(result.metadata["lines"], serde_json::json!(3));

    // Step 2: Read
    let read_tool = FileReadTool;
    let read_args = make_args(&[("file_path", serde_json::json!(file_str))]);
    let result = read_tool.execute(read_args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("line one"));
    assert!(output.contains("line two"));
    assert!(output.contains("line three"));
    assert_eq!(result.metadata["total_lines"], serde_json::json!(3));

    // Step 3: Edit (replace "line two" with "line TWO")
    let edit_tool = FileEditTool;
    let edit_args = make_args(&[
        ("file_path", serde_json::json!(file_str)),
        ("old_string", serde_json::json!("line two")),
        ("new_string", serde_json::json!("line TWO")),
    ]);
    let result = edit_tool.execute(edit_args, &ctx).await;
    assert!(result.success, "edit should succeed: {:?}", result.error);
    assert_eq!(result.metadata["replacements"], serde_json::json!(1));

    // Step 4: Read again to verify edit
    let read_args = make_args(&[("file_path", serde_json::json!(file_str))]);
    let result = read_tool.execute(read_args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("line TWO"), "edit should be visible");
    assert!(!output.contains("line two"), "old text should be gone");
}

/// Verify that FileWriteTool creates nested parent directories.
#[tokio::test]
async fn file_write_creates_nested_dirs() {
    let tmp = TempDir::new().unwrap();
    let nested = tmp.path().join("a").join("b").join("c").join("deep.txt");
    let ctx = ToolContext::new(tmp.path());

    let tool = FileWriteTool;
    let args = make_args(&[
        ("file_path", serde_json::json!(nested.to_str().unwrap())),
        ("content", serde_json::json!("deep content")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(std::fs::read_to_string(&nested).unwrap(), "deep content");
}

/// Verify that FileEditTool handles trailing newline changes correctly.
#[tokio::test]
async fn file_edit_trailing_newline_handling() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("trailing.txt");
    std::fs::write(&file_path, "hello world\n").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("hello world\n")),
        ("new_string", serde_json::json!("hello world")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "trailing newline edit should succeed");

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "hello world");
}

/// Verify that FileEditTool replace_all works on multiple occurrences.
#[tokio::test]
async fn file_edit_replace_all_multiple_occurrences() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("multi.txt");
    std::fs::write(&file_path, "cat dog cat fish cat").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("cat")),
        ("new_string", serde_json::json!("bird")),
        ("replace_all", serde_json::json!(true)),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    assert_eq!(result.metadata["replacements"], serde_json::json!(3));

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "bird dog bird fish bird");
}

/// Verify that FileEditTool rejects non-unique match without replace_all.
#[tokio::test]
async fn file_edit_rejects_ambiguous_match() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("ambiguous.txt");
    std::fs::write(&file_path, "aaa bbb aaa").unwrap();

    let tool = FileEditTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("old_string", serde_json::json!("aaa")),
        ("new_string", serde_json::json!("ccc")),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success, "ambiguous match should fail");
    assert!(result.error.unwrap().contains("2 times"));
}

/// Verify that FileReadTool supports offset and limit.
#[tokio::test]
async fn file_read_offset_and_limit() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("lines.txt");
    let content: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&file_path, &content).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("file_path", serde_json::json!(file_path.to_str().unwrap())),
        ("offset", serde_json::json!(5)),
        ("limit", serde_json::json!(3)),
    ]);
    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("line 5"), "should start at line 5");
    assert!(output.contains("line 7"), "should include line 7");
    assert!(!output.contains("line 8"), "should not include line 8");
    assert_eq!(result.metadata["lines_shown"], serde_json::json!(3));
}

/// Verify that FileReadTool detects binary files.
#[tokio::test]
async fn file_read_detects_binary() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("binary.bin");
    std::fs::write(&file_path, &[0u8, 1, 2, 3, 0, 5, 6, 7]).unwrap();

    let tool = FileReadTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("file_path", serde_json::json!(file_path.to_str().unwrap()))]);
    let result = tool.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Binary"));
}

// ========================================================================
// Search tool integration tests
// ========================================================================

/// Verify search finds patterns across multiple files.
#[tokio::test]
async fn search_finds_pattern_across_files() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "fn main() {\n    todo!()\n}\n").unwrap();
    std::fs::write(tmp.path().join("b.rs"), "fn helper() {}\n").unwrap();
    std::fs::write(tmp.path().join("c.txt"), "no functions here\n").unwrap();

    let tool = GrepTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("fn \\w+"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("fn main"), "should find fn main");
    assert!(output.contains("fn helper"), "should find fn helper");
}

/// Verify search respects glob filter.
#[tokio::test]
async fn search_respects_glob_filter() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("code.rs"), "fn foo() {}\n").unwrap();
    std::fs::write(tmp.path().join("readme.md"), "fn bar() {}\n").unwrap();

    let tool = GrepTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[
        ("pattern", serde_json::json!("fn ")),
        ("glob", serde_json::json!("*.rs")),
    ]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(output.contains("foo"), "should find in .rs file");
    assert!(!output.contains("bar"), "should not find in .md file");
}

/// Verify search reports no matches gracefully.
#[tokio::test]
async fn search_no_matches_is_not_error() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("file.txt"), "hello world\n").unwrap();

    let tool = GrepTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("nonexistent_pattern_xyz"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "no-match should still be success");
    assert!(result.output.unwrap().contains("No matches"));
}

/// Verify search auto-promotes invalid regex to fixed-string mode instead of failing.
#[tokio::test]
async fn search_invalid_regex_becomes_fixed_string() {
    let tool = GrepTool;
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    std::fs::write(dir.join("test.txt"), "[unclosed is literal text\n").unwrap();
    let ctx = ToolContext::new(dir.to_str().unwrap());
    let args = make_args(&[("pattern", serde_json::json!("[unclosed"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "invalid regex should auto-promote to fixed-string search");
}

// ========================================================================
// Git tool integration tests
// ========================================================================

/// Verify git init, add, commit, log workflow in a temp directory.
#[tokio::test]
async fn git_init_commit_and_log() {
    let tmp = TempDir::new().unwrap();

    // Initialize a git repo using BashTool (since GitTool doesn't have init)
    let bash = BashTool::new();
    let ctx = ToolContext::new(tmp.path());

    let init_result = bash
        .execute(
            make_args(&[("command", serde_json::json!("git init && git config user.email 'test@test.com' && git config user.name 'Test'"))]),
            &ctx,
        )
        .await;
    assert!(init_result.success, "git init should succeed");

    // Create a file and stage it
    std::fs::write(tmp.path().join("hello.txt"), "hello world\n").unwrap();
    let add_result = bash
        .execute(
            make_args(&[("command", serde_json::json!("git add hello.txt"))]),
            &ctx,
        )
        .await;
    assert!(add_result.success);

    // Commit using GitTool
    let git = GitTool;
    let commit_args = make_args(&[
        ("action", serde_json::json!("commit")),
        ("message", serde_json::json!("Initial commit")),
    ]);
    let result = git.execute(commit_args, &ctx).await;
    assert!(result.success, "commit should succeed: {:?}", result.error);

    // Check log
    let log_args = make_args(&[
        ("action", serde_json::json!("log")),
        ("limit", serde_json::json!(5)),
    ]);
    let result = git.execute(log_args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(
        output.contains("Initial commit"),
        "log should show commit message"
    );

    // Check status (should be clean)
    let status_args = make_args(&[("action", serde_json::json!("status"))]);
    let result = git.execute(status_args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(
        output.contains("clean") || output.contains("Changes (0)"),
        "repo should be clean after commit, got: {output}"
    );
}

/// Verify git status in a non-repo directory fails gracefully.
#[tokio::test]
async fn git_status_non_repo_fails() {
    let tmp = TempDir::new().unwrap();
    let git = GitTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("action", serde_json::json!("status"))]);

    let result = git.execute(args, &ctx).await;
    assert!(!result.success, "status in non-repo should fail");
}

/// Verify git diff on staged changes.
#[tokio::test]
async fn git_diff_staged_changes() {
    let tmp = TempDir::new().unwrap();
    let bash = BashTool::new();
    let ctx = ToolContext::new(tmp.path());

    // Init repo and make initial commit
    bash.execute(
        make_args(&[("command", serde_json::json!(
            "git init && git config user.email 'test@test.com' && git config user.name 'Test' && echo initial > file.txt && git add . && git commit -m 'init'"
        ))]),
        &ctx,
    ).await;

    // Modify and stage
    std::fs::write(tmp.path().join("file.txt"), "modified\n").unwrap();
    bash.execute(
        make_args(&[("command", serde_json::json!("git add file.txt"))]),
        &ctx,
    )
    .await;

    let git = GitTool;
    let diff_args = make_args(&[
        ("action", serde_json::json!("diff")),
        ("staged", serde_json::json!(true)),
    ]);
    let result = git.execute(diff_args, &ctx).await;
    assert!(result.success);
    let output = result.output.unwrap();
    assert!(
        output.contains("modified"),
        "diff should show staged changes"
    );
}

/// Verify force push to protected branch is blocked.
#[tokio::test]
async fn git_force_push_to_main_blocked() {
    let git = GitTool;
    let ctx = ToolContext::new("/tmp");
    let args = make_args(&[
        ("action", serde_json::json!("push")),
        ("branch", serde_json::json!("main")),
        ("force", serde_json::json!(true)),
    ]);

    let result = git.execute(args, &ctx).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("protected branch"));
}

// ========================================================================
// Fuzzy matching (edit_replacers) integration tests
// ========================================================================

/// find_match resolves through all 9 passes for progressively fuzzier input.
#[test]
fn fuzzy_match_passes_integration() {
    use opendev_tools_impl::edit_replacers::find_match;

    // Pass 1: Simple exact match
    let original = "fn main() {\n    println!(\"hello\");\n}";
    let result = find_match(original, "println!(\"hello\");").unwrap();
    assert_eq!(result.pass_name, "simple");

    // Pass 2: LineTrimmed - LLM provides without indentation
    let original = "fn foo() {\n    let x = 1;\n    let y = 2;\n}";
    let result = find_match(original, "let x = 1;\nlet y = 2;").unwrap();
    assert_eq!(result.pass_name, "line_trimmed");
    assert_eq!(result.actual, "    let x = 1;\n    let y = 2;");

    // Pass 4: WhitespaceNormalized - extra spaces
    let original = "let   x  =   1;";
    let result = find_match(original, "let x = 1;").unwrap();
    assert_eq!(result.pass_name, "whitespace_normalized");
    assert_eq!(result.actual, "let   x  =   1;");

    // Pass 6: EscapeNormalized - literal \n instead of newline
    let original = "let s = \"hello\nworld\";";
    let result = find_match(original, "let s = \"hello\\nworld\";").unwrap();
    assert_eq!(result.pass_name, "escape_normalized");

    // Pass 7: TrimmedBoundary
    let result = opendev_tools_impl::edit_replacers::find_match("abc xyz def", "  xyz  ").unwrap();
    assert!(result.actual.contains("xyz"));

    // No match at all
    let result = find_match("hello world", "completely different text that is nowhere");
    assert!(result.is_none());
}

/// unified_diff generates correct diff output for modifications.
#[test]
fn unified_diff_output_format() {
    use opendev_tools_impl::edit_replacers::unified_diff;

    let original = "line1\nline2\nline3\nline4\n";
    let modified = "line1\nline2_changed\nline3\nline4\nline5\n";

    let diff = unified_diff("test.rs", original, modified, 3);

    assert!(diff.contains("--- a/test.rs"));
    assert!(diff.contains("+++ b/test.rs"));
    assert!(diff.contains("-line2"));
    assert!(diff.contains("+line2_changed"));
    assert!(diff.contains("+line5"));
}

/// unified_diff returns empty string for identical inputs.
#[test]
fn unified_diff_no_changes_returns_empty() {
    use opendev_tools_impl::edit_replacers::unified_diff;

    let text = "same\ncontent\n";
    let diff = unified_diff("test.rs", text, text, 3);
    assert!(diff.is_empty());
}

/// find_occurrence_positions locates all instances of a pattern.
#[test]
fn find_occurrence_positions_multiple() {
    use opendev_tools_impl::edit_replacers::find_occurrence_positions;

    let content = "foo\nbar\nfoo\nbaz\nfoo";
    let positions = find_occurrence_positions(content, "foo");
    assert_eq!(positions, vec![1, 3, 5]);

    // No occurrences
    let positions = find_occurrence_positions(content, "xyz");
    assert!(positions.is_empty());
}

/// CRLF line endings are normalized before matching.
#[test]
fn fuzzy_match_normalizes_crlf() {
    use opendev_tools_impl::edit_replacers::find_match;

    let original = "line1\r\nline2\r\nline3";
    let result = find_match(original, "line2").unwrap();
    assert_eq!(result.pass_name, "simple");
    assert_eq!(result.actual, "line2");
}

// ========================================================================
// FileListTool integration tests
// ========================================================================

/// FileListTool lists files in a directory with correct structure.
#[tokio::test]
async fn file_list_shows_directory_contents() {
    use opendev_tools_impl::FileListTool;

    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("file1.rs"), "fn main() {}").unwrap();
    std::fs::write(tmp.path().join("file2.txt"), "hello").unwrap();
    std::fs::create_dir_all(tmp.path().join("subdir")).unwrap();
    std::fs::write(tmp.path().join("subdir").join("nested.rs"), "mod nested;").unwrap();

    let tool = FileListTool;
    let ctx = ToolContext::new(tmp.path());
    let args = make_args(&[("pattern", serde_json::json!("**/*"))]);

    let result = tool.execute(args, &ctx).await;
    assert!(
        result.success,
        "file_list should succeed: {:?}",
        result.error
    );
    let output = result.output.unwrap();
    assert!(output.contains("file1.rs"));
    assert!(output.contains("file2.txt"));
    assert!(output.contains("nested.rs"));
}

/// Verify git branch creation.
#[tokio::test]
async fn git_branch_creation() {
    let tmp = TempDir::new().unwrap();
    let bash = BashTool::new();
    let ctx = ToolContext::new(tmp.path());

    bash.execute(
        make_args(&[("command", serde_json::json!(
            "git init && git config user.email 'test@test.com' && git config user.name 'Test' && echo x > f.txt && git add . && git commit -m 'init'"
        ))]),
        &ctx,
    ).await;

    let git = GitTool;
    let args = make_args(&[
        ("action", serde_json::json!("branch")),
        ("branch", serde_json::json!("feature-test")),
    ]);
    let result = git.execute(args, &ctx).await;
    assert!(
        result.success,
        "branch creation should succeed: {:?}",
        result.error
    );

    // List branches
    let list_args = make_args(&[("action", serde_json::json!("branch"))]);
    let result = git.execute(list_args, &ctx).await;
    assert!(result.success);
    assert!(result.output.unwrap().contains("feature-test"));
}
