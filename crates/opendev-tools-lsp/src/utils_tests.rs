use super::*;

const SAMPLE: &str = "line zero\nline one\nline two\nline three";

#[test]
fn test_position_to_offset() {
    // "line zero\nline one\nline two\nline three"
    // line 0: 0..9  (line zero)
    // line 1: 10..17 (line one)
    assert_eq!(
        TextUtils::position_to_offset(SAMPLE, Position::new(0, 0)),
        Some(0)
    );
    assert_eq!(
        TextUtils::position_to_offset(SAMPLE, Position::new(0, 4)),
        Some(4)
    );
    assert_eq!(
        TextUtils::position_to_offset(SAMPLE, Position::new(1, 0)),
        Some(10)
    );
    assert_eq!(
        TextUtils::position_to_offset(SAMPLE, Position::new(1, 5)),
        Some(15)
    );
}

#[test]
fn test_offset_to_position() {
    assert_eq!(
        TextUtils::offset_to_position(SAMPLE, 0),
        Some(Position::new(0, 0))
    );
    assert_eq!(
        TextUtils::offset_to_position(SAMPLE, 4),
        Some(Position::new(0, 4))
    );
    assert_eq!(
        TextUtils::offset_to_position(SAMPLE, 10),
        Some(Position::new(1, 0))
    );
}

#[test]
fn test_offset_to_position_end_of_text() {
    let text = "abc";
    assert_eq!(
        TextUtils::offset_to_position(text, 3),
        Some(Position::new(0, 3))
    );
    assert_eq!(TextUtils::offset_to_position(text, 4), None);
}

#[test]
fn test_extract_range() {
    let range = SourceRange::new(Position::new(0, 5), Position::new(0, 9));
    assert_eq!(
        TextUtils::extract_range(SAMPLE, &range),
        Some("zero".to_string())
    );

    let range = SourceRange::new(Position::new(1, 5), Position::new(1, 8));
    assert_eq!(
        TextUtils::extract_range(SAMPLE, &range),
        Some("one".to_string())
    );
}

#[test]
fn test_get_line() {
    assert_eq!(TextUtils::get_line(SAMPLE, 0), Some("line zero"));
    assert_eq!(TextUtils::get_line(SAMPLE, 3), Some("line three"));
    assert_eq!(TextUtils::get_line(SAMPLE, 4), None);
}

#[test]
fn test_line_count() {
    assert_eq!(TextUtils::line_count(SAMPLE), 4);
    assert_eq!(TextUtils::line_count("single"), 1);
    assert_eq!(TextUtils::line_count("a\nb"), 2);
}

#[test]
fn test_replace_range() {
    let range = SourceRange::new(Position::new(0, 5), Position::new(0, 9));
    let result = TextUtils::replace_range(SAMPLE, &range, "ZERO").unwrap();
    assert!(result.starts_with("line ZERO\n"));
}

#[cfg(unix)]
#[test]
fn test_path_to_uri_string() {
    let path = Path::new("/tmp/test.rs");
    let uri = PathUtils::path_to_uri_string(path);
    assert_eq!(uri, "file:///tmp/test.rs");
}

#[test]
fn test_uri_string_to_path() {
    let path = PathUtils::uri_string_to_path("file:///tmp/test.rs").unwrap();
    assert_eq!(path, PathBuf::from("/tmp/test.rs"));
    assert!(PathUtils::uri_string_to_path("http://example.com").is_none());
}

#[test]
fn test_normalize_path() {
    let path = Path::new("/a/b/../c/./d");
    let normalized = PathUtils::normalize(path);
    assert_eq!(normalized, PathBuf::from("/a/c/d"));
}

#[test]
fn test_extension() {
    assert_eq!(
        PathUtils::extension(Path::new("foo.RS")),
        Some("rs".to_string())
    );
    assert_eq!(PathUtils::extension(Path::new("no_ext")), None);
}

#[test]
fn test_atomic_write() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.txt");
    FileUtils::atomic_write(&path, "hello world").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
}
