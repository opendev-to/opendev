use super::*;

fn home() -> String {
    dirs::home_dir().unwrap().to_string_lossy().into_owned()
}

#[test]
fn test_shorten_relative_to_working_dir() {
    let home = home();
    let ps = PathShortener::new(Some(&format!("{home}/project")));
    assert_eq!(
        ps.shorten(&format!("{home}/project/src/main.rs")),
        "src/main.rs"
    );
}

#[test]
fn test_shorten_working_dir_itself() {
    let home = home();
    let ps = PathShortener::new(Some(&format!("{home}/project")));
    assert_eq!(ps.shorten(&format!("{home}/project")), ".");
}

#[test]
fn test_shorten_outside_working_dir_uses_tilde() {
    let home = home();
    let ps = PathShortener::new(Some(&format!("{home}/project")));
    assert_eq!(
        ps.shorten(&format!("{home}/other/src/main.rs")),
        "~/other/src/main.rs"
    );
}

#[test]
fn test_shorten_home_dir_itself() {
    let home = home();
    let ps = PathShortener::new(Some("/some/other/dir"));
    assert_eq!(ps.shorten(&home), "~");
}

#[test]
fn test_shorten_strips_dot_slash() {
    let ps = PathShortener::new(Some("/project"));
    assert_eq!(ps.shorten("./src/main.rs"), "src/main.rs");
}

#[test]
fn test_shorten_text_replaces_wd() {
    let home = home();
    let ps = PathShortener::new(Some(&format!("{home}/project")));
    let text = format!("Explore repo at {home}/project/src with focus on tests");
    assert_eq!(
        ps.shorten_text(&text),
        "Explore repo at src with focus on tests"
    );
}

#[test]
fn test_shorten_text_standalone_wd() {
    let home = home();
    let ps = PathShortener::new(Some(&format!("{home}/project")));
    let text = format!("List({home}/project)");
    assert_eq!(ps.shorten_text(&text), "List(.)");
}

#[test]
fn test_shorten_text_boundary_safety() {
    let ps = PathShortener::new(Some("/project"));
    let text = "Explore /project-v2/src";
    assert_eq!(ps.shorten_text(text), "Explore /project-v2/src");
}

#[test]
fn test_shorten_text_home_fallback() {
    let home = home();
    let ps = PathShortener::new(Some(&format!("{home}/my-project")));
    let text = format!("List({home}/other-project)");
    assert_eq!(ps.shorten_text(&text), "List(~/other-project)");
}

#[test]
fn test_shorten_text_no_working_dir() {
    let home = home();
    let ps = PathShortener::new(None);
    let text = format!("some text {home}/project/file.rs");
    assert_eq!(ps.shorten_text(&text), "some text ~/project/file.rs");
}

#[test]
fn test_shorten_display_short_path() {
    let ps = PathShortener::default();
    assert_eq!(ps.shorten_display("/home/user"), "/home/user");
}

#[test]
fn test_shorten_display_long_non_home_path() {
    let ps = PathShortener::default();
    assert_eq!(ps.shorten_display("/a/b/c/d/myapp"), "…/d/myapp");
}

#[test]
fn test_shorten_display_home_short() {
    let home = home();
    let ps = PathShortener::default();
    // ~/codes/opendev → stays as-is (≤3 parts after ~)
    let result = ps.shorten_display(&format!("{home}/codes/opendev"));
    assert_eq!(result, "~/codes/opendev");
}

#[test]
fn test_shorten_display_home_long() {
    let home = home();
    let ps = PathShortener::default();
    // ~/a/b/c/d → ~/…/c/d
    let result = ps.shorten_display(&format!("{home}/a/b/c/d"));
    assert_eq!(result, "~/…/c/d");
}

#[test]
fn test_default_no_working_dir() {
    let ps = PathShortener::default();
    assert!(ps.working_dir.is_none());
    assert!(ps.home_dir.is_some());
}
