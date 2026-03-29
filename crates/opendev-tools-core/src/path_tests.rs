use super::*;
use std::fs;
use tempfile::TempDir;

// ---- expand_home ----

#[test]
fn test_expand_home_tilde_prefix() {
    let result = expand_home("~/projects/foo");
    assert!(!result.starts_with("~"));
    assert!(result.ends_with("/projects/foo"));
}

#[cfg(unix)]
#[test]
fn test_expand_home_tilde_only() {
    let result = expand_home("~");
    assert!(!result.starts_with("~"));
    assert!(result.starts_with('/'));
}

#[test]
fn test_expand_home_dollar_home_prefix() {
    let result = expand_home("$HOME/Documents");
    assert!(!result.starts_with("$HOME"));
    assert!(result.ends_with("/Documents"));
}

#[cfg(unix)]
#[test]
fn test_expand_home_dollar_home_only() {
    let result = expand_home("$HOME");
    assert!(!result.contains("$HOME"));
    assert!(result.starts_with('/'));
}

#[test]
fn test_expand_home_no_expansion() {
    assert_eq!(expand_home("/absolute/path"), "/absolute/path");
    assert_eq!(expand_home("relative/path"), "relative/path");
    assert_eq!(expand_home("~not-a-tilde"), "~not-a-tilde");
}

// ---- normalize_path ----

#[test]
fn test_normalize_path_collapses_dotdot() {
    let result = normalize_path(Path::new("/home/user/project/../../../etc/passwd"));
    assert_eq!(result, PathBuf::from("/etc/passwd"));
}

#[test]
fn test_normalize_path_collapses_dot() {
    let result = normalize_path(Path::new("/home/user/./project/./src"));
    assert_eq!(result, PathBuf::from("/home/user/project/src"));
}

#[test]
fn test_normalize_path_preserves_root() {
    let result = normalize_path(Path::new("/../../etc"));
    assert_eq!(result, PathBuf::from("/etc"));
}

// ---- resolve_file_path ----

#[test]
fn test_resolve_file_path_redundant_prefix() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let result = resolve_file_path("myproject/main.rs", &project);
    assert_eq!(result, project.join("main.rs"));
}

#[test]
fn test_resolve_file_path_valid_relative() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), "").unwrap();

    let result = resolve_file_path("src/lib.rs", &project);
    assert_eq!(result, project.join("src/lib.rs"));
}

#[test]
fn test_resolve_file_path_absolute_redundant() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("main.rs"), "").unwrap();

    let wrong_path = project.join("myproject").join("main.rs");
    let result = resolve_file_path(wrong_path.to_str().unwrap(), &project);
    assert_eq!(result, project.join("main.rs"));
}

#[test]
fn test_resolve_file_path_dot_slash_redundant() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("main.rs"), "fn main() {}").unwrap();

    let result = resolve_file_path("./myproject/main.rs", &project);
    assert_eq!(result, project.join("main.rs"));
}

#[test]
fn test_resolve_file_path_dot_slash_valid() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), "").unwrap();

    let result = resolve_file_path("./src/lib.rs", &project);
    assert_eq!(result, project.join("src/lib.rs"));
}

#[test]
fn test_resolve_file_path_tilde_redundant() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();

    let result = resolve_file_path("~/nonexistent_dir_xyz/file.rs", &project);
    assert!(
        !result.to_string_lossy().contains('~'),
        "tilde should be expanded: {result:?}"
    );
}

// ---- resolve_dir_path ----

#[test]
fn test_resolve_dir_path_redundant_basename() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();

    let result = resolve_dir_path("myproject", &project);
    assert_eq!(result, project);
}

#[test]
fn test_resolve_dir_path_valid_subdir() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let subdir = project.join("src");
    fs::create_dir_all(&subdir).unwrap();

    let result = resolve_dir_path("src", &project);
    assert_eq!(result, subdir);
}

#[test]
fn test_resolve_dir_path_absolute_existing() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("existing");
    fs::create_dir(&dir).unwrap();

    let result = resolve_dir_path(dir.to_str().unwrap(), tmp.path());
    assert_eq!(result, dir);
}

#[test]
fn test_resolve_dir_path_no_path() {
    let tmp = TempDir::new().unwrap();
    let result = resolve_dir_path("nonexistent", tmp.path());
    assert_eq!(result, tmp.path().join("nonexistent"));
}

#[test]
fn test_resolve_dir_path_absolute_redundant() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();

    let wrong_path = project.join("myproject").join("src");
    let result = resolve_dir_path(wrong_path.to_str().unwrap(), &project);
    assert_eq!(result, src);
}

#[test]
fn test_resolve_dir_path_relative_multi_component() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();

    let result = resolve_dir_path("myproject/src", &project);
    assert_eq!(result, src);
}

#[test]
fn test_resolve_dir_path_absolute_redundant_nonexistent() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();

    let wrong_path = project.join("myproject").join("newdir");
    let result = resolve_dir_path(wrong_path.to_str().unwrap(), &project);
    assert_eq!(result, project.join("newdir"));
}

#[test]
fn test_resolve_dir_path_dot_slash_redundant() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();

    let result = resolve_dir_path("./myproject/src", &project);
    assert_eq!(result, src);
}

#[test]
fn test_resolve_dir_path_tilde_expanded() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();

    let result = resolve_dir_path("~/nonexistent_dir_xyz", &project);
    assert!(
        !result.to_string_lossy().contains('~'),
        "tilde should be expanded: {result:?}"
    );
}

// ---- hallucinated prefix tests ----

#[test]
fn test_resolve_file_path_workspace_prefix() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("package.json"), "{}").unwrap();

    // LLM hallucinates /workspace/package.json
    let result = resolve_file_path("/workspace/package.json", &project);
    assert_eq!(result, project.join("package.json"));
}

#[test]
fn test_resolve_file_path_workspace_nested() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}").unwrap();

    let result = resolve_file_path("/workspace/src/main.rs", &project);
    assert_eq!(result, project.join("src/main.rs"));
}

#[test]
fn test_resolve_file_path_testbed_prefix() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("main.py"), "").unwrap();

    let result = resolve_file_path("/testbed/main.py", &project);
    assert_eq!(result, project.join("main.py"));
}

#[test]
fn test_resolve_dir_path_workspace_prefix() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    let src = project.join("src");
    fs::create_dir_all(&src).unwrap();

    let result = resolve_dir_path("/workspace/src", &project);
    assert_eq!(result, src);
}

#[test]
fn test_resolve_dir_path_bare_workspace() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();

    // /workspace alone should map to working_dir
    let result = resolve_dir_path("/workspace", &project);
    assert_eq!(result, project);
}

#[test]
fn test_resolve_file_path_workspace_nonexistent_file() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("myproject");
    fs::create_dir(&project).unwrap();

    // Even if the file doesn't exist, /workspace/ should still be rewritten
    let result = resolve_file_path("/workspace/newfile.rs", &project);
    assert_eq!(result, project.join("newfile.rs"));
}
