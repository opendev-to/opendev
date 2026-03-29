use super::*;
use tempfile::TempDir;

#[test]
fn test_parse_skill_basic() {
    let content = "# My Skill\n\nSome description.\n\n## Steps\n\n1. Do this\n2. Do that\n";
    let skill = parse_skill(content, "fallback").unwrap();
    assert_eq!(skill.name, "My Skill");
    assert!(skill.sections.contains_key("My Skill"));
    assert!(skill.sections.contains_key("Steps"));
}

#[test]
fn test_parse_skill_no_heading() {
    let content = "Just some text without headings.\n";
    let skill = parse_skill(content, "fallback-name").unwrap();
    assert_eq!(skill.name, "fallback-name");
}

#[test]
fn test_parse_skill_empty() {
    let result = parse_skill("", "fallback");
    assert!(result.is_err());
}

#[test]
fn test_parse_skill_whitespace_only() {
    let result = parse_skill("   \n\n  ", "fallback");
    assert!(result.is_err());
}

#[test]
fn test_parse_skill_multiple_sections() {
    let content = "# Skill\n\nIntro.\n\n## Setup\n\nSetup steps.\n\n## Usage\n\nUsage info.\n";
    let skill = parse_skill(content, "fb").unwrap();
    assert_eq!(skill.name, "Skill");
    assert_eq!(skill.sections.len(), 3);
    assert!(skill.sections.contains_key("Skill"));
    assert!(skill.sections.contains_key("Setup"));
    assert!(skill.sections.contains_key("Usage"));
}

#[test]
fn test_url_to_cache_filename() {
    let f1 = url_to_cache_filename("https://example.com/skill.md");
    let f2 = url_to_cache_filename("https://example.com/other.md");
    assert_ne!(f1, f2);
    assert!(f1.ends_with(".md"));
    assert_eq!(f1.len(), 19); // 16 hex chars + ".md"
}

#[test]
fn test_url_to_cache_filename_deterministic() {
    let f1 = url_to_cache_filename("https://example.com/skill.md");
    let f2 = url_to_cache_filename("https://example.com/skill.md");
    assert_eq!(f1, f2);
}

#[test]
fn test_extract_name_from_url() {
    assert_eq!(
        extract_name_from_url("https://example.com/my-skill.md"),
        "my skill"
    );
    assert_eq!(
        extract_name_from_url("https://example.com/repo/commit_helper.md"),
        "commit helper"
    );
}

#[test]
fn test_load_skill_insecure_url() {
    let result = load_skill_from_url("http://example.com/skill.md");
    assert!(result.is_err());
    match result.unwrap_err() {
        SkillError::InsecureUrl(_) => {}
        other => panic!("expected InsecureUrl, got: {other}"),
    }
}

#[test]
fn test_load_skill_invalid_url() {
    let result = load_skill_from_url("https://x");
    assert!(result.is_err());
}

#[test]
fn test_load_skill_from_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test-skill.md");
    std::fs::write(
        &path,
        "# Test Skill\n\nA test skill.\n\n## Steps\n\n1. Step one\n",
    )
    .unwrap();

    let skill = load_skill_from_file(&path).unwrap();
    assert_eq!(skill.name, "Test Skill");
    assert!(skill.sections.contains_key("Steps"));
    assert!(skill.cache_path.is_some());
}

#[test]
fn test_load_skill_from_file_missing() {
    let result = load_skill_from_file(Path::new("/nonexistent/skill.md"));
    assert!(result.is_err());
}

#[test]
fn test_load_from_url_cached() {
    let dir = TempDir::new().unwrap();
    let url = "https://example.com/cached-skill.md";
    let cache_filename = url_to_cache_filename(url);
    let cache_path = dir.path().join(&cache_filename);

    // Pre-populate cache
    std::fs::write(&cache_path, "# Cached Skill\n\nFrom cache.\n").unwrap();

    let skill = load_skill_from_url_with_options(url, Some(dir.path()), false).unwrap();
    assert_eq!(skill.name, "Cached Skill");
    assert_eq!(skill.source_url.as_deref(), Some(url));
}

#[test]
fn test_list_cached_skills() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.md"), "# A").unwrap();
    std::fs::write(dir.path().join("b.md"), "# B").unwrap();
    std::fs::write(dir.path().join("c.txt"), "not a skill").unwrap();

    let skills = list_cached_skills(Some(dir.path()));
    assert_eq!(skills.len(), 2);
}

#[test]
fn test_list_cached_skills_empty() {
    let dir = TempDir::new().unwrap();
    let skills = list_cached_skills(Some(dir.path()));
    assert!(skills.is_empty());
}

#[test]
fn test_list_cached_skills_missing_dir() {
    let skills = list_cached_skills(Some(Path::new("/nonexistent/dir")));
    assert!(skills.is_empty());
}
