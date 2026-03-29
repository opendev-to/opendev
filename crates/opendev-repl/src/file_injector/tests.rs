use super::*;
use base64::Engine as _;
use std::fs;
use tempfile::TempDir;

fn tmp_injector() -> (TempDir, FileContentInjector) {
    let dir = TempDir::new().unwrap();
    let inj = FileContentInjector::new(dir.path().to_path_buf());
    (dir, inj)
}

// -- extract_refs -------------------------------------------------------

#[test]
fn test_extract_refs_quoted() {
    let (_dir, inj) = tmp_injector();
    let refs = inj.extract_refs(r#"look at @"my file.py""#);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].0, "my file.py");
}

#[test]
fn test_extract_refs_unquoted() {
    let (_dir, inj) = tmp_injector();
    let refs = inj.extract_refs("explain @main.py and @src/utils.rs");
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].0, "main.py");
    assert_eq!(refs[1].0, "src/utils.rs");
}

#[test]
fn test_extract_refs_excludes_emails() {
    let (_dir, inj) = tmp_injector();
    let refs = inj.extract_refs("send to user@example.com please");
    assert!(
        refs.is_empty(),
        "emails should not be extracted: {:?}",
        refs
    );
}

#[test]
fn test_extract_refs_dedup() {
    let (_dir, inj) = tmp_injector();
    let refs = inj.extract_refs("@foo.py and @foo.py again");
    assert_eq!(refs.len(), 1);
}

#[test]
fn test_extract_refs_mixed() {
    let (_dir, inj) = tmp_injector();
    let refs = inj.extract_refs(r#"@plain.rs and @"quoted path.txt""#);
    assert_eq!(refs.len(), 2);
}

// -- is_text_file -------------------------------------------------------

#[test]
fn test_is_text_file_known_extensions() {
    let dir = TempDir::new().unwrap();
    for ext in &[".py", ".rs", ".js", ".md", ".json", ".toml", ".yaml"] {
        let p = dir.path().join(format!("test{}", ext));
        fs::write(&p, "content").unwrap();
        assert!(
            FileContentInjector::is_text_file(&p),
            "{} should be text",
            ext
        );
    }
}

#[test]
fn test_is_text_file_known_filenames() {
    let dir = TempDir::new().unwrap();
    for name in &["Dockerfile", "Makefile", "README", "LICENSE"] {
        let p = dir.path().join(name);
        fs::write(&p, "content").unwrap();
        assert!(
            FileContentInjector::is_text_file(&p),
            "{} should be text",
            name
        );
    }
}

#[test]
fn test_is_text_file_binary_extension() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("thing.exe");
    fs::write(&p, "MZ\x00\x00").unwrap();
    assert!(!FileContentInjector::is_text_file(&p));
}

#[test]
fn test_is_text_file_unknown_ext_text() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("data.cfg");
    fs::write(&p, "key = value\nfoo = bar\n").unwrap();
    assert!(FileContentInjector::is_text_file(&p));
}

// -- detect_text_file ---------------------------------------------------

#[test]
fn test_detect_text_file_with_nulls() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("binary.dat");
    fs::write(&p, b"hello\x00world").unwrap();
    assert!(!FileContentInjector::detect_text_file(&p));
}

#[test]
fn test_detect_text_file_empty() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("empty");
    fs::write(&p, b"").unwrap();
    assert!(FileContentInjector::detect_text_file(&p));
}

// -- get_language -------------------------------------------------------

#[test]
fn test_get_language_known() {
    assert_eq!(
        FileContentInjector::get_language(Path::new("foo.py")),
        "python"
    );
    assert_eq!(
        FileContentInjector::get_language(Path::new("bar.rs")),
        "rust"
    );
    assert_eq!(
        FileContentInjector::get_language(Path::new("baz.ts")),
        "typescript"
    );
}

#[test]
fn test_get_language_unknown() {
    assert_eq!(FileContentInjector::get_language(Path::new("data.xyz")), "");
}

// -- format_size --------------------------------------------------------

#[test]
fn test_format_size_bytes() {
    assert_eq!(FileContentInjector::format_size(0), "0B");
    assert_eq!(FileContentInjector::format_size(512), "512B");
    assert_eq!(FileContentInjector::format_size(1023), "1023B");
}

#[test]
fn test_format_size_kilobytes() {
    assert_eq!(FileContentInjector::format_size(1024), "1.0KB");
    assert_eq!(FileContentInjector::format_size(2560), "2.5KB");
}

#[test]
fn test_format_size_megabytes() {
    assert_eq!(FileContentInjector::format_size(1048576), "1.0MB");
    assert_eq!(FileContentInjector::format_size(5 * 1024 * 1024), "5.0MB");
}

// -- process_text_file --------------------------------------------------

#[test]
fn test_process_text_file_output() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("hello.py");
    fs::write(&p, "print('hello')").unwrap();
    let result = FileContentInjector::process_text_file(&p, "hello.py").unwrap();
    assert!(result.contains("<file_content"));
    assert!(result.contains("path=\"hello.py\""));
    assert!(result.contains("language=\"python\""));
    assert!(result.contains("print('hello')"));
    assert!(result.contains("</file_content>"));
}

// -- process_large_file -------------------------------------------------

#[test]
fn test_process_large_file_truncation() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("big.py");

    // Create a file with 2000 lines
    let lines_vec: Vec<String> = (0..2000).map(|i| format!("line {}", i)).collect();
    let content = lines_vec.join("\n");
    fs::write(&p, &content).unwrap();

    let lines: Vec<&str> = content.lines().collect();
    let result = FileContentInjector::process_large_file(&p, "big.py", &content, &lines);
    assert!(result.contains("<file_truncated"));
    assert!(result.contains("total_lines=\"2000\""));
    assert!(result.contains("=== HEAD"));
    assert!(result.contains("=== TRUNCATED"));
    assert!(result.contains("=== TAIL"));
    assert!(result.contains("</file_truncated>"));
}

// -- process_directory --------------------------------------------------

#[test]
fn test_process_directory_output() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("mydir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("a.txt"), "aaa").unwrap();
    fs::write(sub.join("b.txt"), "bbb").unwrap();

    let inj = FileContentInjector::new(dir.path().to_path_buf());
    let result = inj.process_directory(&sub, "mydir");
    assert!(result.contains("<directory_listing"));
    assert!(result.contains("path=\"mydir\""));
    assert!(result.contains("a.txt"));
    assert!(result.contains("b.txt"));
    assert!(result.contains("</directory_listing>"));
}

#[test]
fn test_process_directory_ignores_git() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("proj");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join(".git").join("config"), "x").unwrap();
    fs::write(root.join("main.rs"), "fn main() {}").unwrap();

    let inj = FileContentInjector::new(dir.path().to_path_buf());
    let result = inj.process_directory(&root, "proj");
    assert!(!result.contains(".git"));
    assert!(result.contains("main.rs"));
}

// -- process_image ------------------------------------------------------

#[test]
fn test_process_image_base64() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("logo.png");
    // Minimal 1x1 red PNG
    let png_bytes: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG header
        0x00, 0x00, 0x00, 0x01, // chunk length (fake minimal data)
    ];
    fs::write(&p, png_bytes).unwrap();

    let (tag, block) = FileContentInjector::process_image(&p, "logo.png");
    assert!(tag.contains("<image"));
    assert!(tag.contains("type=\"image/png\""));
    assert!(tag.contains("[Image attached as multimodal content]"));

    let block = block.expect("should produce an ImageBlock");
    assert_eq!(block.media_type, "image/png");
    // Verify the base64 decodes back to original bytes.
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&block.data)
        .unwrap();
    assert_eq!(decoded, png_bytes);
}

#[test]
fn test_process_image_jpeg_mime() {
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("photo.jpg");
    fs::write(&p, b"fake jpeg data").unwrap();
    let (_tag, block) = FileContentInjector::process_image(&p, "photo.jpg");
    assert_eq!(block.unwrap().media_type, "image/jpeg");
}

// -- inject_content (end-to-end) ----------------------------------------

#[test]
fn test_inject_content_no_refs() {
    let (_dir, inj) = tmp_injector();
    let result = inj.inject_content("just a plain query");
    assert!(result.text_content.is_empty());
    assert!(result.image_blocks.is_empty());
    assert!(result.errors.is_empty());
}

#[test]
fn test_inject_content_file_not_found() {
    let (_dir, inj) = tmp_injector();
    let result = inj.inject_content("look at @nonexistent.py");
    assert!(result.text_content.contains("file_error"));
    assert!(result.text_content.contains("File not found"));
    assert_eq!(result.errors.len(), 1);
}

#[test]
fn test_inject_content_text_file() {
    let (dir, inj) = tmp_injector();
    let p = dir.path().join("hello.rs");
    fs::write(&p, "fn main() {}").unwrap();

    let result = inj.inject_content("explain @hello.rs");
    assert!(result.text_content.contains("<file_content"));
    assert!(result.text_content.contains("fn main() {}"));
    assert!(result.errors.is_empty());
}

#[test]
fn test_inject_content_directory() {
    let (dir, inj) = tmp_injector();
    let sub = dir.path().join("src");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("lib.rs"), "// lib").unwrap();

    let result = inj.inject_content("show me @src");
    assert!(result.text_content.contains("<directory_listing"));
    assert!(result.text_content.contains("lib.rs"));
}

#[test]
fn test_inject_content_image() {
    let (dir, inj) = tmp_injector();
    let p = dir.path().join("pic.png");
    fs::write(&p, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

    let result = inj.inject_content("analyze @pic.png");
    assert!(result.text_content.contains("<image"));
    assert_eq!(result.image_blocks.len(), 1);
}

#[test]
fn test_inject_content_unsupported() {
    let (dir, inj) = tmp_injector();
    let p = dir.path().join("data.exe");
    fs::write(&p, b"\x00\x00\x00\x00").unwrap();

    let result = inj.inject_content("look at @data.exe");
    // .exe is a known binary extension AND an image ext isn't matched,
    // so it goes through process_ref which calls is_text_file => false => Unsupported
    assert!(result.text_content.contains("file_error"));
}

// -- resolve_path -------------------------------------------------------

#[test]
fn test_resolve_path_relative() {
    let (dir, inj) = tmp_injector();
    let p = dir.path().join("test.py");
    fs::write(&p, "x").unwrap();
    let resolved = inj.resolve_path("test.py");
    assert!(resolved.is_absolute());
    assert!(resolved.ends_with("test.py"));
}

#[test]
fn test_resolve_path_absolute() {
    let (_dir, inj) = tmp_injector();
    let abs_path = std::env::temp_dir().join("some_file.py");
    let abs_str = abs_path.to_str().unwrap();
    let resolved = inj.resolve_path(abs_str);
    assert_eq!(resolved, abs_path);
}
