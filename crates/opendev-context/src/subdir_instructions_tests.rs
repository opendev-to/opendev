use super::*;

#[test]
fn test_tracker_new_with_startup_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create a startup instruction file
    let agents_md = root.join("AGENTS.md");
    std::fs::write(&agents_md, "# Project rules").unwrap();

    let tracker = SubdirInstructionTracker::new(root.clone(), &[agents_md.clone()]);
    assert_eq!(tracker.injected_count(), 1);
}

#[test]
fn test_check_file_read_finds_subdir_instruction() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create subdirectory with AGENTS.md
    let subdir = root.join("src").join("payments");
    std::fs::create_dir_all(&subdir).unwrap();
    let agents_md = subdir.join("AGENTS.md");
    std::fs::write(&agents_md, "# Payment rules\nBe careful with money").unwrap();

    // Create a file in that subdirectory
    let file = subdir.join("checkout.rs");
    std::fs::write(&file, "fn checkout() {}").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Payment rules"));
    assert!(results[0].relative_path.contains("AGENTS.md"));
}

#[test]
fn test_check_file_read_deduplicates() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let subdir = root.join("src");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(subdir.join("AGENTS.md"), "rules").unwrap();
    std::fs::write(subdir.join("a.rs"), "").unwrap();
    std::fs::write(subdir.join("b.rs"), "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    // First read finds the instruction
    let r1 = tracker.check_file_read(&subdir.join("a.rs"));
    assert_eq!(r1.len(), 1);

    // Second read in same dir should not re-inject
    let r2 = tracker.check_file_read(&subdir.join("b.rs"));
    assert_eq!(r2.len(), 0);
}

#[test]
fn test_check_file_read_skips_startup_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create root-level AGENTS.md (already injected at startup)
    let agents_md = root.join("AGENTS.md");
    std::fs::write(&agents_md, "root rules").unwrap();

    let file = root.join("main.rs");
    std::fs::write(&file, "fn main() {}").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[agents_md]);

    // Should not find anything — root AGENTS.md was already injected
    let results = tracker.check_file_read(&file);
    assert_eq!(results.len(), 0);
}

#[test]
fn test_walks_up_to_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create instruction files at different levels
    let deep = root.join("a").join("b").join("c");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(root.join("a").join("CLAUDE.md"), "level a").unwrap();
    std::fs::write(deep.join("AGENTS.md"), "level c").unwrap();

    let file = deep.join("file.rs");
    std::fs::write(&file, "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    // Should find both: a/CLAUDE.md and a/b/c/AGENTS.md
    assert_eq!(results.len(), 2);
}

#[test]
fn test_context_md_recognized() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let subdir = root.join("lib");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(subdir.join("CONTEXT.md"), "deprecated but supported").unwrap();

    let file = subdir.join("util.rs");
    std::fs::write(&file, "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("deprecated but supported"));
}

#[test]
fn test_cursorrules_discovered() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create a .cursorrules file at project root
    std::fs::write(
        root.join(".cursorrules"),
        "Always use TypeScript strict mode",
    )
    .unwrap();

    let file = root.join("index.ts");
    std::fs::write(&file, "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("TypeScript strict mode"));
}

#[test]
fn test_reset_after_compaction_clears_subdirectory_instructions() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let subdir = root.join("src").join("payments");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(subdir.join("AGENTS.md"), "Payment rules").unwrap();
    std::fs::write(subdir.join("checkout.rs"), "fn checkout() {}").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    // Inject instruction
    let results = tracker.check_file_read(&subdir.join("checkout.rs"));
    assert_eq!(results.len(), 1);

    // Simulate compaction removing all messages
    tracker.reset_after_compaction(&[], &[]);

    // Should be able to re-inject
    let results2 = tracker.check_file_read(&subdir.join("checkout.rs"));
    assert_eq!(results2.len(), 1);
}

#[test]
fn test_reset_preserves_startup_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let agents_md = root.join("AGENTS.md");
    std::fs::write(&agents_md, "root rules").unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}").unwrap();

    let startup = vec![agents_md.clone()];
    let mut tracker = SubdirInstructionTracker::new(root.clone(), &startup);
    assert_eq!(tracker.injected_count(), 1);

    // Reset should preserve startup files
    tracker.reset_after_compaction(&startup, &[]);
    assert_eq!(tracker.injected_count(), 1);

    // Root AGENTS.md should still not be re-injected
    let results = tracker.check_file_read(&root.join("main.rs"));
    assert_eq!(results.len(), 0);
}

#[test]
fn test_reset_preserves_instructions_still_in_messages() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let subdir = root.join("src");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(subdir.join("AGENTS.md"), "src rules").unwrap();
    std::fs::write(subdir.join("lib.rs"), "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    // Inject
    let results = tracker.check_file_read(&subdir.join("lib.rs"));
    assert_eq!(results.len(), 1);

    // Simulate compaction that keeps the instruction in remaining messages
    let remaining = vec![serde_json::json!({
        "role": "user",
        "content": format!("Instructions from AGENTS.md in {}", subdir.display()),
    })];
    tracker.reset_after_compaction(&[], &remaining);

    // Should NOT re-inject since it's still in messages
    let results2 = tracker.check_file_read(&subdir.join("lib.rs"));
    assert_eq!(results2.len(), 0);
}

#[test]
fn test_reinjection_after_reset() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let sub_a = root.join("a");
    let sub_b = root.join("b");
    std::fs::create_dir_all(&sub_a).unwrap();
    std::fs::create_dir_all(&sub_b).unwrap();
    std::fs::write(sub_a.join("AGENTS.md"), "rules a").unwrap();
    std::fs::write(sub_b.join("AGENTS.md"), "rules b").unwrap();
    std::fs::write(sub_a.join("f.rs"), "").unwrap();
    std::fs::write(sub_b.join("g.rs"), "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    // Inject both
    assert_eq!(tracker.check_file_read(&sub_a.join("f.rs")).len(), 1);
    assert_eq!(tracker.check_file_read(&sub_b.join("g.rs")).len(), 1);

    // Reset with empty messages — both cleared
    tracker.reset_after_compaction(&[], &[]);

    // Both should re-inject
    assert_eq!(tracker.check_file_read(&sub_a.join("f.rs")).len(), 1);
    assert_eq!(tracker.check_file_read(&sub_b.join("g.rs")).len(), 1);
}

#[test]
fn test_copilot_instructions_discovered() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Create .github/copilot-instructions.md
    let github_dir = root.join(".github");
    std::fs::create_dir_all(&github_dir).unwrap();
    std::fs::write(
        github_dir.join("copilot-instructions.md"),
        "Use conventional commits",
    )
    .unwrap();

    let file = root.join("main.rs");
    std::fs::write(&file, "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("conventional commits"));
}

#[test]
fn test_opendev_rules_directory_discovered() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let rules_dir = root.join(".opendev").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("style.md"), "Use 4 spaces").unwrap();

    let file = root.join("main.rs");
    std::fs::write(&file, "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    assert!(
        results.iter().any(|r| r.content.contains("4 spaces")),
        "Should discover .opendev/rules/style.md"
    );
}

#[test]
fn test_opendev_rules_with_frontmatter_paths() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let rules_dir = root.join(".opendev").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join("rust.md"),
        "---\npaths:\n  - \"**/*.rs\"\n---\nUse snake_case",
    )
    .unwrap();

    let file = root.join("main.rs");
    std::fs::write(&file, "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    let rust_rule = results.iter().find(|r| r.content.contains("snake_case"));
    assert!(rust_rule.is_some());
    assert!(rust_rule.unwrap().path_globs.is_some());
    assert_eq!(
        rust_rule.unwrap().path_globs.as_ref().unwrap()[0],
        "**/*.rs"
    );
}

#[test]
fn test_html_comments_stripped_in_subdir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let subdir = root.join("src");
    std::fs::create_dir_all(&subdir).unwrap();
    std::fs::write(
        subdir.join("AGENTS.md"),
        "Visible\n<!-- hidden -->\nAlso visible",
    )
    .unwrap();

    let file = subdir.join("main.rs");
    std::fs::write(&file, "").unwrap();

    let mut tracker = SubdirInstructionTracker::new(root.clone(), &[]);

    let results = tracker.check_file_read(&file);
    assert_eq!(results.len(), 1);
    assert!(results[0].content.contains("Visible"));
    assert!(results[0].content.contains("Also visible"));
    assert!(!results[0].content.contains("hidden"));
}
