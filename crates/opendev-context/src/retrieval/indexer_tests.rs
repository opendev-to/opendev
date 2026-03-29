use super::*;
use std::fs;

#[test]
fn test_generate_index_with_temp_dir() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.py"), "print('hello')").unwrap();
    fs::write(
        dir.path().join("README.md"),
        "# My Project\n\nA test project.",
    )
    .unwrap();

    let indexer = CodebaseIndexer::new(dir.path());
    let index = indexer.generate_index(5000);

    assert!(index.contains("## Overview"));
    assert!(index.contains("## Structure"));
    assert!(index.contains("## Key Files"));
}

#[test]
fn test_detect_project_type_python() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("pyproject.toml"), "[build-system]").unwrap();

    let indexer = CodebaseIndexer::new(dir.path());
    assert_eq!(indexer.detect_project_type(), Some("Python"));
}

#[test]
fn test_detect_project_type_rust() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

    let indexer = CodebaseIndexer::new(dir.path());
    assert_eq!(indexer.detect_project_type(), Some("Rust"));
}

#[test]
fn test_detect_project_type_none() {
    let dir = tempfile::tempdir().unwrap();
    let indexer = CodebaseIndexer::new(dir.path());
    assert_eq!(indexer.detect_project_type(), None);
}

#[test]
fn test_generate_dependencies_requirements_txt() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("requirements.txt"),
        "flask==2.0\nrequests\n# comment\npytest",
    )
    .unwrap();

    let indexer = CodebaseIndexer::new(dir.path());
    let deps = indexer.generate_dependencies().unwrap();
    assert!(deps.contains("### Python"));
    assert!(deps.contains("flask==2.0"));
    assert!(deps.contains("requests"));
    assert!(!deps.contains("# comment"));
}

#[test]
fn test_compress_content() {
    let indexer = CodebaseIndexer::new(Path::new("/tmp"));
    let content = (0..50)
        .map(|i| format!("Paragraph {} with some text here.", i))
        .collect::<Vec<_>>()
        .join("\n\n");

    // Very tight budget: should truncate
    let compressed = indexer.compress_content(&content, 10);
    assert!(indexer.token_monitor.count_tokens(&compressed) <= 20); // some slack for last paragraph
}

#[test]
fn test_generate_overview_with_readme() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("README.md"),
        "# Title\n\nThis is the description.",
    )
    .unwrap();

    let indexer = CodebaseIndexer::new(dir.path());
    let overview = indexer.generate_overview();
    assert!(overview.contains("## Overview"));
    assert!(overview.contains("# Title"));
}
