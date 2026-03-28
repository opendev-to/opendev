//! Search tools — `grep` (ripgrep) and `ast_grep` (ast-grep structural search).

mod ast_grep_tool;
mod backends;
mod excludes;
mod grep_tool;
mod types;

use std::time::Duration;

pub use ast_grep_tool::AstGrepTool;
pub use excludes::{DEFAULT_SEARCH_EXCLUDE_GLOBS, DEFAULT_SEARCH_EXCLUDES, default_ignore_file};
pub use grep_tool::GrepTool;

// ===========================================================================
// Shared constants and helpers
// ===========================================================================

const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Apply offset and head_limit to output lines.
fn apply_pagination(output: &str, offset: usize, head_limit: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let start = offset.min(lines.len());
    let selected = &lines[start..];
    let selected = if head_limit > 0 {
        &selected[..head_limit.min(selected.len())]
    } else {
        selected
    };
    let mut result = selected.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::TempDir;

    use opendev_tools_core::{BaseTool, ToolContext};
    use types::{AstGrepArgs, GrepArgs, OutputMode};

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> std::collections::HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    // --- Unit tests for default exclusions ---

    #[test]
    fn test_build_rg_command_includes_ignore_file() {
        let args =
            GrepArgs::from_map(&make_args(&[("pattern", serde_json::json!("hello"))])).unwrap();
        let cmd = GrepTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();

        // Verify --ignore-file is present
        assert!(
            cmd_args.contains(&"--ignore-file".to_string()),
            "should include --ignore-file flag"
        );
    }

    #[test]
    fn test_default_ignore_file_contents() {
        let path = default_ignore_file().expect("should create ignore file");
        let content = fs::read_to_string(path).unwrap();
        assert!(
            content.contains("node_modules/"),
            "should contain node_modules/"
        );
        assert!(
            content.contains("__pycache__/"),
            "should contain __pycache__/"
        );
        assert!(content.contains(".git/"), "should contain .git/");
        assert!(content.contains("target/"), "should contain target/");
        assert!(content.contains("*.min.js"), "should contain *.min.js");
        assert!(content.contains("*.pyc"), "should contain *.pyc");
    }

    #[test]
    fn test_default_exclusion_lists_not_empty() {
        assert!(!DEFAULT_SEARCH_EXCLUDES.is_empty());
        assert!(!DEFAULT_SEARCH_EXCLUDE_GLOBS.is_empty());
        for entry in DEFAULT_SEARCH_EXCLUDES {
            assert!(!entry.is_empty());
        }
        for pat in DEFAULT_SEARCH_EXCLUDE_GLOBS {
            assert!(
                pat.starts_with('*'),
                "glob pattern should start with '*': {pat}"
            );
        }
    }

    #[tokio::test]
    async fn test_grep_excludes_node_modules() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::create_dir_all(tmp.path().join("node_modules/pkg")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "fn hello() {}").unwrap();
        fs::write(
            tmp.path().join("node_modules/pkg/index.js"),
            "function hello() {}",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("hello"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("main.rs"),
            "should find hello in src/main.rs"
        );
        assert!(
            !output.contains("node_modules"),
            "should not search node_modules, got: {output}"
        );
    }

    // --- Unit tests for argument parsing ---

    #[test]
    fn test_parse_grep_args_minimal() {
        let args = make_args(&[("pattern", serde_json::json!("hello"))]);
        let parsed = GrepArgs::from_map(&args).unwrap();
        assert_eq!(parsed.pattern, "hello");
        assert_eq!(parsed.output_mode, OutputMode::Content);
        assert!(!parsed.case_insensitive);
        assert!(!parsed.multiline);
        assert!(!parsed.fixed_string);
        assert!(parsed.line_numbers);
        assert_eq!(parsed.head_limit, 0);
        assert_eq!(parsed.offset, 0);
    }

    #[test]
    fn test_parse_grep_args_all_options() {
        let args = make_args(&[
            ("pattern", serde_json::json!("test")),
            ("path", serde_json::json!("/tmp")),
            ("glob", serde_json::json!("*.rs")),
            ("type", serde_json::json!("rust")),
            ("-i", serde_json::json!(true)),
            ("multiline", serde_json::json!(true)),
            ("fixed_string", serde_json::json!(true)),
            ("output_mode", serde_json::json!("files_with_matches")),
            ("context", serde_json::json!(3)),
            ("-A", serde_json::json!(2)),
            ("-B", serde_json::json!(1)),
            ("-n", serde_json::json!(false)),
            ("head_limit", serde_json::json!(10)),
            ("offset", serde_json::json!(5)),
        ]);
        let parsed = GrepArgs::from_map(&args).unwrap();
        assert_eq!(parsed.pattern, "test");
        assert_eq!(parsed.path.as_deref(), Some("/tmp"));
        assert_eq!(parsed.glob.as_deref(), Some("*.rs"));
        assert_eq!(parsed.file_type.as_deref(), Some("rust"));
        assert!(parsed.case_insensitive);
        assert!(parsed.multiline);
        assert!(parsed.fixed_string);
        assert_eq!(parsed.output_mode, OutputMode::FilesWithMatches);
        assert_eq!(parsed.context, Some(3));
        assert_eq!(parsed.after_context, Some(2));
        assert_eq!(parsed.before_context, Some(1));
        assert!(!parsed.line_numbers);
        assert_eq!(parsed.head_limit, 10);
        assert_eq!(parsed.offset, 5);
    }

    #[test]
    fn test_parse_grep_args_missing_pattern() {
        let args = make_args(&[("glob", serde_json::json!("*.rs"))]);
        assert!(GrepArgs::from_map(&args).is_err());
    }

    #[test]
    fn test_parse_grep_args_invalid_output_mode() {
        let args = make_args(&[
            ("pattern", serde_json::json!("x")),
            ("output_mode", serde_json::json!("bogus")),
        ]);
        assert!(GrepArgs::from_map(&args).is_err());
    }

    #[test]
    fn test_parse_ast_grep_args_minimal() {
        let args = make_args(&[("pattern", serde_json::json!("fn $NAME($$$PARAMS)"))]);
        let parsed = AstGrepArgs::from_map(&args).unwrap();
        assert_eq!(parsed.pattern, "fn $NAME($$$PARAMS)");
        assert!(parsed.path.is_none());
        assert!(parsed.lang.is_none());
        assert_eq!(parsed.head_limit, 0);
    }

    #[test]
    fn test_parse_ast_grep_args_all_options() {
        let args = make_args(&[
            ("pattern", serde_json::json!("$OBJ.$METHOD($$$ARGS)")),
            ("path", serde_json::json!("src")),
            ("lang", serde_json::json!("rust")),
            ("head_limit", serde_json::json!(10)),
        ]);
        let parsed = AstGrepArgs::from_map(&args).unwrap();
        assert_eq!(parsed.pattern, "$OBJ.$METHOD($$$ARGS)");
        assert_eq!(parsed.path.as_deref(), Some("src"));
        assert_eq!(parsed.lang.as_deref(), Some("rust"));
        assert_eq!(parsed.head_limit, 10);
    }

    #[test]
    fn test_parse_ast_grep_args_missing_pattern() {
        let args = make_args(&[("lang", serde_json::json!("rust"))]);
        assert!(AstGrepArgs::from_map(&args).is_err());
    }

    // --- Unit tests for pagination ---

    #[test]
    fn test_pagination_no_limits() {
        let input = "line1\nline2\nline3\n";
        let result = apply_pagination(input, 0, 0);
        assert_eq!(result, "line1\nline2\nline3\n");
    }

    #[test]
    fn test_pagination_head_limit() {
        let input = "line1\nline2\nline3\nline4";
        let result = apply_pagination(input, 0, 2);
        assert_eq!(result, "line1\nline2\n");
    }

    #[test]
    fn test_pagination_offset() {
        let input = "line1\nline2\nline3\nline4";
        let result = apply_pagination(input, 2, 0);
        assert_eq!(result, "line3\nline4\n");
    }

    #[test]
    fn test_pagination_offset_and_limit() {
        let input = "line1\nline2\nline3\nline4\nline5";
        let result = apply_pagination(input, 1, 2);
        assert_eq!(result, "line2\nline3\n");
    }

    #[test]
    fn test_pagination_offset_beyond_end() {
        let input = "line1\nline2";
        let result = apply_pagination(input, 10, 0);
        assert_eq!(result, "");
    }

    // --- Unit tests for rg command building ---

    #[test]
    fn test_build_rg_command_basic() {
        let args =
            GrepArgs::from_map(&make_args(&[("pattern", serde_json::json!("hello"))])).unwrap();
        let cmd = GrepTool::build_rg_command(&args, Path::new("/tmp"));
        let prog = cmd.as_std().get_program();
        assert_eq!(prog, "rg");
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--no-heading")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--color=never")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-n")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("hello")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("/tmp")));
    }

    #[test]
    fn test_build_rg_command_files_with_matches() {
        let args = GrepArgs::from_map(&make_args(&[
            ("pattern", serde_json::json!("x")),
            ("output_mode", serde_json::json!("files_with_matches")),
        ]))
        .unwrap();
        let cmd = GrepTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-l")));
    }

    #[test]
    fn test_build_rg_command_count() {
        let args = GrepArgs::from_map(&make_args(&[
            ("pattern", serde_json::json!("x")),
            ("output_mode", serde_json::json!("count")),
        ]))
        .unwrap();
        let cmd = GrepTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-c")));
    }

    #[test]
    fn test_build_rg_command_all_flags() {
        let args = GrepArgs::from_map(&make_args(&[
            ("pattern", serde_json::json!("test")),
            ("glob", serde_json::json!("*.rs")),
            ("type", serde_json::json!("rust")),
            ("-i", serde_json::json!(true)),
            ("multiline", serde_json::json!(true)),
            ("fixed_string", serde_json::json!(true)),
            ("context", serde_json::json!(3)),
            ("-A", serde_json::json!(2)),
            ("-B", serde_json::json!(1)),
        ]))
        .unwrap();
        let cmd = GrepTool::build_rg_command(&args, Path::new("/tmp"));
        let cmd_args: Vec<_> = cmd.as_std().get_args().collect();
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-i")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-U")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--multiline-dotall")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-F")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("--context=3")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-A=2")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("-B=1")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("*.rs")));
        assert!(cmd_args.contains(&std::ffi::OsStr::new("rust")));
    }

    // --- Integration tests (require rg installed) ---

    #[tokio::test]
    async fn test_grep_basic_with_rg() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.rs"),
            "fn main() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("println"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("println"));
    }

    #[tokio::test]
    async fn test_grep_with_glob_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn foo() {}\n").unwrap();
        fs::write(tmp.path().join("b.txt"), "fn bar() {}\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn ")),
            ("glob", serde_json::json!("*.rs")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("foo"));
        assert!(!output.contains("bar"));
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.txt"), "hello world\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[("pattern", serde_json::json!("nonexistent"))]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.output.unwrap().contains("No matches"));
    }

    #[tokio::test]
    async fn test_grep_files_with_matches_mode() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn foo() {}\nfn foo2() {}\n").unwrap();
        fs::write(tmp.path().join("b.rs"), "fn bar() {}\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn ")),
            ("output_mode", serde_json::json!("files_with_matches")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("a.rs"));
        assert!(output.contains("b.rs"));
        assert!(!output.contains("foo"));
    }

    #[tokio::test]
    async fn test_grep_files_with_matches_sorted_by_mtime() {
        use std::fs::FileTimes;
        use std::time::SystemTime;

        let tmp = TempDir::new().unwrap();
        let now = SystemTime::now();

        fs::write(tmp.path().join("old.rs"), "fn target() {}\n").unwrap();
        let old_time = now - Duration::from_secs(60);
        let f = fs::File::options()
            .write(true)
            .open(tmp.path().join("old.rs"))
            .unwrap();
        f.set_times(FileTimes::new().set_modified(old_time))
            .unwrap();

        fs::write(tmp.path().join("mid.rs"), "fn target() {}\n").unwrap();
        let mid_time = now - Duration::from_secs(30);
        let f = fs::File::options()
            .write(true)
            .open(tmp.path().join("mid.rs"))
            .unwrap();
        f.set_times(FileTimes::new().set_modified(mid_time))
            .unwrap();

        fs::write(tmp.path().join("new.rs"), "fn target() {}\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("target")),
            ("output_mode", serde_json::json!("files_with_matches")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3, "should have 3 files, got: {output}");
        assert!(
            lines[0].contains("new.rs"),
            "first should be new.rs, got: {}",
            lines[0]
        );
        assert!(
            lines[1].contains("mid.rs"),
            "second should be mid.rs, got: {}",
            lines[1]
        );
        assert!(
            lines[2].contains("old.rs"),
            "third should be old.rs, got: {}",
            lines[2]
        );
    }

    #[tokio::test]
    async fn test_grep_count_mode() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn ")),
            ("output_mode", serde_json::json!("count")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains(":2"));
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.txt"), "Hello World\nhello world\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("HELLO")),
            ("-i", serde_json::json!(true)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("Hello"));
        assert!(output.contains("hello"));
    }

    #[tokio::test]
    async fn test_grep_fixed_string() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.txt"), "a.b\na+b\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("a.b")),
            ("fixed_string", serde_json::json!(true)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("a.b"));
        assert!(!output.contains("a+b"));
    }

    #[tokio::test]
    async fn test_grep_with_context() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.txt"),
            "line1\nline2\nTARGET\nline4\nline5\n",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("TARGET")),
            ("context", serde_json::json!(1)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("line2"));
        assert!(output.contains("TARGET"));
        assert!(output.contains("line4"));
    }

    #[tokio::test]
    async fn test_grep_path_not_found() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        let tool = GrepTool;
        let ctx = ToolContext::new(&dir_path);
        let args = make_args(&[
            ("pattern", serde_json::json!("x")),
            (
                "path",
                serde_json::json!(dir_path.join("nonexistent").to_str().unwrap()),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Path not found"));
    }

    #[tokio::test]
    async fn test_grep_path_not_found_shows_available_dirs() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().canonicalize().unwrap();
        fs::create_dir_all(dir_path.join("crates")).unwrap();
        fs::create_dir_all(dir_path.join("docs")).unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(&dir_path);
        let args = make_args(&[
            ("pattern", serde_json::json!("x")),
            (
                "path",
                serde_json::json!(dir_path.join("src").to_str().unwrap()),
            ),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let error = result.error.unwrap();
        assert!(error.contains("Path not found"));
        assert!(error.contains("crates"));
        assert!(error.contains("docs"));
    }

    #[tokio::test]
    async fn test_grep_invalid_regex_falls_back_to_fixed_string() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.txt"), "match [invalid here\n").unwrap();
        let tool = GrepTool;
        let ctx = ToolContext::new(dir.path());
        let args = make_args(&[("pattern", serde_json::json!("[invalid"))]);

        let result = tool.execute(args, &ctx).await;
        // Should succeed via fixed_string fallback instead of failing with "Invalid regex"
        assert!(result.success, "expected fixed_string fallback to succeed");
    }

    #[tokio::test]
    async fn test_grep_multiline() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("test.rs"),
            "struct Foo {\n    bar: i32,\n}\n",
        )
        .unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("struct.*\\{[\\s\\S]*?\\}")),
            ("multiline", serde_json::json!(true)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("struct Foo"));
    }

    #[tokio::test]
    async fn test_grep_with_file_type() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.rs"), "fn hello() {}\n").unwrap();
        fs::write(tmp.path().join("b.py"), "def hello(): pass\n").unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("hello")),
            ("type", serde_json::json!("rust")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("a.rs"));
        assert!(!output.contains("b.py"));
    }

    #[tokio::test]
    async fn test_grep_pagination() {
        let tmp = TempDir::new().unwrap();
        let mut content = String::new();
        for i in 1..=20 {
            content.push_str(&format!("line{i}\n"));
        }
        fs::write(tmp.path().join("test.txt"), &content).unwrap();

        let tool = GrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("line")),
            ("offset", serde_json::json!(5)),
            ("head_limit", serde_json::json!(3)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3);
    }

    // --- Integration tests for ast_grep (require `sg` binary) ---

    /// Helper: skip test if ast-grep (sg) is not installed.
    fn sg_installed() -> bool {
        std::process::Command::new("sg")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[tokio::test]
    async fn test_ast_grep_rust_struct_pattern() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("lib.rs"),
            "pub struct Config {\n    pub name: String,\n    pub value: i32,\n}\n",
        )
        .unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            (
                "pattern",
                serde_json::json!("pub struct $NAME { $$$FIELDS }"),
            ),
            ("lang", serde_json::json!("rust")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success, "should succeed: {:?}", result.output);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("Config"),
            "should find struct Config: {output}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_rust_impl_pattern() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("lib.rs"),
            concat!(
                "pub struct Foo;\n",
                "pub trait Bar { fn bar(&self); }\n",
                "impl Bar for Foo {\n    fn bar(&self) {}\n}\n",
            ),
        )
        .unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            (
                "pattern",
                serde_json::json!("impl $TRAIT for $TYPE { $$$BODY }"),
            ),
            ("lang", serde_json::json!("rust")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("Bar"),
            "should find impl Bar for Foo: {output}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_typescript_function() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("index.ts"),
            "function greet(name: string): void {\n  console.log(name);\n}\n",
        )
        .unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            (
                "pattern",
                serde_json::json!("function $NAME($$$ARGS): void { $$$BODY }"),
            ),
            ("lang", serde_json::json!("typescript")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("greet"),
            "should find greet function: {output}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_call_pattern() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("app.js"),
            "console.log('hello');\nconsole.log('world');\nalert('test');\n",
        )
        .unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("console.log($$$ARGS)")),
            ("lang", serde_json::json!("javascript")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("console.log"),
            "should find console.log calls: {output}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_no_matches() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("main.rs"), "fn main() {}\n").unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            (
                "pattern",
                serde_json::json!("pub struct $NAME { $$$FIELDS }"),
            ),
            ("lang", serde_json::json!("rust")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("No structural matches found"),
            "should report no matches: {output}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_invalid_pattern_error() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("index.ts"), "class Foo {}\n").unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            (
                "pattern",
                serde_json::json!("$NAME($$$ARGS): void { $$$BODY }"),
            ),
            ("lang", serde_json::json!("typescript")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success, "should fail for invalid pattern");
        let error = result.error.unwrap_or_default();
        assert!(
            error.contains("Hint:"),
            "should include helpful hint: {error}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_path_not_found() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("fn $NAME()")),
            ("path", serde_json::json!("nonexistent_dir")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        let error = result.error.unwrap_or_default();
        assert!(
            error.contains("Path not found"),
            "should report path not found: {error}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_with_head_limit() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("lib.rs"),
            concat!(
                "pub struct A;\npub struct B;\npub struct C;\n",
                "pub struct D;\npub struct E;\n",
            ),
        )
        .unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("pub struct $NAME;")),
            ("lang", serde_json::json!("rust")),
            ("head_limit", serde_json::json!(2)),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        // Should show only 2 matches but mention total
        let match_lines: Vec<&str> = output
            .lines()
            .filter(|l| l.contains("pub struct"))
            .collect();
        assert_eq!(match_lines.len(), 2, "should limit to 2 matches: {output}");
        assert!(
            output.contains("of 5 matches shown"),
            "should mention total: {output}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_go_function() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("main.go"),
            "package main\n\nfunc greet(name string) {\n\tfmt.Println(name)\n}\n",
        )
        .unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            (
                "pattern",
                serde_json::json!("func $NAME($$$ARGS) { $$$BODY }"),
            ),
            ("lang", serde_json::json!("go")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        assert!(
            output.contains("greet"),
            "should find greet function: {output}"
        );
    }

    #[tokio::test]
    async fn test_ast_grep_multiple_matches() {
        if !sg_installed() {
            return;
        }
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("app.js"),
            concat!(
                "console.log('a');\n",
                "console.log('b');\n",
                "console.log('c');\n",
            ),
        )
        .unwrap();

        let tool = AstGrepTool;
        let ctx = ToolContext::new(tmp.path());
        let args = make_args(&[
            ("pattern", serde_json::json!("console.log($$$ARGS)")),
            ("lang", serde_json::json!("javascript")),
        ]);

        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap_or_default();
        let match_lines: Vec<&str> = output
            .lines()
            .filter(|l| l.contains("console.log"))
            .collect();
        assert_eq!(
            match_lines.len(),
            3,
            "should find all 3 console.log calls: {output}"
        );
    }
}
