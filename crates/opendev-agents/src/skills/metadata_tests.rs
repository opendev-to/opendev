use super::*;

fn make_metadata(name: &str, namespace: &str, source: SkillSource) -> SkillMetadata {
    SkillMetadata {
        name: name.to_string(),
        description: format!("Use when {name}"),
        namespace: namespace.to_string(),
        path: None,
        source,
        model: None,
        agent: None,
        paths: vec![],
        context: SkillContext::default(),
        effort: SkillEffort::default(),
        allowed_tools: vec![],
        disable_model_invocation: false,
        user_invocable: true,
        hooks: vec![],
    }
}

// --- SkillSource ---

#[test]
fn test_skill_source_display() {
    assert_eq!(SkillSource::Builtin.to_string(), "builtin");
    assert_eq!(SkillSource::UserGlobal.to_string(), "user-global");
    assert_eq!(SkillSource::Project.to_string(), "project");
    assert_eq!(
        SkillSource::Url("https://example.com/skills".to_string()).to_string(),
        "url:https://example.com/skills"
    );
}

#[test]
fn test_skill_source_equality() {
    assert_eq!(SkillSource::Builtin, SkillSource::Builtin);
    assert_ne!(SkillSource::Builtin, SkillSource::Project);
    assert_eq!(
        SkillSource::Url("a".to_string()),
        SkillSource::Url("a".to_string())
    );
    assert_ne!(
        SkillSource::Url("a".to_string()),
        SkillSource::Url("b".to_string())
    );
}

// --- SkillMetadata::full_name ---

#[test]
fn test_full_name_default_namespace() {
    let m = make_metadata("commit", "default", SkillSource::Builtin);
    assert_eq!(m.full_name(), "commit");
}

#[test]
fn test_full_name_custom_namespace() {
    let m = make_metadata("deploy", "devops", SkillSource::Project);
    assert_eq!(m.full_name(), "devops:deploy");
}

#[test]
fn test_full_name_empty_namespace_is_not_default() {
    let m = make_metadata("test", "", SkillSource::Builtin);
    // Empty string != "default", so it should prefix
    assert_eq!(m.full_name(), ":test");
}

// --- SkillMetadata::estimate_tokens ---

#[test]
fn test_estimate_tokens_no_path() {
    let m = make_metadata("commit", "default", SkillSource::Builtin);
    assert_eq!(m.estimate_tokens(), None);
}

#[test]
fn test_estimate_tokens_missing_file() {
    let mut m = make_metadata("commit", "default", SkillSource::Project);
    m.path = Some(PathBuf::from("/nonexistent/skill.md"));
    assert_eq!(m.estimate_tokens(), None);
}

#[test]
fn test_estimate_tokens_real_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("skill.md");
    // 400 chars → ~100 tokens
    std::fs::write(&file, "x".repeat(400)).unwrap();

    let mut m = make_metadata("test", "default", SkillSource::Project);
    m.path = Some(file);
    assert_eq!(m.estimate_tokens(), Some(100));
}

// --- LoadedSkill ---

#[test]
fn test_loaded_skill_estimate_tokens() {
    let skill = LoadedSkill {
        metadata: make_metadata("commit", "default", SkillSource::Builtin),
        content: "a".repeat(200),
        companion_files: vec![],
        cached_mtime: None,
    };
    assert_eq!(skill.estimate_tokens(), 50);
}

#[test]
fn test_loaded_skill_estimate_tokens_empty() {
    let skill = LoadedSkill {
        metadata: make_metadata("empty", "default", SkillSource::Builtin),
        content: String::new(),
        companion_files: vec![],
        cached_mtime: None,
    };
    assert_eq!(skill.estimate_tokens(), 0);
}

#[test]
fn test_loaded_skill_estimate_tokens_small() {
    let skill = LoadedSkill {
        metadata: make_metadata("small", "default", SkillSource::Builtin),
        content: "hi".to_string(), // 2 chars → 0 tokens (integer division)
        companion_files: vec![],
        cached_mtime: None,
    };
    assert_eq!(skill.estimate_tokens(), 0);
}
