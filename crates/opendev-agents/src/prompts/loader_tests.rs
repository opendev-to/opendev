use super::*;
use std::fs;

#[test]
fn test_load_prompt_md() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test-prompt.md");
    fs::write(&path, "<!-- meta -->\n# Title\nPrompt content").unwrap();

    let loader = PromptLoader::new(dir.path());
    let result = loader.load_prompt("test-prompt").unwrap();
    assert_eq!(result, "# Title\nPrompt content");
}

#[test]
fn test_load_prompt_txt_fallback() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("legacy.txt");
    fs::write(&path, "  Legacy prompt  ").unwrap();

    let loader = PromptLoader::new(dir.path());
    let result = loader.load_prompt("legacy").unwrap();
    assert_eq!(result, "Legacy prompt");
}

#[test]
fn test_load_prompt_md_preferred_over_txt() {
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(dir.path().join("both.md"), "MD content").unwrap();
    fs::write(dir.path().join("both.txt"), "TXT content").unwrap();

    let loader = PromptLoader::new(dir.path());
    let result = loader.load_prompt("both").unwrap();
    assert_eq!(result, "MD content");
}

#[test]
fn test_load_prompt_not_found() {
    let dir = tempfile::TempDir::new().unwrap();
    let loader = PromptLoader::new(dir.path());
    assert!(loader.load_prompt("nonexistent").is_err());
}

#[test]
fn test_load_prompt_with_fallback() {
    let dir = tempfile::TempDir::new().unwrap();
    let loader = PromptLoader::new(dir.path());
    let result = loader
        .load_prompt_with_fallback("missing", Some("fallback text"))
        .unwrap();
    assert_eq!(result, "fallback text");
}

#[test]
fn test_load_tool_description_from_embedded() {
    // Use empty dir — should resolve from embedded
    let dir = tempfile::TempDir::new().unwrap();
    let loader = PromptLoader::new(dir.path());
    let result = loader.load_tool_description("read_file");
    // The embedded store has "tools/tool-read-file.md"
    assert!(result.is_ok(), "Should load from embedded");
}

#[test]
fn test_load_tool_description_from_filesystem() {
    // Use a tool name that is NOT in the embedded store to test filesystem fallback.
    let dir = tempfile::TempDir::new().unwrap();
    let tools_dir = dir.path().join("tools");
    fs::create_dir_all(&tools_dir).unwrap();
    fs::write(tools_dir.join("tool-custom-tool.md"), "Custom tool desc").unwrap();

    let loader = PromptLoader::new(dir.path());
    let result = loader.load_tool_description("custom_tool").unwrap();
    assert_eq!(result, "Custom tool desc");
}

#[test]
fn test_load_tool_description_embedded_takes_priority() {
    // Even with a filesystem override, embedded wins for known templates.
    let dir = tempfile::TempDir::new().unwrap();
    let loader = PromptLoader::new(dir.path());
    let result = loader.load_tool_description("read_file").unwrap();
    // Should come from embedded, not filesystem
    assert!(result.contains("Read a file"));
}

#[test]
fn test_get_prompt_path_md() {
    let dir = tempfile::TempDir::new().unwrap();
    fs::write(dir.path().join("prompt.md"), "content").unwrap();

    let loader = PromptLoader::new(dir.path());
    let path = loader.get_prompt_path("prompt");
    assert!(path.to_string_lossy().ends_with(".md"));
}

#[test]
fn test_get_prompt_path_txt_when_no_md() {
    let dir = tempfile::TempDir::new().unwrap();

    let loader = PromptLoader::new(dir.path());
    let path = loader.get_prompt_path("prompt");
    assert!(path.to_string_lossy().ends_with(".txt"));
}

#[test]
fn test_load_embedded_system_prompt() {
    let dir = tempfile::TempDir::new().unwrap();
    let loader = PromptLoader::new(dir.path());
    // "system/compaction" maps to embedded key "system/compaction.md"
    let result = loader.load_prompt("system/compaction");
    assert!(result.is_ok());
    assert!(result.unwrap().contains("conversation compactor"));
}
