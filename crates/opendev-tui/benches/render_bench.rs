//! Criterion benchmarks for TUI rendering hot paths.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use std::collections::HashMap;

use opendev_tui::app::{DisplayMessage, DisplayRole, DisplayToolCall};
use opendev_tui::formatters::MarkdownRenderer;
use opendev_tui::widgets::ConversationWidget;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_user_message(content: &str) -> DisplayMessage {
    DisplayMessage {
        role: DisplayRole::User,
        content: content.to_string(),
        tool_call: None,
    }
}

fn make_assistant_message(content: &str) -> DisplayMessage {
    DisplayMessage {
        role: DisplayRole::Assistant,
        content: content.to_string(),
        tool_call: None,
    }
}

fn make_assistant_with_tool(content: &str, tool_name: &str) -> DisplayMessage {
    DisplayMessage {
        role: DisplayRole::Assistant,
        content: content.to_string(),
        tool_call: Some(DisplayToolCall {
            name: tool_name.to_string(),
            arguments: HashMap::new(),
            summary: Some(format!("{tool_name} summary")),
            success: true,
            collapsed: false,
            result_lines: vec!["output line 1".into(), "output line 2".into()],
            nested_calls: vec![],
        }),
    }
}

fn make_conversation(n: usize) -> Vec<DisplayMessage> {
    let mut msgs = Vec::with_capacity(n);
    for i in 0..n {
        if i % 3 == 0 {
            msgs.push(make_user_message(&format!("User message number {i}")));
        } else if i % 3 == 1 {
            msgs.push(make_assistant_message(&format!(
                "Assistant response {i}. This is a longer message with **bold** text and `code`."
            )));
        } else {
            msgs.push(make_assistant_with_tool(
                &format!("Running tool for step {i}"),
                "bash",
            ));
        }
    }
    msgs
}

// ---------------------------------------------------------------------------
// ConversationWidget benchmarks
// ---------------------------------------------------------------------------

fn bench_conversation_render(c: &mut Criterion) {
    let mut group = c.benchmark_group("ConversationWidget::render");

    for count in [10, 100, 1000] {
        let messages = make_conversation(count);
        let area = Rect::new(0, 0, 120, 50);

        group.bench_function(format!("{count}_messages"), |b| {
            b.iter(|| {
                let widget = ConversationWidget::new(black_box(&messages), 0).terminal_width(120);
                let mut buf = Buffer::empty(area);
                widget.render(area, &mut buf);
            });
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// MarkdownRenderer benchmarks
// ---------------------------------------------------------------------------

fn bench_markdown_render(c: &mut Criterion) {
    let mut group = c.benchmark_group("MarkdownRenderer::render");

    let small = "Hello **world** with `code` inline.";
    let medium = (0..50)
        .map(|i| {
            format!(
                "## Heading {i}\n\
                 This is paragraph {i} with **bold**, `code`, and normal text.\n\
                 - bullet point A\n\
                 - bullet point B\n\
                 ```rust\nfn example_{i}() {{}}\n```\n"
            )
        })
        .collect::<String>();
    let large = medium.repeat(20);

    group.bench_function("small_inline", |b| {
        b.iter(|| MarkdownRenderer::render(black_box(small)));
    });

    group.bench_function("medium_50_sections", |b| {
        b.iter(|| MarkdownRenderer::render(black_box(&medium)));
    });

    group.bench_function("large_1000_sections", |b| {
        b.iter(|| MarkdownRenderer::render(black_box(&large)));
    });

    group.finish();
}

criterion_group!(benches, bench_conversation_render, bench_markdown_render);
criterion_main!(benches);
