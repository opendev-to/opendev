use super::*;

#[test]
fn test_is_valid_identifier() {
    assert!(is_valid_identifier("foo"));
    assert!(is_valid_identifier("_bar"));
    assert!(is_valid_identifier("Baz123"));
    assert!(!is_valid_identifier(""));
    assert!(!is_valid_identifier("123"));
    assert!(!is_valid_identifier("foo-bar"));
    assert!(!is_valid_identifier("foo bar"));
}

#[test]
fn test_detect_lang() {
    use std::path::PathBuf;
    assert_eq!(detect_lang(&PathBuf::from("a.py")), LangCategory::Python);
    assert_eq!(detect_lang(&PathBuf::from("a.rs")), LangCategory::CLike);
    assert_eq!(detect_lang(&PathBuf::from("a.txt")), LangCategory::Other);
}

#[test]
fn test_truncate() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 5), "hello...");
}

#[test]
fn test_relative_display() {
    let base = std::path::PathBuf::from("/workspace");
    let path = std::path::PathBuf::from("/workspace/src/main.rs");
    assert_eq!(relative_display(&path, &base), "src/main.rs");

    let other = std::path::PathBuf::from("/other/file.rs");
    assert_eq!(relative_display(&other, &base), "/other/file.rs");
}
