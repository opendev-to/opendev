use super::*;

#[test]
fn test_input_widget_creation() {
    let _widget = InputWidget::new("hello", 3, "NORMAL", 0, 0, None);
}

#[test]
fn test_input_widget_empty() {
    let _widget = InputWidget::new("", 0, "NORMAL", 0, 0, None);
}

#[test]
fn test_queue_indicator_in_separator() {
    // Verify the widget renders queue count in the separator line (row 0)
    let area = Rect::new(0, 0, 60, 3);
    let mut buf = Buffer::empty(area);

    let widget = InputWidget::new("", 0, "NORMAL", 2, 0, None);
    widget.render(area, &mut buf);

    let rendered: String = (0..area.width)
        .map(|x| {
            buf.cell((x, 0))
                .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
        })
        .collect();
    assert!(
        rendered.contains("2 messages queued"),
        "Expected '2 messages queued' in separator line, got: {rendered:?}"
    );
}

#[test]
fn test_queue_indicator_single_message() {
    let area = Rect::new(0, 0, 60, 3);
    let mut buf = Buffer::empty(area);

    let widget = InputWidget::new("", 0, "NORMAL", 1, 0, None);
    widget.render(area, &mut buf);

    let rendered: String = (0..area.width)
        .map(|x| {
            buf.cell((x, 0))
                .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
        })
        .collect();
    assert!(
        rendered.contains("1 message queued"),
        "Expected '1 message queued' in separator line, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("1 messages"),
        "Should use singular 'message' for count=1"
    );
}

#[test]
fn test_queue_indicator_bg_results_only() {
    let area = Rect::new(0, 0, 60, 3);
    let mut buf = Buffer::empty(area);

    let widget = InputWidget::new("", 0, "NORMAL", 0, 2, None);
    widget.render(area, &mut buf);

    let rendered: String = (0..area.width)
        .map(|x| {
            buf.cell((x, 0))
                .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
        })
        .collect();
    assert!(
        rendered.contains("2 results queued"),
        "Expected '2 results queued' in separator line, got: {rendered:?}"
    );
    // No ESC hint for bg-only results
    assert!(
        !rendered.contains("ESC"),
        "Should not show ESC hint for bg-only results, got: {rendered:?}"
    );
}

#[test]
fn test_queue_indicator_mixed() {
    let area = Rect::new(0, 0, 60, 3);
    let mut buf = Buffer::empty(area);

    let widget = InputWidget::new("", 0, "NORMAL", 1, 2, None);
    widget.render(area, &mut buf);

    let rendered: String = (0..area.width)
        .map(|x| {
            buf.cell((x, 0))
                .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
        })
        .collect();
    assert!(
        rendered.contains("3 queued"),
        "Expected '3 queued' in separator line, got: {rendered:?}"
    );
}

#[test]
fn test_activity_tag_renders() {
    let area = Rect::new(0, 0, 80, 3);
    let mut buf = Buffer::empty(area);

    let widget = InputWidget::new("", 0, "NORMAL", 0, 0, Some("implementing status bar"));
    widget.render(area, &mut buf);

    let rendered: String = (0..area.width)
        .map(|x| {
            buf.cell((x, 0))
                .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
        })
        .collect();
    assert!(
        rendered.contains("implementing-status-bar"),
        "Expected kebab-cased activity tag in separator line, got: {rendered:?}"
    );
}

#[test]
fn test_activity_tag_with_queue() {
    let area = Rect::new(0, 0, 100, 3);
    let mut buf = Buffer::empty(area);

    let widget = InputWidget::new("", 0, "NORMAL", 1, 0, Some("debugging login"));
    widget.render(area, &mut buf);

    let rendered: String = (0..area.width)
        .map(|x| {
            buf.cell((x, 0))
                .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
        })
        .collect();
    assert!(
        rendered.contains("1 message queued"),
        "Expected queue indicator, got: {rendered:?}"
    );
    assert!(
        rendered.contains("debugging-login"),
        "Expected kebab-cased activity tag, got: {rendered:?}"
    );
}

#[test]
fn test_to_kebab_display() {
    assert_eq!(to_kebab_display("Hello World"), "hello-world");
    assert_eq!(to_kebab_display("Auth Refactor"), "auth-refactor");
    assert_eq!(to_kebab_display("Fix: login bug!"), "fix-login-bug");
    assert_eq!(to_kebab_display("  spaces  "), "spaces");
    assert_eq!(to_kebab_display("already-kebab"), "already-kebab");
    assert_eq!(to_kebab_display("MiXeD CaSe"), "mixed-case");
}

#[test]
fn test_to_kebab_display_long_title_no_truncation() {
    let long_title = "implementing the new authentication middleware refactor";
    let kebab = to_kebab_display(long_title);
    assert_eq!(
        kebab,
        "implementing-the-new-authentication-middleware-refactor"
    );
    // No truncation — full string preserved
    assert!(!kebab.contains("..."));
    assert!(kebab.len() > 30);
}

#[test]
fn test_activity_tag_long_title_not_truncated() {
    let area = Rect::new(0, 0, 120, 3);
    let mut buf = Buffer::empty(area);

    let long_tag = "implementing the new authentication middleware refactor";
    let widget = InputWidget::new("", 0, "NORMAL", 0, 0, Some(long_tag));
    widget.render(area, &mut buf);

    let rendered: String = (0..area.width)
        .map(|x| {
            buf.cell((x, 0))
                .map_or(' ', |c| c.symbol().chars().next().unwrap_or(' '))
        })
        .collect();
    // Full kebab tag should appear, no "..." truncation
    assert!(
        rendered.contains("implementing-the-new-authentication-middleware-refactor"),
        "Expected full long tag without truncation, got: {rendered:?}"
    );
    assert!(
        !rendered.contains("..."),
        "Tag should not be truncated, got: {rendered:?}"
    );
}
