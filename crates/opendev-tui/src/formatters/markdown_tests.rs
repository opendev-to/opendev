use super::*;

#[test]
fn test_plain_text() {
    let lines = MarkdownRenderer::render("Hello world");
    assert_eq!(lines.len(), 1);
}

#[test]
fn test_headers() {
    let lines = MarkdownRenderer::render("# Title\n## Subtitle\n### Section");
    // With spacing: title + blank + blank + subtitle + blank + blank + section + blank = 8
    assert_eq!(lines.len(), 8);
}

#[test]
fn test_code_block() {
    let md = "```rust\nfn main() {}\n```";
    let lines = MarkdownRenderer::render(md);
    // lang hint + code line = 2
    assert_eq!(lines.len(), 2);
}

#[test]
fn test_bullet_list() {
    let md = "- item one\n- item two";
    let lines = MarkdownRenderer::render(md);
    assert_eq!(lines.len(), 2);
}

#[test]
fn test_nested_bullets() {
    let md = "- top\n  - nested\n    - deep";
    let lines = MarkdownRenderer::render(md);
    assert_eq!(lines.len(), 3);
    // Check prefixes
    let first: String = lines[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        first.starts_with("  - "),
        "top-level should start with '  - '"
    );
    let second: String = lines[1]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        second.starts_with("    - "),
        "nested should start with '    - '"
    );
    let third: String = lines[2]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(
        third.starts_with("      - "),
        "deep nested should start with '      - '"
    );
}

#[test]
fn test_ordered_list() {
    let md = "1. first\n2. second\n3. third";
    let lines = MarkdownRenderer::render(md);
    assert_eq!(lines.len(), 3);
    let first: String = lines[0]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(first.contains("1. "));
    let second: String = lines[1]
        .spans
        .iter()
        .map(|s| s.content.to_string())
        .collect();
    assert!(second.contains("2. "));
}

#[test]
fn test_bullet_with_inline_formatting() {
    let md = "- **bold** and `code`";
    let lines = MarkdownRenderer::render(md);
    assert_eq!(lines.len(), 1);
    // Should have more than 2 spans (prefix + inline formatted content)
    assert!(
        lines[0].spans.len() > 2,
        "bullet content should preserve inline formatting"
    );
}

#[test]
fn test_inline_code() {
    let spans = parse_inline_spans("use `tokio` for async");
    assert!(spans.len() >= 3);
}

#[test]
fn test_bold_text() {
    let spans = parse_inline_spans("this is **bold** text");
    assert!(spans.len() >= 3);
    // The bold span should have BOLD modifier
    let bold_span = spans.iter().find(|s| s.content.as_ref() == "bold").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn test_markdown_link() {
    let spans = parse_inline_spans("visit [example](http://example.com) now");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "visit example now");
    let link_span = &spans[1];
    assert_eq!(link_span.content.as_ref(), "example");
}

#[test]
fn test_markdown_link_url_as_text() {
    // Common pattern: [http://url](http://url)
    let spans =
        parse_inline_spans("running at [http://localhost:5173/](http://localhost:5173/).");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "running at http://localhost:5173/.");
}

#[test]
fn test_bold_with_code_inside() {
    // Bug 1: bold markers broken when backticks appear inside
    let spans = parse_inline_spans("**bold `code` more**");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "bold code more");
    // "bold " should be bold
    let bold_span = spans
        .iter()
        .find(|s| s.content.as_ref() == "bold ")
        .unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    // "code" should have code styling and bold
    let code_span = spans.iter().find(|s| s.content.as_ref() == "code").unwrap();
    assert!(code_span.style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(code_span.style.fg, Some(MdPalette::default().code_fg));
    // " more" should be bold
    let more_span = spans
        .iter()
        .find(|s| s.content.as_ref() == " more")
        .unwrap();
    assert!(more_span.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn test_bold_with_link_inside() {
    let spans = parse_inline_spans("**see [link](http://example.com) here**");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "see link here");
    // "see " should be bold
    let see_span = spans.iter().find(|s| s.content.as_ref() == "see ").unwrap();
    assert!(see_span.style.add_modifier.contains(Modifier::BOLD));
    // "link" should have link color and bold
    let link_span = spans.iter().find(|s| s.content.as_ref() == "link").unwrap();
    assert!(link_span.style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(link_span.style.fg, Some(MdPalette::default().link));
}

#[test]
fn test_italic_text() {
    let spans = parse_inline_spans("this is *italic* text");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "this is italic text");
    let italic_span = spans
        .iter()
        .find(|s| s.content.as_ref() == "italic")
        .unwrap();
    assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn test_bold_and_italic() {
    let spans = parse_inline_spans("**bold** and *italic*");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "bold and italic");
    let bold_span = spans.iter().find(|s| s.content.as_ref() == "bold").unwrap();
    assert!(bold_span.style.add_modifier.contains(Modifier::BOLD));
    let italic_span = spans
        .iter()
        .find(|s| s.content.as_ref() == "italic")
        .unwrap();
    assert!(italic_span.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn test_unmatched_bold_renders_literally() {
    let spans = parse_inline_spans("this **has no closing");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    // Should contain ** literally since it's unmatched
    assert!(
        text.contains("**"),
        "unmatched ** should render literally, got: {text}"
    );
}

#[test]
fn test_triple_star_bold_italic() {
    let spans = parse_inline_spans("***bold italic***");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "bold italic");
    let styled = spans
        .iter()
        .find(|s| s.content.as_ref() == "bold italic")
        .unwrap();
    assert!(styled.style.add_modifier.contains(Modifier::BOLD));
    assert!(styled.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn test_multi_backtick_code_span() {
    // Double backtick with inner single backtick
    let spans = parse_inline_spans("use ``code with `backtick` inside`` here");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "use code with `backtick` inside here");
    let code_span = spans
        .iter()
        .find(|s| s.content.as_ref().contains("`backtick`"))
        .unwrap();
    assert_eq!(code_span.style.fg, Some(MdPalette::default().code_fg));
}

#[test]
fn test_bold_italic_nested() {
    // Bold wrapping italic: **bold *and italic* text**
    let spans = parse_inline_spans("**bold *and italic* text**");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "bold and italic text");
    // "and italic" should be bold+italic
    let both = spans
        .iter()
        .find(|s| s.content.as_ref() == "and italic")
        .unwrap();
    assert!(both.style.add_modifier.contains(Modifier::BOLD));
    assert!(both.style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn test_empty_bold_markers() {
    // **** should produce no visible text between bold markers
    let spans = parse_inline_spans("text **** more");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "text  more");
}

#[test]
fn test_adjacent_bold_regions() {
    let spans = parse_inline_spans("**first****second**");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "firstsecond");
}

#[test]
fn test_mid_word_bold() {
    let spans = parse_inline_spans("foo**bar**baz");
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(text, "foobarbaz");
    let bold = spans.iter().find(|s| s.content.as_ref() == "bar").unwrap();
    assert!(bold.style.add_modifier.contains(Modifier::BOLD));
}
