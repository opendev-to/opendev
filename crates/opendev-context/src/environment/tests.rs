use super::*;
use tempfile::TempDir;

#[test]
fn test_detect_config_files() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
    std::fs::write(dir.path().join("Makefile"), "all:").unwrap();

    let configs = project::detect_config_files(dir.path());
    assert!(configs.contains(&"Cargo.toml".to_string()));
    assert!(configs.contains(&"Makefile".to_string()));
    assert!(!configs.contains(&"package.json".to_string()));
}

#[test]
fn test_infer_tech_stack() {
    let configs = vec!["Cargo.toml".to_string(), "Dockerfile".to_string()];
    let stack = project::infer_tech_stack(&configs);
    assert!(stack.contains(&"Rust".to_string()));
    assert!(stack.contains(&"Docker".to_string()));
}

#[test]
fn test_infer_tech_stack_dedup() {
    let configs = vec!["pyproject.toml".to_string(), "requirements.txt".to_string()];
    let stack = project::infer_tech_stack(&configs);
    // Both map to "Python", should be deduped
    assert_eq!(stack.iter().filter(|s| *s == "Python").count(), 1);
}

#[test]
fn test_build_directory_tree() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join("src")).unwrap();
    std::fs::write(dir_path.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(dir_path.join("Cargo.toml"), "[package]").unwrap();

    let tree = project::build_directory_tree(&dir_path, 2).unwrap();
    assert!(tree.contains("src/"));
    assert!(tree.contains("main.rs"));
    assert!(tree.contains("Cargo.toml"));
}

#[test]
fn test_build_directory_tree_skips_hidden() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".hidden")).unwrap();
    std::fs::create_dir(dir_path.join("visible")).unwrap();
    std::fs::write(dir_path.join("visible/file.txt"), "hi").unwrap();

    let tree = project::build_directory_tree(&dir_path, 2).unwrap();
    assert!(!tree.contains(".hidden"));
    assert!(tree.contains("visible/"));
}

#[test]
fn test_build_directory_tree_skips_node_modules() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join("node_modules")).unwrap();
    std::fs::create_dir(dir_path.join("src")).unwrap();
    std::fs::write(dir_path.join("src/index.js"), "").unwrap();

    let tree = project::build_directory_tree(&dir_path, 2).unwrap();
    assert!(!tree.contains("node_modules"));
    assert!(tree.contains("src/"));
}

#[test]
fn test_environment_context_format_prompt_block() {
    let ctx = EnvironmentContext {
        working_dir: "/Users/test/myproject".to_string(),
        git_branch: Some("feature/test".to_string()),
        git_default_branch: Some("main".to_string()),
        git_status: Some("M src/lib.rs\n?? new_file.rs".to_string()),
        git_recent_commits: Some("abc1234 Fix bug\ndef5678 Add feature".to_string()),
        git_remote_url: Some("git@github.com:user/repo.git".to_string()),
        platform: "macos aarch64".to_string(),
        current_date: "2026-03-14".to_string(),
        shell: Some("/bin/zsh".to_string()),
        project_config_files: vec!["Cargo.toml".to_string()],
        tech_stack: vec!["Rust".to_string()],
        directory_tree: Some("project/\n├── src/\n└── Cargo.toml".to_string()),
        instruction_files: vec![],
        model_name: Some("gpt-4o".to_string()),
        memory_content: None,
    };

    let block = ctx.format_prompt_block();
    assert!(block.contains("# Environment"));
    assert!(block.contains("Working directory: /Users/test/myproject"));
    assert!(block.contains("macos aarch64"));
    assert!(block.contains("Rust"));
    assert!(block.contains("# Git Status"));
    assert!(block.contains("feature/test"));
    assert!(block.contains("M src/lib.rs"));
    assert!(block.contains("Fix bug"));
    assert!(block.contains("# Project Structure"));
    assert!(block.contains("Cargo.toml"));
}

#[test]
fn test_environment_context_no_git() {
    let ctx = EnvironmentContext {
        platform: "linux x86_64".to_string(),
        current_date: "2026-03-14".to_string(),
        ..Default::default()
    };

    let block = ctx.format_prompt_block();
    assert!(block.contains("# Environment"));
    assert!(!block.contains("# Git Status"));
}

#[test]
fn test_collect_on_current_dir() {
    // Just verify it doesn't panic
    let ctx = EnvironmentContext::collect(std::path::Path::new("."));
    assert!(!ctx.platform.is_empty());
    assert!(!ctx.current_date.is_empty());
}

// --- Instruction file discovery ---

#[test]
fn test_discover_agents_md() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("AGENTS.md"), "# Rules\nDo X.").unwrap();
    // Add .git so discovery stops here
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].scope, "project");
    assert!(files[0].content.contains("Do X."));
}

#[test]
fn test_discover_claude_md() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("CLAUDE.md"), "# Claude\nBe helpful.").unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("Be helpful."));
}

#[test]
fn test_discover_opendev_instructions() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".opendev")).unwrap();
    std::fs::write(
        dir_path.join(".opendev/instructions.md"),
        "Custom instructions",
    )
    .unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("Custom instructions"));
}

#[test]
fn test_discover_multiple_instruction_files() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("AGENTS.md"), "agents").unwrap();
    std::fs::write(dir_path.join("CLAUDE.md"), "claude").unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert_eq!(files.len(), 2);
}

#[test]
fn test_discover_walks_up_to_git_root() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    // Parent has AGENTS.md and .git
    std::fs::write(dir_path.join("AGENTS.md"), "parent rules").unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();
    // Child subdirectory
    let child = dir_path.join("sub");
    std::fs::create_dir(&child).unwrap();

    let files = discover_instruction_files(&child, &[], &[]);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].scope, "parent");
    assert!(files[0].content.contains("parent rules"));
}

#[test]
fn test_discover_empty_file_skipped() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("AGENTS.md"), "  \n  ").unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert!(files.is_empty());
}

#[test]
fn test_discover_no_duplicates() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("AGENTS.md"), "rules").unwrap();
    // No .git, so it would walk up — but the same file shouldn't appear twice
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert_eq!(files.len(), 1);
}

#[test]
fn test_claude_instructions_dir_not_loaded() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(dir_path.join(".claude")).unwrap();
    std::fs::write(
        dir_path.join(".claude/instructions.md"),
        "Claude-specific instructions",
    )
    .unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    // .claude/instructions.md should not be loaded
    assert_eq!(files.len(), 0);
}

#[test]
fn test_only_opendev_instructions_loaded() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(dir_path.join(".opendev")).unwrap();
    std::fs::create_dir_all(dir_path.join(".claude")).unwrap();
    std::fs::write(dir_path.join(".opendev/instructions.md"), "OpenDev rules").unwrap();
    std::fs::write(dir_path.join(".claude/instructions.md"), "Claude rules").unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("OpenDev rules"));
}

#[test]
fn test_instruction_in_prompt_block() {
    let ctx = EnvironmentContext {
        platform: "test".to_string(),
        current_date: "2026-03-15".to_string(),
        instruction_files: vec![InstructionFile::new(
            "project",
            std::path::PathBuf::from("/project/AGENTS.md"),
            "# Build rules\nRun cargo test.".to_string(),
        )],
        ..Default::default()
    };

    let block = ctx.format_prompt_block();
    assert!(block.contains("# Project Instructions"));
    assert!(block.contains("AGENTS.md (project)"));
    assert!(block.contains("Run cargo test."));
}

#[test]
fn test_resolve_instruction_paths_direct_file() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("CONTRIBUTING.md"), "contrib rules").unwrap();

    let files = resolve_instruction_paths(&["CONTRIBUTING.md".to_string()], &dir_path);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].scope, "config");
    assert!(files[0].content.contains("contrib rules"));
}

#[test]
fn test_resolve_instruction_paths_glob_pattern() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let rules_dir = dir_path.join("rules");
    std::fs::create_dir(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("a.md"), "rule a").unwrap();
    std::fs::write(rules_dir.join("b.md"), "rule b").unwrap();
    std::fs::write(rules_dir.join("c.txt"), "not a markdown").unwrap();

    let files = resolve_instruction_paths(&["rules/*.md".to_string()], &dir_path);
    assert_eq!(files.len(), 2);
    let contents: Vec<&str> = files.iter().map(|f| f.content.as_str()).collect();
    assert!(contents.iter().any(|c| c.contains("rule a")));
    assert!(contents.iter().any(|c| c.contains("rule b")));
}

#[test]
fn test_resolve_instruction_paths_absolute() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("guide.md"), "absolute guide").unwrap();
    let abs_path = dir_path.join("guide.md").to_string_lossy().to_string();

    let files = resolve_instruction_paths(&[abs_path], Path::new("/tmp"));
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("absolute guide"));
}

#[test]
fn test_resolve_instruction_paths_skips_empty() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("empty.md"), "   ").unwrap();

    let files = resolve_instruction_paths(&["empty.md".to_string()], &dir_path);
    assert!(files.is_empty());
}

#[test]
fn test_resolve_instruction_paths_deduplicates() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("rules.md"), "dedup test").unwrap();

    let files =
        resolve_instruction_paths(&["rules.md".to_string(), "rules.md".to_string()], &dir_path);
    assert_eq!(files.len(), 1);
}

#[test]
fn test_resolve_instruction_paths_nonexistent_skipped() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let files = resolve_instruction_paths(&["does_not_exist.md".to_string()], &dir_path);
    assert!(files.is_empty());
}

// ---- Remote URL instructions ----

#[test]
fn test_resolve_instruction_paths_url_invalid_skipped() {
    // An invalid URL should be skipped gracefully.
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let files = resolve_instruction_paths(
        &["https://localhost:1/__nonexistent_path_test__".to_string()],
        &dir_path,
    );
    assert!(files.is_empty());
}

#[test]
fn test_resolve_instruction_paths_url_deduplicates() {
    // Same URL listed twice should produce at most one entry.
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let url = "https://localhost:1/__dup_test__".to_string();
    let files = resolve_instruction_paths(&[url.clone(), url], &dir_path);
    // Both should fail (unreachable host), but even if one succeeded,
    // dedup ensures at most 1.
    assert!(files.len() <= 1);
}

#[test]
fn test_resolve_instruction_paths_mixed_local_and_url() {
    // Local file + unreachable URL: local file should still load.
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::write(dir_path.join("local.md"), "local content").unwrap();

    let files = resolve_instruction_paths(
        &[
            "local.md".to_string(),
            "https://localhost:1/__unreachable__".to_string(),
        ],
        &dir_path,
    );
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("local content"));
    assert_eq!(files[0].scope, "config");
}

#[test]
fn test_fetch_remote_instruction_unreachable() {
    let result = instructions::fetch_remote_instruction("https://localhost:1/__test__");
    assert!(result.is_none());
}

#[test]
fn test_remote_instruction_scope_is_remote() {
    // Verify that if we had a successful fetch, the scope would be "remote".
    // We can't easily test a real URL in unit tests, but we test the function contract:
    // scope for remote files is "remote", path is the URL.
    let file = InstructionFile::new(
        "remote",
        std::path::PathBuf::from("https://example.com/rules.md"),
        "test content".to_string(),
    );
    assert_eq!(file.scope, "remote");
    assert_eq!(file.path.to_string_lossy(), "https://example.com/rules.md");
}

#[test]
fn test_discover_cursorrules() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    // .git to stop traversal
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    // Create .cursorrules file
    std::fs::write(dir_path.join(".cursorrules"), "Use strict TypeScript").unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert!(
        files
            .iter()
            .any(|f| f.content.contains("strict TypeScript")),
        "Should discover .cursorrules: {:?}",
        files
            .iter()
            .map(|f| f.path.display().to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_discover_copilot_instructions() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let github_dir = dir_path.join(".github");
    std::fs::create_dir_all(&github_dir).unwrap();
    std::fs::write(
        github_dir.join("copilot-instructions.md"),
        "Follow conventional commits",
    )
    .unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert!(
        files
            .iter()
            .any(|f| f.content.contains("conventional commits")),
        "Should discover .github/copilot-instructions.md"
    );
}

#[test]
fn test_discover_cursor_rules_directory() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    std::fs::create_dir(dir_path.join(".git")).unwrap();

    // Create .cursor/rules/ with rule files
    let rules_dir = dir_path.join(".cursor").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("security.md"), "Always validate input").unwrap();
    std::fs::write(rules_dir.join("style.md"), "Use 4-space indentation").unwrap();
    // Non-rule file should be ignored
    std::fs::write(rules_dir.join("README"), "Ignore this").unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert!(
        files.iter().any(|f| f.content.contains("validate input")),
        "Should discover .cursor/rules/security.md"
    );
    assert!(
        files.iter().any(|f| f.content.contains("4-space")),
        "Should discover .cursor/rules/style.md"
    );
}

// --- New features ---

#[test]
fn test_discover_opendev_rules_directory() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let rules_dir = dir_path.join(".opendev").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("rust.md"), "Use snake_case").unwrap();
    std::fs::write(rules_dir.join("testing.md"), "Write unit tests").unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert!(
        files.iter().any(|f| f.content.contains("snake_case")),
        "Should discover .opendev/rules/rust.md"
    );
    assert!(
        files.iter().any(|f| f.content.contains("unit tests")),
        "Should discover .opendev/rules/testing.md"
    );
}

#[test]
fn test_discover_opendev_rules_with_frontmatter() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    let rules_dir = dir_path.join(".opendev").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join("rust.md"),
        "---\npaths:\n  - \"src/**/*.rs\"\n---\nUse snake_case",
    )
    .unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    let rust_rule = files.iter().find(|f| f.content.contains("snake_case"));
    assert!(rust_rule.is_some());
    let rust_rule = rust_rule.unwrap();
    assert!(rust_rule.path_globs.is_some());
    assert_eq!(rust_rule.path_globs.as_ref().unwrap()[0], "src/**/*.rs");
    assert_eq!(rust_rule.source, InstructionSource::Rules);
}

#[test]
fn test_discover_local_overrides() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    std::fs::write(dir_path.join("AGENTS.md"), "shared rules").unwrap();
    std::fs::write(dir_path.join("AGENTS.local.md"), "my local overrides").unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert!(
        files.iter().any(|f| f.content.contains("shared rules")),
        "Should discover AGENTS.md"
    );
    assert!(
        files
            .iter()
            .any(|f| f.content.contains("my local overrides")),
        "Should discover AGENTS.local.md"
    );
    let local = files
        .iter()
        .find(|f| f.content.contains("my local overrides"))
        .unwrap();
    assert_eq!(local.scope, "local");
    assert_eq!(local.source, InstructionSource::Local);
}

#[test]
fn test_discover_with_exclusion() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    std::fs::write(dir_path.join("AGENTS.md"), "included").unwrap();
    std::fs::write(dir_path.join("CLAUDE.md"), "excluded").unwrap();

    let files = discover_instruction_files(&dir_path, &["CLAUDE.md".to_string()], &[]);
    assert!(files.iter().any(|f| f.content.contains("included")));
    assert!(
        !files.iter().any(|f| f.content.contains("excluded")),
        "CLAUDE.md should be excluded"
    );
}

#[test]
fn test_discover_additional_dirs() {
    let dir1 = TempDir::new().unwrap();
    let dir1_path = dir1.path().canonicalize().unwrap();
    std::fs::create_dir(dir1_path.join(".git")).unwrap();
    std::fs::write(dir1_path.join("AGENTS.md"), "dir1 rules").unwrap();

    let dir2 = TempDir::new().unwrap();
    let dir2_path = dir2.path().canonicalize().unwrap();
    std::fs::create_dir(dir2_path.join(".git")).unwrap();
    std::fs::write(dir2_path.join("AGENTS.md"), "dir2 rules").unwrap();

    let files = discover_instruction_files(&dir1_path, &[], &[dir2_path]);
    assert!(files.iter().any(|f| f.content.contains("dir1 rules")));
    assert!(files.iter().any(|f| f.content.contains("dir2 rules")));
}

#[test]
fn test_html_comments_stripped_from_discovered_files() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    std::fs::write(
        dir_path.join("AGENTS.md"),
        "Visible content\n<!-- Hidden comment -->\nMore visible",
    )
    .unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    assert_eq!(files.len(), 1);
    assert!(files[0].content.contains("Visible content"));
    assert!(files[0].content.contains("More visible"));
    assert!(!files[0].content.contains("Hidden comment"));
}

#[test]
fn test_includes_processed_in_discovered_files() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    std::fs::create_dir(dir_path.join(".git")).unwrap();

    std::fs::write(dir_path.join("shared.md"), "Shared rules").unwrap();
    std::fs::write(
        dir_path.join("AGENTS.md"),
        "@./shared.md\nMain agents rules",
    )
    .unwrap();

    let files = discover_instruction_files(&dir_path, &[], &[]);
    // Should have both the included file and the AGENTS.md
    assert!(files.iter().any(|f| f.content.contains("Shared rules")));
    assert!(
        files
            .iter()
            .any(|f| f.content.contains("Main agents rules"))
    );
}

#[test]
fn test_conditional_rules_filtered_in_prompt() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();

    let ctx = EnvironmentContext {
        working_dir: dir_path.display().to_string(),
        platform: "test".to_string(),
        current_date: "2026-04-09".to_string(),
        instruction_files: vec![
            InstructionFile {
                scope: "project".to_string(),
                path: std::path::PathBuf::from("unconditional.md"),
                content: "Always shown".to_string(),
                source: InstructionSource::Rules,
                path_globs: None,
                included_from: None,
            },
            InstructionFile {
                scope: "project".to_string(),
                path: std::path::PathBuf::from("rust-only.md"),
                content: "Rust rules".to_string(),
                source: InstructionSource::Rules,
                path_globs: Some(vec!["**/*.rs".to_string()]),
                included_from: None,
            },
        ],
        ..Default::default()
    };

    // Without active files, conditional rule should be excluded
    let block = ctx.format_prompt_block_with_context(&[]);
    assert!(block.contains("Always shown"));
    assert!(!block.contains("Rust rules"));

    // With matching active file, conditional rule should be included
    let rust_file = dir_path.join("src/main.rs");
    std::fs::create_dir_all(rust_file.parent().unwrap()).unwrap();
    std::fs::write(&rust_file, "fn main() {}").unwrap();
    let block = ctx.format_prompt_block_with_context(&[rust_file]);
    assert!(block.contains("Always shown"));
    assert!(block.contains("Rust rules"));
}
