use super::*;

#[test]
fn test_new_defaults() {
    let ctrl = SkillCreatorController::new();
    assert_eq!(ctrl.name(), "");
    assert_eq!(ctrl.description(), "");
    assert_eq!(ctrl.content(), "");
    assert!(ctrl.is_user_invocable());
}

#[test]
fn test_set_fields() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("review-pr");
    ctrl.set_description("Review a pull request");
    ctrl.set_content("You are a code reviewer.\nBe thorough.");
    ctrl.set_user_invocable(false);

    assert_eq!(ctrl.name(), "review-pr");
    assert_eq!(ctrl.description(), "Review a pull request");
    assert_eq!(ctrl.content(), "You are a code reviewer.\nBe thorough.");
    assert!(!ctrl.is_user_invocable());
}

#[test]
fn test_validate_success() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("commit");
    ctrl.set_description("Create a git commit");
    ctrl.set_content("Analyze staged changes and create a commit.");

    let spec = ctrl.validate().unwrap();
    assert_eq!(spec.name, "commit");
    assert_eq!(spec.description, "Create a git commit");
    assert_eq!(spec.content, "Analyze staged changes and create a commit.");
    assert!(spec.is_user_invocable);
}

#[test]
fn test_validate_not_invocable() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("internal");
    ctrl.set_description("Internal skill");
    ctrl.set_content("content");
    ctrl.set_user_invocable(false);

    let spec = ctrl.validate().unwrap();
    assert!(!spec.is_user_invocable);
}

#[test]
fn test_validate_missing_name() {
    let ctrl = SkillCreatorController::new();
    let err = ctrl.validate().unwrap_err();
    assert!(err.contains("name"), "Error should mention name: {err}");
}

#[test]
fn test_validate_missing_description() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("skill");
    let err = ctrl.validate().unwrap_err();
    assert!(
        err.contains("description"),
        "Error should mention description: {err}"
    );
}

#[test]
fn test_validate_missing_content() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("skill");
    ctrl.set_description("desc");
    let err = ctrl.validate().unwrap_err();
    assert!(
        err.contains("content"),
        "Error should mention content: {err}"
    );
}

#[test]
fn test_validate_trims_whitespace() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("  skill  ");
    ctrl.set_description("  desc  ");
    ctrl.set_content("content");

    let spec = ctrl.validate().unwrap();
    assert_eq!(spec.name, "skill");
    assert_eq!(spec.description, "desc");
}

#[test]
fn test_validate_whitespace_only_is_invalid() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("   ");
    assert!(ctrl.validate().is_err());
}

#[test]
fn test_reset() {
    let mut ctrl = SkillCreatorController::new();
    ctrl.set_name("skill");
    ctrl.set_description("desc");
    ctrl.set_content("content");
    ctrl.set_user_invocable(false);

    ctrl.reset();

    assert_eq!(ctrl.name(), "");
    assert_eq!(ctrl.description(), "");
    assert_eq!(ctrl.content(), "");
    assert!(ctrl.is_user_invocable());
}

#[test]
fn test_default_trait() {
    let ctrl = SkillCreatorController::default();
    assert_eq!(ctrl.name(), "");
    assert!(ctrl.is_user_invocable());
}
