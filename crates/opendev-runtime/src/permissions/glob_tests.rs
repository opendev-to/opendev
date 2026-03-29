use super::*;

#[test]
fn test_glob_exact_match() {
    assert!(glob_matches("hello", "hello"));
    assert!(!glob_matches("hello", "world"));
}

#[test]
fn test_glob_star() {
    assert!(glob_matches("bash:*", "bash:ls -la"));
    assert!(glob_matches("edit:*", "edit:foo.rs"));
    assert!(!glob_matches("bash:*", "edit:foo.rs"));
}

#[test]
fn test_glob_double_star() {
    assert!(glob_matches("src/**", "src/foo/bar/baz.rs"));
    assert!(glob_matches("**/*.rs", "src/foo/bar/baz.rs"));
    assert!(!glob_matches("src/**", "vendor/foo.rs"));
}

#[test]
fn test_glob_question_mark() {
    assert!(glob_matches("ba?h:*", "bash:cmd"));
    assert!(!glob_matches("ba?h:*", "batch:cmd"));
}

#[test]
fn test_glob_star_matches_slash_in_permission_mode() {
    // In permission mode, `*` matches any char including `/`
    assert!(glob_matches("bash:*", "bash:cat /etc/passwd"));
    assert!(glob_matches("edit:*", "edit:src/foo/bar.rs"));
}

#[test]
fn test_glob_path_star_no_slash() {
    // In path mode, single `*` should not match `/`
    assert!(!glob_matches_path("src/*", "src/foo/bar.rs"));
    assert!(glob_matches_path("src/*", "src/bar.rs"));
}
