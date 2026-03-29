use super::*;

#[test]
fn test_file_type_indicator_rust() {
    let (tag, color) = file_type_indicator("src/main.rs");
    assert_eq!(tag, "rs");
    assert_eq!(color, "Red");
}

#[test]
fn test_file_type_indicator_python() {
    let (tag, _) = file_type_indicator("script.py");
    assert_eq!(tag, "py");
}

#[test]
fn test_file_type_indicator_unknown() {
    let (tag, _) = file_type_indicator("data.xyz");
    assert_eq!(tag, "file");
}

#[test]
fn test_file_type_indicator_makefile() {
    let (tag, _) = file_type_indicator("Makefile");
    assert_eq!(tag, "make");
}

#[test]
fn test_shorten_path_short() {
    assert_eq!(shorten_path("src/lib.rs", 30), "src/lib.rs");
}

#[test]
fn test_shorten_path_long() {
    let long = "very/deep/nested/directory/structure/file.rs";
    let shortened = shorten_path(long, 25);
    assert!(shortened.len() <= 30);
    assert!(shortened.contains("...") || shortened.len() <= 25);
}

#[test]
fn test_format_command() {
    let item = CompletionItem {
        insert_text: "/help".into(),
        label: "/help".into(),
        description: "show available commands".into(),
        kind: CompletionKind::Command,
        score: 0.0,
    };
    let (label, desc) = CompletionFormatter::format(&item);
    assert!(label.contains("/help"));
    assert!(desc.contains("show available commands"));
}

#[test]
fn test_format_file() {
    let item = CompletionItem {
        insert_text: "@src/main.rs".into(),
        label: "src/main.rs".into(),
        description: "1.2 KB".into(),
        kind: CompletionKind::File,
        score: 0.0,
    };
    let (label, desc) = CompletionFormatter::format(&item);
    assert!(label.contains("rs"));
    assert!(label.contains("src/main.rs"));
    assert!(desc.contains("1.2 KB"));
}

#[test]
fn test_format_symbol() {
    let item = CompletionItem {
        insert_text: "MyStruct".into(),
        label: "MyStruct".into(),
        description: "struct".into(),
        kind: CompletionKind::Symbol,
        score: 0.0,
    };
    let (label, desc) = CompletionFormatter::format(&item);
    assert!(label.contains("MyStruct"));
    assert_eq!(desc, "struct");
}
