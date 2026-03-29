use super::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn tmp_enhancer() -> (TempDir, QueryEnhancer) {
    let dir = TempDir::new().unwrap();
    let enh = QueryEnhancer::new(dir.path().to_path_buf());
    (dir, enh)
}

// -- enhance_query ------------------------------------------------------

#[test]
fn test_enhance_query_no_refs() {
    let (_dir, enh) = tmp_enhancer();
    let (enhanced, images) = enh.enhance_query("just a plain query");
    assert_eq!(enhanced, "just a plain query");
    assert!(images.is_empty());
}

#[test]
fn test_enhance_query_strips_at_unquoted() {
    let (_dir, enh) = tmp_enhancer();
    // File doesn't exist so no content appended, but @ should still be stripped
    let (enhanced, _) = enh.enhance_query("look at @main.py please");
    assert!(!enhanced.contains("@main.py"));
    assert!(enhanced.contains("main.py"));
}

#[test]
fn test_enhance_query_strips_at_quoted() {
    let (_dir, enh) = tmp_enhancer();
    let (enhanced, _) = enh.enhance_query(r#"check @"my file.py" now"#);
    assert!(!enhanced.contains("@\""));
    assert!(enhanced.contains("my file.py"));
}

#[test]
fn test_enhance_query_preserves_email() {
    let (_dir, enh) = tmp_enhancer();
    let (enhanced, _) = enh.enhance_query("send to user@example.com");
    assert!(enhanced.contains("user@example.com"));
}

#[test]
fn test_enhance_query_injects_file_content() {
    let (dir, enh) = tmp_enhancer();
    let p = dir.path().join("hello.rs");
    fs::write(&p, "fn main() {}").unwrap();

    let (enhanced, images) = enh.enhance_query("explain @hello.rs");
    assert!(enhanced.contains("<file_content"));
    assert!(enhanced.contains("fn main() {}"));
    assert!(images.is_empty());
}

#[test]
fn test_enhance_query_image_blocks() {
    let (dir, enh) = tmp_enhancer();
    let p = dir.path().join("pic.png");
    fs::write(&p, &[0x89, 0x50, 0x4E, 0x47]).unwrap();

    let (enhanced, images) = enh.enhance_query("analyze @pic.png");
    assert!(enhanced.contains("<image"));
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].media_type, "image/png");
}

// -- prepare_messages ---------------------------------------------------

#[test]
fn test_prepare_messages_basic() {
    let (_dir, enh) = tmp_enhancer();
    let msgs = enh.prepare_messages("hello", "hello", "You are helpful.", None, &[], false, None);
    assert_eq!(msgs.len(), 1); // just system message
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "You are helpful.");
}

#[test]
fn test_prepare_messages_with_session() {
    let (_dir, enh) = tmp_enhancer();
    let session = vec![
        json!({"role": "user", "content": "hi"}),
        json!({"role": "assistant", "content": "hello"}),
    ];
    let msgs = enh.prepare_messages(
        "hi",
        "hi",
        "system prompt",
        Some(&session),
        &[],
        false,
        None,
    );
    // system + user + assistant
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[1]["role"], "user");
    assert_eq!(msgs[2]["role"], "assistant");
}

#[test]
fn test_prepare_messages_replaces_enhanced_content() {
    let (_dir, enh) = tmp_enhancer();
    let session = vec![json!({"role": "user", "content": "look at @foo.py"})];
    let msgs = enh.prepare_messages(
        "look at @foo.py",
        "look at foo.py\n\n<file_content>...</file_content>",
        "sys",
        Some(&session),
        &[],
        false,
        None,
    );
    // Last user message content should be the enhanced version
    let user_msg = &msgs[1];
    assert_eq!(user_msg["role"], "user");
    assert!(
        user_msg["content"]
            .as_str()
            .unwrap()
            .contains("<file_content>")
    );
}

#[test]
fn test_prepare_messages_thinking_placeholder() {
    let (_dir, enh) = tmp_enhancer();

    // thinking visible
    let msgs = enh.prepare_messages(
        "q",
        "q",
        "Do this: {thinking_instruction}",
        None,
        &[],
        true,
        None,
    );
    let content = msgs[0]["content"].as_str().unwrap();
    assert!(content.contains("reasoning"));
    assert!(!content.contains("{thinking_instruction}"));

    // thinking hidden
    let msgs = enh.prepare_messages(
        "q",
        "q",
        "Do this: {thinking_instruction}",
        None,
        &[],
        false,
        None,
    );
    let content = msgs[0]["content"].as_str().unwrap();
    assert!(content.contains("directly"));
    assert!(!content.contains("{thinking_instruction}"));
}

#[test]
fn test_prepare_messages_playbook_context() {
    let (_dir, enh) = tmp_enhancer();
    let msgs = enh.prepare_messages(
        "q",
        "q",
        "base prompt",
        None,
        &[],
        false,
        Some("- Always run tests before committing"),
    );
    let content = msgs[0]["content"].as_str().unwrap();
    assert!(content.contains("## Learned Strategies"));
    assert!(content.contains("Always run tests before committing"));
}

#[test]
fn test_prepare_messages_multimodal_images() {
    let (_dir, enh) = tmp_enhancer();
    let session = vec![json!({"role": "user", "content": "analyze this image"})];
    let images = vec![ImageBlock {
        media_type: "image/png".to_string(),
        data: "base64data".to_string(),
    }];
    let msgs = enh.prepare_messages(
        "analyze this image",
        "analyze this image",
        "sys",
        Some(&session),
        &images,
        false,
        None,
    );
    // Last user message should be multimodal (array of content blocks)
    let user_content = &msgs[1]["content"];
    assert!(user_content.is_array());
    let blocks = user_content.as_array().unwrap();
    assert_eq!(blocks.len(), 2); // text + image
    assert_eq!(blocks[0]["type"], "text");
    assert_eq!(blocks[1]["type"], "image");
}

// -- format_messages_summary --------------------------------------------

#[test]
fn test_format_messages_summary_empty() {
    let summary = QueryEnhancer::format_messages_summary(&[], 60);
    assert_eq!(summary, "0 messages");
}

#[test]
fn test_format_messages_summary_basic() {
    let msgs = vec![
        json!({"role": "system", "content": "You are helpful."}),
        json!({"role": "user", "content": "Hello world"}),
    ];
    let summary = QueryEnhancer::format_messages_summary(&msgs, 60);
    assert!(summary.starts_with("2 messages:"));
    assert!(summary.contains("system:"));
    assert!(summary.contains("user:"));
}

#[test]
fn test_format_messages_summary_truncates() {
    let msgs = vec![json!({"role": "user", "content": "a]".repeat(100)})];
    let summary = QueryEnhancer::format_messages_summary(&msgs, 10);
    assert!(summary.contains("..."));
}
