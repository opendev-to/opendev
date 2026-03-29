use super::*;

#[test]
fn test_is_binary() {
    assert!(is_binary(&[0u8, 1, 2]));
    assert!(!is_binary(b"hello world\n"));
}

#[test]
fn test_is_binary_file_by_extension() {
    let path = std::path::Path::new("image.png");
    assert!(is_binary_file(path, b"this is actually text"));

    let path = std::path::Path::new("archive.zip");
    assert!(is_binary_file(path, b"text content"));

    let path = std::path::Path::new("data.sqlite3");
    assert!(is_binary_file(path, b"text content"));
}

#[test]
fn test_is_binary_file_case_insensitive() {
    let path = std::path::Path::new("image.PNG");
    assert!(is_binary_file(path, b"text"));

    let path = std::path::Path::new("image.Jpg");
    assert!(is_binary_file(path, b"text"));
}

#[test]
fn test_is_binary_file_text_extensions_use_content() {
    // .rs file with no null bytes = not binary
    let path = std::path::Path::new("main.rs");
    assert!(!is_binary_file(path, b"fn main() {}"));

    // .rs file with null bytes = binary
    let path = std::path::Path::new("main.rs");
    assert!(is_binary_file(path, &[0u8, 1, 2]));
}

#[test]
fn test_is_binary_file_no_extension() {
    // No extension, fallback to content check
    let path = std::path::Path::new("Makefile");
    assert!(!is_binary_file(path, b"all: build"));
    assert!(is_binary_file(path, &[0u8]));
}

#[test]
fn test_binary_detection_null_bytes() {
    let bytes = b"hello\x00world";
    assert!(is_binary(bytes));
}

#[test]
fn test_binary_detection_high_non_printable_ratio() {
    // 50% non-printable chars (bytes < 9)
    let mut bytes = vec![0x01u8; 50];
    bytes.extend_from_slice(&[b'a'; 50]);
    assert!(is_binary(&bytes));
}

#[test]
fn test_binary_detection_low_non_printable_ratio() {
    // Mostly printable with a few control chars (< 30%)
    let mut bytes = vec![b'a'; 100];
    bytes[0] = 0x01;
    bytes[1] = 0x02;
    assert!(!is_binary(&bytes));
}

#[test]
fn test_binary_detection_empty() {
    assert!(!is_binary(&[]));
}

#[test]
fn test_binary_detection_text_with_tabs_newlines() {
    // Tabs and newlines should NOT count as non-printable
    let bytes = b"hello\tworld\nfoo\rbar";
    assert!(!is_binary(bytes));
}
