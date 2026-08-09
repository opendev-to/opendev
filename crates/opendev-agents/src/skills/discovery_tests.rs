use super::super::metadata::SkillMetadata;
use super::*;

// ---- URL fetching ----

#[test]
fn test_fetch_url_invalid_command() {
    // Unreachable URL should return error
    let result = fetch_url("https://192.0.2.1/nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_pull_url_skills_invalid_url() {
    let result = pull_url_skills("https://192.0.2.1/nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("curl failed"));
}

#[test]
fn test_skill_source_url_display() {
    let source = SkillSource::Url("https://example.com/skills".to_string());
    assert_eq!(source.to_string(), "url:https://example.com/skills");
}

// ---- Cache invalidation via mtime ----

#[test]
fn test_is_cache_stale_builtin_never_stale() {
    let skill = LoadedSkill {
        metadata: SkillMetadata {
            name: "commit".to_string(),
            description: "Builtin commit".to_string(),
            namespace: "default".to_string(),
            path: None,
            source: SkillSource::Builtin,
            model: None,
            agent: None,
            paths: vec![],
            context: super::super::metadata::SkillContext::default(),
            effort: super::super::metadata::SkillEffort::default(),
            allowed_tools: vec![],
            disable_model_invocation: false,
            user_invocable: true,
            hooks: vec![],
        },
        content: "content".to_string(),
        companion_files: vec![],
        cached_mtime: None,
    };
    assert!(!is_cache_stale(&skill));
}

#[test]
fn test_is_cache_stale_no_mtime_not_stale() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("skill.md");
    std::fs::write(&file, "---\nname: test\ndescription: t\n---\ncontent").unwrap();

    let skill = LoadedSkill {
        metadata: SkillMetadata {
            name: "test".to_string(),
            description: "t".to_string(),
            namespace: "default".to_string(),
            path: Some(file),
            source: SkillSource::Project,
            model: None,
            agent: None,
            paths: vec![],
            context: super::super::metadata::SkillContext::default(),
            effort: super::super::metadata::SkillEffort::default(),
            allowed_tools: vec![],
            disable_model_invocation: false,
            user_invocable: true,
            hooks: vec![],
        },
        content: "content".to_string(),
        companion_files: vec![],
        cached_mtime: None, // No mtime recorded
    };
    assert!(!is_cache_stale(&skill));
}

#[test]
fn test_is_cache_stale_unmodified_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("skill.md");
    std::fs::write(&file, "---\nname: test\ndescription: t\n---\ncontent").unwrap();

    let mtime = std::fs::metadata(&file).unwrap().modified().unwrap();

    let skill = LoadedSkill {
        metadata: SkillMetadata {
            name: "test".to_string(),
            description: "t".to_string(),
            namespace: "default".to_string(),
            path: Some(file),
            source: SkillSource::Project,
            model: None,
            agent: None,
            paths: vec![],
            context: super::super::metadata::SkillContext::default(),
            effort: super::super::metadata::SkillEffort::default(),
            allowed_tools: vec![],
            disable_model_invocation: false,
            user_invocable: true,
            hooks: vec![],
        },
        content: "content".to_string(),
        companion_files: vec![],
        cached_mtime: Some(mtime),
    };
    assert!(!is_cache_stale(&skill));
}

#[test]
fn test_is_cache_stale_modified_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("skill.md");
    std::fs::write(&file, "---\nname: test\ndescription: t\n---\noriginal").unwrap();

    // Record an old mtime (1 second in the past).
    let old_mtime = std::time::SystemTime::now() - std::time::Duration::from_secs(2);

    let skill = LoadedSkill {
        metadata: SkillMetadata {
            name: "test".to_string(),
            description: "t".to_string(),
            namespace: "default".to_string(),
            path: Some(file),
            source: SkillSource::Project,
            model: None,
            agent: None,
            paths: vec![],
            context: super::super::metadata::SkillContext::default(),
            effort: super::super::metadata::SkillEffort::default(),
            allowed_tools: vec![],
            disable_model_invocation: false,
            user_invocable: true,
            hooks: vec![],
        },
        content: "original".to_string(),
        companion_files: vec![],
        cached_mtime: Some(old_mtime),
    };

    // File was written "now", cached mtime is 2s in the past → stale.
    assert!(is_cache_stale(&skill));
}

#[test]
fn test_is_cache_stale_deleted_file() {
    let skill = LoadedSkill {
        metadata: SkillMetadata {
            name: "gone".to_string(),
            description: "t".to_string(),
            namespace: "default".to_string(),
            path: Some(std::path::PathBuf::from("/nonexistent/skill.md")),
            source: SkillSource::Project,
            model: None,
            agent: None,
            paths: vec![],
            context: super::super::metadata::SkillContext::default(),
            effort: super::super::metadata::SkillEffort::default(),
            allowed_tools: vec![],
            disable_model_invocation: false,
            user_invocable: true,
            hooks: vec![],
        },
        content: "content".to_string(),
        companion_files: vec![],
        cached_mtime: Some(std::time::SystemTime::now()),
    };
    // File doesn't exist → not stale (keep cache).
    assert!(!is_cache_stale(&skill));
}
