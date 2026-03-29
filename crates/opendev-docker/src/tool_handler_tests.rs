use super::*;

fn make_handler() -> DockerToolHandler {
    let session = DockerSession::new("test123", "default");
    DockerToolHandler::new(session, "/workspace", "")
}

#[test]
fn test_translate_path_empty() {
    let h = make_handler();
    assert_eq!(h.translate_path(""), "/workspace");
}

#[test]
fn test_translate_path_container_path() {
    let h = make_handler();
    assert_eq!(h.translate_path("/testbed/repo"), "/testbed/repo");
    assert_eq!(h.translate_path("/workspace/file.py"), "/workspace/file.py");
}

#[test]
fn test_translate_path_relative() {
    let h = make_handler();
    assert_eq!(h.translate_path("src/main.rs"), "/workspace/src/main.rs");
    assert_eq!(h.translate_path("./src/main.rs"), "/workspace/src/main.rs");
}

#[test]
fn test_translate_path_absolute_host() {
    let h = make_handler();
    assert_eq!(
        h.translate_path("/Users/me/project/file.py"),
        "/workspace/file.py"
    );
}

#[test]
fn test_check_command_has_error_nonzero_exit() {
    assert!(DockerToolHandler::check_command_has_error(1, ""));
    assert!(DockerToolHandler::check_command_has_error(127, ""));
}

#[test]
fn test_check_command_has_error_patterns() {
    assert!(DockerToolHandler::check_command_has_error(
        0,
        "Traceback (most recent call last)"
    ));
    assert!(DockerToolHandler::check_command_has_error(
        0,
        "ModuleNotFoundError: foo"
    ));
    assert!(!DockerToolHandler::check_command_has_error(0, "all good"));
}

#[tokio::test]
async fn test_run_command_empty() {
    let h = make_handler();
    let r = h.run_command("", 10.0, None).await;
    assert!(!r.success);
    assert!(r.error.as_deref().unwrap().contains("required"));
}

#[tokio::test]
async fn test_read_file_empty_path() {
    let h = make_handler();
    let r = h.read_file("").await;
    assert!(!r.success);
}

#[tokio::test]
async fn test_write_file_empty_path() {
    let h = make_handler();
    let r = h.write_file("", "content").await;
    assert!(!r.success);
}

#[tokio::test]
async fn test_search_empty_query() {
    let h = make_handler();
    let r = h.search("", None).await;
    assert!(!r.success);
}
