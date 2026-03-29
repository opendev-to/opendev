use super::*;

#[test]
fn test_create_default_composer_section_count() {
    let dir = tempfile::TempDir::new().unwrap();
    let composer = create_default_composer(dir.path());
    // Should have many sections registered
    assert!(composer.section_count() > 15);
}

#[test]
fn test_create_composer_dispatch() {
    let dir = tempfile::TempDir::new().unwrap();

    let main = create_composer(dir.path(), "system/main");
    assert!(main.section_count() > 15);
}
