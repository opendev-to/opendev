use super::*;
use std::fs;

fn setup_templates(dir: &std::path::Path) {
    let main_dir = dir.join("system/main");
    fs::create_dir_all(&main_dir).unwrap();

    fs::write(main_dir.join("section-a.md"), "# Section A\nContent A").unwrap();
    fs::write(main_dir.join("section-b.md"), "# Section B\nContent B").unwrap();
    fs::write(
        main_dir.join("section-c.md"),
        "<!-- frontmatter: true -->\n# Section C\nContent C",
    )
    .unwrap();
    fs::write(main_dir.join("section-d.md"), "# Dynamic\nDynamic content").unwrap();
}

#[test]
fn test_compose_basic() {
    let dir = tempfile::TempDir::new().unwrap();
    setup_templates(dir.path());

    let mut composer = PromptComposer::new(dir.path());
    composer.register_section("a", "system/main/section-a.md", None, 10, true);
    composer.register_section("b", "system/main/section-b.md", None, 20, true);

    let result = composer.compose(&HashMap::new());
    assert!(result.contains("Content A"));
    assert!(result.contains("Content B"));
    // A should come before B (lower priority)
    assert!(result.find("Content A") < result.find("Content B"));
}

#[test]
fn test_compose_priority_ordering() {
    let dir = tempfile::TempDir::new().unwrap();
    setup_templates(dir.path());

    let mut composer = PromptComposer::new(dir.path());
    // Register in reverse order
    composer.register_section("b", "system/main/section-b.md", None, 20, true);
    composer.register_section("a", "system/main/section-a.md", None, 10, true);

    let result = composer.compose(&HashMap::new());
    assert!(result.find("Content A") < result.find("Content B"));
}

#[test]
fn test_compose_with_condition() {
    let dir = tempfile::TempDir::new().unwrap();
    setup_templates(dir.path());

    let mut composer = PromptComposer::new(dir.path());
    composer.register_section("a", "system/main/section-a.md", None, 10, true);
    composer.register_section(
        "b",
        "system/main/section-b.md",
        Some(ctx_bool("show_b")),
        20,
        true,
    );

    // Without condition met
    let result = composer.compose(&HashMap::new());
    assert!(result.contains("Content A"));
    assert!(!result.contains("Content B"));

    // With condition met
    let mut ctx = HashMap::new();
    ctx.insert("show_b".to_string(), serde_json::json!(true));
    let result = composer.compose(&ctx);
    assert!(result.contains("Content A"));
    assert!(result.contains("Content B"));
}

#[test]
fn test_compose_strips_frontmatter() {
    let dir = tempfile::TempDir::new().unwrap();
    setup_templates(dir.path());

    let mut composer = PromptComposer::new(dir.path());
    composer.register_section("c", "system/main/section-c.md", None, 10, true);

    let result = composer.compose(&HashMap::new());
    assert!(!result.contains("frontmatter"));
    assert!(result.contains("Content C"));
}

#[test]
fn test_compose_two_part() {
    let dir = tempfile::TempDir::new().unwrap();
    setup_templates(dir.path());

    let mut composer = PromptComposer::new(dir.path());
    composer.register_section("a", "system/main/section-a.md", None, 10, true);
    composer.register_section("d", "system/main/section-d.md", None, 20, false);

    let (stable, dynamic) = composer.compose_two_part(&HashMap::new());
    assert!(stable.contains("Content A"));
    assert!(!stable.contains("Dynamic content"));
    assert!(dynamic.contains("Dynamic content"));
    assert!(!dynamic.contains("Content A"));
}

#[test]
fn test_compose_missing_file() {
    let dir = tempfile::TempDir::new().unwrap();

    let mut composer = PromptComposer::new(dir.path());
    composer.register_section("missing", "nonexistent.md", None, 10, true);

    let result = composer.compose(&HashMap::new());
    assert!(result.is_empty());
}

#[test]
fn test_strip_frontmatter() {
    assert_eq!(
        strip_frontmatter("<!-- key: value -->\n# Title\nContent"),
        "# Title\nContent"
    );
    assert_eq!(strip_frontmatter("No frontmatter"), "No frontmatter");
    assert_eq!(strip_frontmatter(""), "");
}

#[test]
fn test_section_count_and_names() {
    let composer_dir = tempfile::TempDir::new().unwrap();
    let mut composer = PromptComposer::new(composer_dir.path());
    composer.register_simple("alpha", "alpha.md");
    composer.register_simple("beta", "beta.md");

    assert_eq!(composer.section_count(), 2);
    let names = composer.section_names();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

#[test]
fn test_substitute_variables_basic() {
    let mut vars = HashMap::new();
    vars.insert("name".to_string(), "world".to_string());
    assert_eq!(
        substitute_variables("Hello {{name}}!", &vars),
        "Hello world!"
    );
}

#[test]
fn test_substitute_variables_multiple() {
    let mut vars = HashMap::new();
    vars.insert("session_id".to_string(), "abc-123".to_string());
    vars.insert("path".to_string(), "/home/user".to_string());

    let template = "Session {{session_id}} at {{path}}";
    assert_eq!(
        substitute_variables(template, &vars),
        "Session abc-123 at /home/user"
    );
}

#[test]
fn test_substitute_variables_missing_left_as_is() {
    let vars = HashMap::new();
    assert_eq!(
        substitute_variables("Hello {{unknown}}!", &vars),
        "Hello {{unknown}}!"
    );
}

#[test]
fn test_substitute_variables_no_placeholders() {
    let vars = HashMap::new();
    assert_eq!(substitute_variables("No vars here", &vars), "No vars here");
}

#[test]
fn test_compose_with_vars() {
    let dir = tempfile::TempDir::new().unwrap();
    let main_dir = dir.path().join("system/main");
    fs::create_dir_all(&main_dir).unwrap();
    fs::write(
        main_dir.join("template.md"),
        "Session: {{session_id}}\nPath: {{path}}",
    )
    .unwrap();

    let mut composer = PromptComposer::new(dir.path());
    composer.register_section("t", "system/main/template.md", None, 10, true);

    let mut vars = HashMap::new();
    vars.insert("session_id".to_string(), "xyz-789".to_string());
    vars.insert("path".to_string(), "/workspace".to_string());

    let result = composer.compose_with_vars(&HashMap::new(), &vars);
    assert!(result.contains("Session: xyz-789"));
    assert!(result.contains("Path: /workspace"));
}

#[test]
fn test_compose_two_part_with_vars() {
    let dir = tempfile::TempDir::new().unwrap();
    let main_dir = dir.path().join("test");
    fs::create_dir_all(&main_dir).unwrap();
    fs::write(main_dir.join("stable.md"), "Stable {{key}}").unwrap();
    fs::write(main_dir.join("dynamic.md"), "Dynamic {{key}}").unwrap();

    let mut composer = PromptComposer::new(dir.path());
    composer.register_section("s", "test/stable.md", None, 10, true);
    composer.register_section("d", "test/dynamic.md", None, 20, false);

    let mut vars = HashMap::new();
    vars.insert("key".to_string(), "value".to_string());

    let (stable, dynamic) = composer.compose_two_part_with_vars(&HashMap::new(), &vars);
    assert_eq!(stable, "Stable value");
    assert_eq!(dynamic, "Dynamic value");
}

#[test]
fn test_embedded_templates_used_by_default_composer() {
    // Use a temp dir that has NO files — embedded should still resolve
    let dir = tempfile::TempDir::new().unwrap();
    let composer = create_default_composer(dir.path());

    // Compose without any conditions to get the always-included sections
    let result = composer.compose(&HashMap::new());

    // The security policy section is always included (no condition) and should
    // come from embedded templates even though the filesystem dir is empty.
    assert!(
        result.contains("Security Policy"),
        "Expected embedded security policy template"
    );
}
