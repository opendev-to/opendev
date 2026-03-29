use super::*;
use tempfile::TempDir;

#[test]
fn test_strip_path() {
    assert_eq!(unified::strip_path("a/b/c.rs", 1), "b/c.rs");
    assert_eq!(unified::strip_path("a/b/c.rs", 2), "c.rs");
    assert_eq!(unified::strip_path("c.rs", 0), "c.rs");
}

#[test]
fn test_parse_hunk_header() {
    let hb = unified::parse_hunk_header("@@ -10,5 +10,7 @@ fn main()").unwrap();
    assert_eq!(hb.old_start, 10);
}

#[tokio::test]
async fn test_patch_missing() {
    let tool = PatchTool;
    let ctx = ToolContext::new("/tmp");
    let result = tool.execute(HashMap::new(), &ctx).await;
    assert!(!result.success);
}

#[test]
fn test_apply_hunks_simple() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();

    // Use apply_patch_manually with a crafted unified diff
    let patch = "--- a/test.txt\n+++ b/test.txt\n@@ -1,3 +1,3 @@\n line1\n-line2\n+line2_modified\n line3\n";
    let result = unified::apply_patch_manually(patch, tmp.path(), 1);
    assert!(result.success, "Failed: {:?}", result.error);
    let content = std::fs::read_to_string(tmp.path().join("test.txt")).unwrap();
    assert!(content.contains("line2_modified"));
    assert!(!content.contains("\nline2\n"));
}

// -----------------------------------------------------------------------
// Structured patch format tests
// -----------------------------------------------------------------------

#[test]
fn test_is_structured_patch() {
    assert!(structured::is_structured_patch(
        "*** Begin Patch\n*** End Patch"
    ));
    assert!(structured::is_structured_patch(
        "\n*** Begin Patch\n*** Add File: foo.rs\n*** End Patch"
    ));
    assert!(!structured::is_structured_patch(
        "--- a/foo.rs\n+++ b/foo.rs\n"
    ));
    assert!(!structured::is_structured_patch("random text"));
}

#[test]
fn test_structured_add_file() {
    let tmp = TempDir::new().unwrap();
    let patch = "\
*** Begin Patch
*** Add File: src/new_file.rs
fn main() {
println!(\"hello\");
}
*** End Patch";

    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(result.success, "Failed: {:?}", result.output);

    let content = std::fs::read_to_string(tmp.path().join("src/new_file.rs")).unwrap();
    assert!(content.contains("fn main()"));
    assert!(content.contains("println!(\"hello\")"));
}

#[test]
fn test_structured_delete_file() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("old.txt"), "content").unwrap();

    let patch = "\
*** Begin Patch
*** Delete File: old.txt
*** End Patch";

    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(result.success, "Failed: {:?}", result.output);
    assert!(!tmp.path().join("old.txt").exists());
}

#[test]
fn test_structured_move_file() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("old.rs"), "fn hello() {}").unwrap();

    let patch = "\
*** Begin Patch
*** Move File: old.rs -> subdir/new.rs
*** End Patch";

    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(result.success, "Failed: {:?}", result.output);
    assert!(!tmp.path().join("old.rs").exists());
    let content = std::fs::read_to_string(tmp.path().join("subdir/new.rs")).unwrap();
    assert_eq!(content, "fn hello() {}");
}

#[test]
fn test_structured_update_file() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("main.rs"),
        "fn main() {\n    println!(\"old\");\n    return;\n}\n",
    )
    .unwrap();

    let patch = "\
*** Begin Patch
*** Update File: main.rs
 fn main() {
-    println!(\"old\");
+    println!(\"new\");
+    println!(\"extra\");
 return;
*** End Patch";

    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(result.success, "Failed: {:?}", result.output);

    let content = std::fs::read_to_string(tmp.path().join("main.rs")).unwrap();
    assert!(content.contains("println!(\"new\")"));
    assert!(content.contains("println!(\"extra\")"));
    assert!(!content.contains("println!(\"old\")"));
    assert!(content.contains("return;"));
}

#[test]
fn test_structured_update_multiple_locations() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("lib.rs"),
        "use std::io;\n\nfn alpha() {\n    // alpha\n}\n\nfn beta() {\n    // beta\n}\n",
    )
    .unwrap();

    let patch = "\
*** Begin Patch
*** Update File: lib.rs
 fn alpha() {
-    // alpha
+    // alpha modified
 }

 fn beta() {
-    // beta
+    // beta modified
 }
*** End Patch";

    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(result.success, "Failed: {:?}", result.output);

    let content = std::fs::read_to_string(tmp.path().join("lib.rs")).unwrap();
    assert!(content.contains("// alpha modified"));
    assert!(content.contains("// beta modified"));
    assert!(!content.contains("\n    // alpha\n"));
    assert!(!content.contains("\n    // beta\n"));
}

#[test]
fn test_structured_mixed_operations() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("existing.rs"), "fn existing() {}\n").unwrap();
    std::fs::write(tmp.path().join("to_delete.rs"), "fn old() {}\n").unwrap();

    let patch = "\
*** Begin Patch
*** Add File: new.rs
fn new() {}
*** Delete File: to_delete.rs
*** Update File: existing.rs
-fn existing() {}
+fn existing() { 42 }
*** End Patch";

    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(result.success, "Failed: {:?}", result.output);

    assert!(tmp.path().join("new.rs").exists());
    assert!(!tmp.path().join("to_delete.rs").exists());
    let content = std::fs::read_to_string(tmp.path().join("existing.rs")).unwrap();
    assert!(content.contains("fn existing() { 42 }"));
}

#[test]
fn test_structured_empty_patch() {
    let tmp = TempDir::new().unwrap();
    let patch = "*** Begin Patch\n*** End Patch";
    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(!result.success);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("No operations")
    );
}

#[test]
fn test_structured_update_with_trimmed_match() {
    let tmp = TempDir::new().unwrap();
    // File has trailing spaces on a line
    std::fs::write(
        tmp.path().join("spaced.rs"),
        "fn main() {  \n    old_line\n}\n",
    )
    .unwrap();

    let patch = "\
*** Begin Patch
*** Update File: spaced.rs
 fn main() {
-    old_line
+    new_line
 }
*** End Patch";

    let result = structured::apply_structured_patch(patch, tmp.path());
    assert!(result.success, "Failed: {:?}", result.output);

    let content = std::fs::read_to_string(tmp.path().join("spaced.rs")).unwrap();
    assert!(content.contains("new_line"));
}

#[test]
fn test_apply_context_changes_no_changes() {
    let content = "line1\nline2\n";
    let changes: Vec<String> = vec![" line1".to_string(), " line2".to_string()];
    let result = structured::apply_context_changes(content, &changes).unwrap();
    assert_eq!(result, content);
}

#[test]
fn test_parse_change_groups_simple() {
    let changes = vec![
        " context".to_string(),
        "-old".to_string(),
        "+new".to_string(),
    ];
    let groups = structured::parse_change_groups(&changes);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].context_before, vec!["context"]);
    assert_eq!(groups[0].removals, vec!["old"]);
    assert_eq!(groups[0].additions, vec!["new"]);
}

#[tokio::test]
async fn test_execute_routes_structured_patch() {
    let tmp = TempDir::new().unwrap();
    let ctx = ToolContext::new(tmp.path().to_str().unwrap());

    let tool = PatchTool;
    let mut args = HashMap::new();
    args.insert(
        "patch".to_string(),
        serde_json::Value::String(
            "*** Begin Patch\n*** Add File: hello.txt\nhello world\n*** End Patch".to_string(),
        ),
    );

    let result = tool.execute(args, &ctx).await;
    assert!(result.success, "Failed: {:?}", result.output);
    assert!(tmp.path().join("hello.txt").exists());
}

mod proptest_patch {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// parse_hunk_header must never panic on arbitrary input.
        #[test]
        fn fuzz_hunk_header_no_panic(line in "\\PC*") {
            let _ = unified::parse_hunk_header(&line);
        }

        /// strip_path must never panic on arbitrary input.
        #[test]
        fn fuzz_strip_path_no_panic(
            path in "\\PC{0,200}",
            strip in 0usize..10
        ) {
            let _ = unified::strip_path(&path, strip);
        }

        /// Valid hunk headers must be parsed correctly.
        #[test]
        fn valid_hunk_header_parsed(
            old_start in 1usize..10000,
            old_count in 0usize..1000,
            new_start in 1usize..10000,
            new_count in 0usize..1000,
        ) {
            let line = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@");
            let result = unified::parse_hunk_header(&line);
            prop_assert!(result.is_some(), "Failed to parse: {}", line);
            let hb = result.unwrap();
            prop_assert_eq!(hb.old_start, old_start);
        }

        /// apply_patch_manually must not panic on arbitrary patch content.
        #[test]
        fn fuzz_apply_patch_manually_no_panic(
            patch in "\\PC{0,1000}",
            strip in 0usize..5,
        ) {
            let tmp = TempDir::new().unwrap();
            // Create a dummy file so patch application has something to work with
            std::fs::write(tmp.path().join("test.txt"), "line1\nline2\nline3\n").unwrap();
            // Should not panic — errors are returned as ToolResult
            let _ = unified::apply_patch_manually(&patch, tmp.path(), strip);
        }
    }
}
