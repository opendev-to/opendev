//! Conversation message caching and incremental rebuild logic.

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::{App, DisplayMessage, DisplayRole};

/// Compute a hash key for markdown cache lookup from role and content.
fn markdown_cache_key(role: &DisplayRole, content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(role).hash(&mut hasher);
    content.hash(&mut hasher);
    hasher.finish()
}

/// Compute a content hash for a `DisplayMessage` used by per-message dirty tracking.
fn display_message_hash(msg: &DisplayMessage) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(&msg.role).hash(&mut hasher);
    msg.content.hash(&mut hasher);
    msg.collapsed.hash(&mut hasher);
    if let Some(ref tc) = msg.tool_call {
        tc.name.hash(&mut hasher);
        format!("{:?}", tc.arguments).hash(&mut hasher);
        tc.summary.hash(&mut hasher);
        tc.success.hash(&mut hasher);
        tc.collapsed.hash(&mut hasher);
        tc.result_lines.hash(&mut hasher);
        tc.nested_calls.len().hash(&mut hasher);
        for nested in &tc.nested_calls {
            nested.name.hash(&mut hasher);
            nested.success.hash(&mut hasher);
            format!("{:?}", nested.arguments).hash(&mut hasher);
        }
    }
    hasher.finish()
}

impl App {
    pub fn clear_markdown_cache(&mut self) {
        self.state.markdown_cache.clear();
    }

    /// Rebuild the cached static conversation lines from messages.
    ///
    /// Uses per-message dirty tracking: each message's content is hashed and
    /// compared with the stored hash. Only messages whose hash changed or that
    /// are new get re-rendered. If a message in the middle changed, we rebuild
    /// from that point forward.
    ///
    /// Viewport culling is still applied: messages far above the visible viewport
    /// emit placeholder blank lines to preserve scroll math.
    pub(super) fn rebuild_cached_lines(&mut self) {
        use crate::formatters::display::strip_system_reminders;

        let num_messages = self.state.messages.len();

        // Compute per-message hashes for the current messages
        let new_hashes: Vec<u64> = self
            .state
            .messages
            .iter()
            .map(display_message_hash)
            .collect();

        // Find the first message index where the hash differs
        let first_dirty = {
            let old_len = self.state.per_message_hashes.len();
            if old_len > num_messages {
                0 // Messages were removed -- full rebuild
            } else {
                let mut dirty_idx = old_len;
                for (i, new_hash) in new_hashes
                    .iter()
                    .enumerate()
                    .take(old_len.min(num_messages))
                {
                    if self.state.per_message_hashes[i] != *new_hash {
                        dirty_idx = i;
                        break;
                    }
                }
                dirty_idx
            }
        };

        // Nothing changed
        if first_dirty >= num_messages && self.state.per_message_hashes.len() == num_messages {
            return;
        }

        // If the first dirty message attaches to its predecessor, re-render that
        // predecessor too so its trailing blank line can be suppressed.
        let first_dirty = if first_dirty > 0
            && self
                .state
                .messages
                .get(first_dirty)
                .and_then(|m| m.role.style())
                .is_some_and(|s| s.attach_to_previous)
        {
            first_dirty - 1
        } else {
            first_dirty
        };

        // Truncate to the point before first_dirty
        let lines_to_keep: usize = self
            .state
            .per_message_line_counts
            .iter()
            .take(first_dirty)
            .sum();
        self.state.cached_lines.truncate(lines_to_keep);
        self.state.per_message_hashes.truncate(first_dirty);
        self.state.per_message_line_counts.truncate(first_dirty);

        // --- Viewport culling ---
        let viewport_h = self.state.terminal_height as usize;
        let buffer_lines = 50;
        let visible_from_bottom = self.state.scroll_offset as usize + viewport_h + buffer_lines;

        let msg_line_estimates: Vec<usize> = self
            .state
            .messages
            .iter()
            .map(|msg| {
                let content = strip_system_reminders(&msg.content);
                let text_lines = if content.is_empty() {
                    0
                } else {
                    content.lines().count()
                };
                let tool_lines = if let Some(ref tc) = msg.tool_call {
                    1 + if !tc.collapsed {
                        tc.result_lines.len()
                    } else if !tc.result_lines.is_empty() {
                        1
                    } else {
                        0
                    } + tc.nested_calls.len()
                } else {
                    0
                };
                text_lines + tool_lines + 1
            })
            .collect();

        let total_estimated: usize = msg_line_estimates.iter().sum();
        let cull_start = total_estimated.saturating_sub(visible_from_bottom);
        let mut cumulative = 0usize;
        let msg_visible: Vec<bool> = msg_line_estimates
            .iter()
            .map(|&est| {
                let msg_end = cumulative + est;
                cumulative = msg_end;
                msg_end > cull_start
            })
            .collect();

        // Re-render only messages from first_dirty onward
        for msg_idx in first_dirty..num_messages {
            let msg = &self.state.messages[msg_idx];
            let lines_before = self.state.cached_lines.len();

            if !msg_visible[msg_idx] {
                let est = msg_line_estimates[msg_idx];
                for _ in 0..est {
                    self.state.cached_lines.push(ratatui::text::Line::from(""));
                }
            } else {
                let next_role = self.state.messages.get(msg_idx + 1).map(|m| &m.role);
                Self::render_single_message(
                    msg,
                    next_role,
                    &mut self.state.cached_lines,
                    &mut self.state.markdown_cache,
                    Some(&self.state.working_dir),
                );
            }

            let lines_produced = self.state.cached_lines.len() - lines_before;
            self.state.per_message_hashes.push(new_hashes[msg_idx]);
            self.state.per_message_line_counts.push(lines_produced);
        }
    }

    /// Render a single `DisplayMessage` into styled lines, appending to `lines`.
    /// `next_role` is the role of the following message (if any), used to suppress
    /// the trailing blank line before messages that attach to the previous one.
    pub(super) fn render_single_message(
        msg: &DisplayMessage,
        next_role: Option<&DisplayRole>,
        lines: &mut Vec<ratatui::text::Line<'static>>,
        markdown_cache: &mut HashMap<u64, Vec<ratatui::text::Line<'static>>>,
        working_dir: Option<&str>,
    ) {
        use crate::formatters::display::strip_system_reminders;
        use crate::formatters::markdown::MarkdownRenderer;
        use crate::formatters::style_tokens::{self, Indent};
        use crate::formatters::tool_registry::{categorize_tool, format_tool_call_parts_with_wd};
        use crate::widgets::spinner::{COMPLETED_CHAR, CONTINUATION_CHAR};
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let content = strip_system_reminders(&msg.content);
        if content.is_empty() && msg.tool_call.is_none() {
            return;
        }

        match msg.role {
            DisplayRole::Assistant => {
                let cache_key = markdown_cache_key(&msg.role, &content);
                let md_lines = if let Some(cached) = markdown_cache.get(&cache_key) {
                    cached.clone()
                } else {
                    let rendered = MarkdownRenderer::render(&content);
                    markdown_cache.insert(cache_key, rendered.clone());
                    rendered
                };
                let mut leading_consumed = false;
                for md_line in md_lines {
                    let line_text: String = md_line
                        .spans
                        .iter()
                        .map(|s| s.content.to_string())
                        .collect();
                    let has_content = !line_text.trim().is_empty();

                    if !leading_consumed && has_content {
                        let mut spans = vec![Span::styled(
                            format!("{} ", COMPLETED_CHAR),
                            Style::default().fg(style_tokens::GREEN_BRIGHT),
                        )];
                        spans.extend(
                            md_line
                                .spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.to_string(), s.style)),
                        );
                        lines.push(Line::from(spans));
                        leading_consumed = true;
                    } else {
                        let mut spans = vec![Span::raw(Indent::CONT)];
                        spans.extend(
                            md_line
                                .spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.to_string(), s.style)),
                        );
                        lines.push(Line::from(spans));
                    }
                }
            }
            DisplayRole::User | DisplayRole::System | DisplayRole::Interrupt => {
                let rs = msg.role.style().unwrap();
                for (i, line_text) in content.lines().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(rs.icon.clone(), rs.icon_style),
                            Span::styled(line_text.to_string(), Style::default().fg(rs.text_color)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw(rs.continuation),
                            Span::styled(line_text.to_string(), Style::default().fg(rs.text_color)),
                        ]));
                    }
                }
            }
            DisplayRole::Thinking => {
                for (i, content_line) in content.lines().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{} ", style_tokens::THINKING_ICON),
                                Style::default().fg(style_tokens::THINKING_BG),
                            ),
                            Span::styled(
                                content_line.to_string(),
                                Style::default()
                                    .fg(style_tokens::THINKING_BG)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw(Indent::CONT),
                            Span::styled(
                                content_line.to_string(),
                                Style::default()
                                    .fg(style_tokens::THINKING_BG)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ]));
                    }
                }
            }
        }

        // Tool call summary
        if let Some(ref tc) = msg.tool_call {
            let (icon, icon_color) = if tc.success {
                (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
            } else {
                (COMPLETED_CHAR, style_tokens::ERROR)
            };
            let (verb, arg) = format_tool_call_parts_with_wd(&tc.name, &tc.arguments, working_dir);
            lines.push(Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                Span::styled(
                    verb,
                    Style::default()
                        .fg(style_tokens::PRIMARY)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({arg})"),
                    Style::default().fg(style_tokens::SUBTLE),
                ),
            ]));

            // Diff tools are never collapsed
            use crate::widgets::conversation::{
                is_diff_tool, parse_unified_diff, render_diff_entries,
            };
            let effective_collapsed = tc.collapsed && !is_diff_tool(&tc.name);
            if !effective_collapsed && !tc.result_lines.is_empty() {
                let use_diff = is_diff_tool(&tc.name);
                if use_diff {
                    let (summary, entries) = parse_unified_diff(&tc.result_lines);
                    if !summary.is_empty() {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("  {}  ", CONTINUATION_CHAR),
                                Style::default().fg(style_tokens::GREY),
                            ),
                            Span::styled(summary, Style::default().fg(style_tokens::SUBTLE)),
                        ]));
                    }
                    render_diff_entries(&entries, lines);
                } else {
                    for (i, result_line) in tc.result_lines.iter().enumerate() {
                        let prefix_char: Cow<'static, str> = if i == 0 {
                            format!("  {}  ", CONTINUATION_CHAR).into()
                        } else {
                            Cow::Borrowed(Indent::RESULT_CONT)
                        };
                        lines.push(Line::from(vec![
                            Span::styled(prefix_char, Style::default().fg(style_tokens::SUBTLE)),
                            Span::styled(
                                result_line.clone(),
                                Style::default().fg(style_tokens::SUBTLE),
                            ),
                        ]));
                    }
                }
            } else if effective_collapsed && !tc.result_lines.is_empty() {
                let count = tc.result_lines.len();
                let is_read = categorize_tool(&tc.name)
                    == crate::formatters::tool_registry::ToolCategory::FileRead;
                let label = if is_read {
                    format!("  {}  ({count} lines)", CONTINUATION_CHAR)
                } else {
                    format!(
                        "  {}  ({count} lines collapsed, press Ctrl+O to expand)",
                        CONTINUATION_CHAR,
                    )
                };
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(style_tokens::SUBTLE),
                )));
            }

            for nested in &tc.nested_calls {
                let (n_icon, n_icon_color) = if nested.success {
                    (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
                } else {
                    (COMPLETED_CHAR, style_tokens::ERROR)
                };
                let (n_verb, n_arg) =
                    format_tool_call_parts_with_wd(&nested.name, &nested.arguments, working_dir);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}\u{2514}\u{2500} ", Indent::CONT),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                    Span::styled(format!("{n_icon} "), Style::default().fg(n_icon_color)),
                    Span::styled(
                        n_verb,
                        Style::default()
                            .fg(style_tokens::PRIMARY)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("({n_arg})"),
                        Style::default().fg(style_tokens::SUBTLE),
                    ),
                ]));
            }
        }

        // Blank line between messages — skip before messages that attach to previous
        let next_attaches = next_role
            .and_then(|r| r.style())
            .is_some_and(|s| s.attach_to_previous);
        if !next_attaches {
            lines.push(Line::from(""));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[test]
    fn test_viewport_culling_cached_lines() {
        let mut app = App::new();
        // Add many messages
        for i in 0..100 {
            app.state.messages.push(DisplayMessage {
                role: DisplayRole::User,
                content: format!("Message {i}"),
                tool_call: None,
                collapsed: false,
            });
        }
        app.state.message_generation = 1;
        app.state.terminal_height = 24;
        app.state.scroll_offset = 0;

        // Build cached lines
        app.rebuild_cached_lines();

        // Should have lines for all messages (some may be placeholders)
        assert!(
            !app.state.cached_lines.is_empty(),
            "cached_lines should not be empty"
        );
    }

    // ---------------------------------------------------------------
    // Per-message dirty tracking tests
    // ---------------------------------------------------------------

    #[test]

    fn test_markdown_cache_hit() {
        let mut app = App::new();
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Assistant,
            content: "Hello **world**".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 1);
        let first_lines = app.state.cached_lines.clone();
        app.state.per_message_hashes.clear();
        app.state.per_message_line_counts.clear();
        app.state.cached_lines.clear();
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 1);
        assert_eq!(app.state.cached_lines.len(), first_lines.len());
    }

    #[test]
    fn test_markdown_cache_miss_different_content() {
        let mut app = App::new();
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Assistant,
            content: "Hello **world**".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 1);
        app.state.messages[0].content = "Goodbye **world**".into();
        app.rebuild_cached_lines();
        assert_eq!(app.state.markdown_cache.len(), 2);
    }

    #[test]
    fn test_markdown_cache_clear() {
        let mut app = App::new();
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Assistant,
            content: "# Title\nSome text".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert!(!app.state.markdown_cache.is_empty());
        app.clear_markdown_cache();
        assert!(app.state.markdown_cache.is_empty());
    }

    #[test]

    fn test_incremental_append_only_renders_new_message() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "First message".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        let lines_after_first = app.state.cached_lines.len();
        assert!(lines_after_first > 0);
        assert_eq!(app.state.per_message_hashes.len(), 1);
        assert_eq!(app.state.per_message_line_counts.len(), 1);
        let first_hash = app.state.per_message_hashes[0];
        let first_lines_snapshot = app.state.cached_lines.clone();

        // Append a second message
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "Second message".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        assert_eq!(
            app.state.per_message_hashes[0], first_hash,
            "first message hash should be unchanged after append"
        );
        assert_eq!(app.state.per_message_hashes.len(), 2);
        for i in 0..first_lines_snapshot.len() {
            assert_eq!(
                format!("{:?}", app.state.cached_lines[i]),
                format!("{:?}", first_lines_snapshot[i]),
                "first message lines should be preserved at index {i}"
            );
        }
        assert!(app.state.cached_lines.len() > lines_after_first);
    }

    #[test]
    fn test_incremental_modify_middle_rebuilds_from_change() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        for content in &["First", "Second", "Third"] {
            app.state.messages.push(DisplayMessage {
                role: DisplayRole::User,
                content: content.to_string(),
                tool_call: None,
                collapsed: false,
            });
        }
        app.rebuild_cached_lines();
        let original_lines = app.state.cached_lines.len();
        assert_eq!(app.state.per_message_hashes.len(), 3);
        let first_hash = app.state.per_message_hashes[0];
        let first_line_count = app.state.per_message_line_counts[0];

        // Modify the second message
        app.state.messages[1].content = "Modified Second".into();
        app.rebuild_cached_lines();

        // First message preserved
        assert_eq!(app.state.per_message_hashes[0], first_hash);
        assert_eq!(app.state.per_message_line_counts[0], first_line_count);
        assert_eq!(app.state.per_message_hashes.len(), 3);
        // Second hash changed
        assert_ne!(
            app.state.per_message_hashes[1],
            display_message_hash(&DisplayMessage {
                role: DisplayRole::User,
                content: "Second".into(),
                tool_call: None,
                collapsed: false,
            }),
        );
        assert_eq!(app.state.cached_lines.len(), original_lines);
    }

    #[test]
    fn test_incremental_empty_conversation() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.rebuild_cached_lines();
        assert!(app.state.cached_lines.is_empty());
        assert!(app.state.per_message_hashes.is_empty());
        assert!(app.state.per_message_line_counts.is_empty());
    }

    #[test]
    fn test_incremental_multiple_appends_correct_cache() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        for i in 0..5u32 {
            app.state.messages.push(DisplayMessage {
                role: if i % 2 == 0 {
                    DisplayRole::User
                } else {
                    DisplayRole::Assistant
                },
                content: format!("Message {i}"),
                tool_call: None,
                collapsed: false,
            });
            app.rebuild_cached_lines();
            assert_eq!(app.state.per_message_hashes.len(), (i + 1) as usize);
            assert_eq!(app.state.per_message_line_counts.len(), (i + 1) as usize);
        }
        // Compare with full rebuild
        let incremental_lines = app.state.cached_lines.clone();
        app.state.per_message_hashes.clear();
        app.state.per_message_line_counts.clear();
        app.state.cached_lines.clear();
        app.rebuild_cached_lines();
        assert_eq!(app.state.cached_lines.len(), incremental_lines.len());
        for (i, (inc, full)) in incremental_lines
            .iter()
            .zip(app.state.cached_lines.iter())
            .enumerate()
        {
            assert_eq!(
                format!("{:?}", inc),
                format!("{:?}", full),
                "line {i} differs between incremental and full rebuild"
            );
        }
    }

    #[test]
    fn test_incremental_no_change_is_noop() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "Hello".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        let lines_after = app.state.cached_lines.clone();
        // Second rebuild with no changes
        app.rebuild_cached_lines();
        assert_eq!(app.state.cached_lines.len(), lines_after.len());
    }

    #[test]
    fn test_incremental_message_removal() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "First".into(),
            tool_call: None,
            collapsed: false,
        });
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::User,
            content: "Second".into(),
            tool_call: None,
            collapsed: false,
        });
        app.rebuild_cached_lines();
        assert_eq!(app.state.per_message_hashes.len(), 2);
        app.state.messages.pop();
        app.rebuild_cached_lines();
        assert_eq!(app.state.per_message_hashes.len(), 1);
        assert_eq!(app.state.per_message_line_counts.len(), 1);
    }

    // -- Slash command argument parsing tests --
}
