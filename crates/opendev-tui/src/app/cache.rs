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
    msg.thinking_duration_secs.hash(&mut hasher);
    // For unfinalized reasoning, hash elapsed millis (tick-aligned) to drive
    // shimmer animation and elapsed timer updates without excessive re-renders
    if msg.thinking_duration_secs.is_none()
        && let Some(started) = msg.thinking_started_at
    {
        // ~30fps: changes every 33ms, matching the tick rate
        (started.elapsed().as_millis() / 33).hash(&mut hasher);
    }
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
    fn conversation_viewport_height(&self) -> usize {
        let has_todos = !self.state.todo_items.is_empty();
        let todo_height: u16 = if has_todos {
            if self.state.todo_expanded {
                (self.state.todo_items.len() as u16 + 3).min(12)
            } else {
                3
            }
        } else {
            0
        };
        let input_lines = self.state.input_buffer.matches('\n').count() + 1;
        let input_height = (input_lines as u16 + 1).min(8);
        let conv_height = self
            .state
            .terminal_height
            .saturating_sub(todo_height)
            .saturating_sub(input_height)
            .saturating_sub(2)
            .max(5);
        conv_height.saturating_sub(1) as usize
    }

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
        let content_width = self.state.terminal_width.saturating_sub(1);

        // Width-change detection: if terminal was resized, clear all caches
        if self.state.cached_width != content_width {
            self.state.cached_width = content_width;
            self.state.cached_lines.clear();
            self.state.per_message_hashes.clear();
            self.state.per_message_line_counts.clear();
            self.state.per_message_culled.clear();
            self.state.markdown_cache.clear();
        }

        // Compute per-message hashes for the current messages
        let new_hashes: Vec<u64> = self
            .state
            .messages
            .iter()
            .map(display_message_hash)
            .collect();

        // Find the first message index where the hash differs
        let mut first_dirty = {
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

        // --- Viewport culling ---
        let viewport_h = self.conversation_viewport_height();
        let mut buffer_lines = 100usize;
        if self.state.task_progress.is_some()
            || !self.state.active_tools.is_empty()
            || !self.state.active_subagents.is_empty()
            || self.state.agent_active
        {
            buffer_lines = buffer_lines.max(viewport_h.saturating_mul(4));
        }
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
                    use crate::formatters::tool_registry::{ResultFormat, lookup_tool};
                    let is_bash = lookup_tool(&tc.name).result_format == ResultFormat::Bash;
                    1 + if !tc.collapsed {
                        tc.result_lines.len()
                    } else if is_bash {
                        // Bash preview: ≤4 lines shown inline, >4 shows 5 (2+ellipsis+2), 0 shows 1
                        let n = tc.result_lines.len();
                        if n == 0 { 1 } else { n.min(4).max(if n > 4 { 5 } else { n }) }
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

        // Detect culling state changes (messages transitioning visible <-> culled).
        // When the user scrolls, previously-culled messages may enter the viewport
        // and need to be re-rendered from their blank placeholders.
        if self.state.per_message_culled.len() == num_messages {
            for (i, (new_vis, old_vis)) in msg_visible
                .iter()
                .zip(self.state.per_message_culled.iter())
                .enumerate()
            {
                if new_vis != old_vis {
                    first_dirty = first_dirty.min(i);
                    break;
                }
            }
        } else if !self.state.per_message_culled.is_empty() {
            // Length mismatch (messages added/removed) — already handled by hash check
            first_dirty = first_dirty.min(self.state.per_message_culled.len());
        }

        // Nothing changed (content hashes match AND culling state unchanged)
        if first_dirty >= num_messages && self.state.per_message_hashes.len() == num_messages {
            self.state.per_message_culled = msg_visible;
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
                    &self.state.path_shortener,
                    content_width,
                    self.state.spinner.tick_count(),
                );
            }

            let lines_produced = self.state.cached_lines.len() - lines_before;
            self.state.per_message_hashes.push(new_hashes[msg_idx]);
            self.state.per_message_line_counts.push(lines_produced);
        }

        self.state.per_message_culled = msg_visible;
    }

    /// Render a single `DisplayMessage` into styled lines, appending to `lines`.
    /// `next_role` is the role of the following message (if any), used to suppress
    /// the trailing blank line before messages that attach to the previous one.
    /// `content_width` is the available display width for word-wrapping (0 = no wrapping).
    pub(super) fn render_single_message(
        msg: &DisplayMessage,
        next_role: Option<&DisplayRole>,
        lines: &mut Vec<ratatui::text::Line<'static>>,
        markdown_cache: &mut HashMap<u64, Vec<ratatui::text::Line<'static>>>,
        shortener: &crate::formatters::PathShortener,
        content_width: u16,
        tick_count: u64,
    ) {
        use crate::formatters::display::strip_system_reminders;
        use crate::formatters::markdown::MarkdownRenderer;
        use crate::formatters::style_tokens::{self, Indent};
        use crate::formatters::tool_registry::{ResultFormat, format_tool_call_parts_short};
        use crate::formatters::wrap::wrap_spans_to_lines;
        use crate::widgets::conversation::build_bash_preview;
        use crate::widgets::spinner::{COMPLETED_CHAR, CONTINUATION_CHAR};
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let content = strip_system_reminders(&msg.content);
        if content.is_empty() && msg.tool_call.is_none() {
            return;
        }

        let max_w = content_width as usize;

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

                let first_prefix = vec![Span::styled(
                    format!("{} ", COMPLETED_CHAR),
                    Style::default().fg(style_tokens::GREEN_BRIGHT),
                )];
                let cont_prefix = vec![Span::raw(Indent::CONT)];

                if max_w > 0 {
                    let wrapped = wrap_spans_to_lines(md_lines, first_prefix, cont_prefix, max_w);
                    lines.extend(wrapped);
                } else {
                    // Fallback: no wrapping (width unknown)
                    let mut leading_consumed = false;
                    for md_line in md_lines {
                        let line_text: String = md_line
                            .spans
                            .iter()
                            .map(|s| s.content.to_string())
                            .collect();
                        let has_content = !line_text.trim().is_empty();

                        if !leading_consumed && has_content {
                            let mut spans = first_prefix.clone();
                            spans.extend(
                                md_line
                                    .spans
                                    .into_iter()
                                    .map(|s| Span::styled(s.content.to_string(), s.style)),
                            );
                            lines.push(Line::from(spans));
                            leading_consumed = true;
                        } else {
                            let mut spans = cont_prefix.clone();
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
            }
            DisplayRole::System => {
                let subtle_style = Style::default().fg(style_tokens::SUBTLE);
                for (i, line_text) in content.lines().enumerate() {
                    if i == 0 {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{} ", COMPLETED_CHAR),
                                Style::default().fg(style_tokens::WARNING),
                            ),
                            Span::styled(line_text.to_string(), subtle_style),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw(Indent::CONT),
                            Span::styled(line_text.to_string(), subtle_style),
                        ]));
                    }
                }
            }
            DisplayRole::User
            | DisplayRole::Interrupt
            | DisplayRole::SlashCommand
            | DisplayRole::CommandResult => {
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
            DisplayRole::Reasoning => {
                let thinking_style = Style::default().fg(style_tokens::THINKING_BG);

                if msg.collapsed {
                    if let Some(secs) = msg.thinking_duration_secs {
                        // Finalized collapsed: "⟡ Thought for Xs (Ctrl+I to expand)"
                        let duration_text = if secs == 0 {
                            "<1".to_string()
                        } else {
                            secs.to_string()
                        };
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!(
                                    "{} Thought for {}s",
                                    style_tokens::THINKING_ICON,
                                    duration_text
                                ),
                                thinking_style,
                            ),
                            Span::styled(
                                " (Ctrl+I to expand)",
                                Style::default().fg(style_tokens::SUBTLE),
                            ),
                        ]));
                    } else if let Some(started) = msg.thinking_started_at {
                        // Streaming: shimmer wave "⟡ Thinking... Xs (Ctrl+I to expand)"
                        let elapsed = started.elapsed().as_secs();
                        let text = format!(
                            "{} Thinking... {}s",
                            style_tokens::THINKING_ICON, elapsed
                        );
                        let highlight = ratatui::style::Color::Rgb(200, 200, 220);
                        let mut spans = style_tokens::shimmer_line(
                            &text,
                            tick_count,
                            style_tokens::THINKING_BG,
                            highlight,
                        );
                        spans.push(Span::styled(
                            " (Ctrl+I to expand)",
                            Style::default().fg(style_tokens::SUBTLE),
                        ));
                        lines.push(Line::from(spans));
                    }
                } else {
                    // Expanded: full markdown rendering (unchanged)
                    let cache_key = markdown_cache_key(&msg.role, &content);
                    let md_lines = if let Some(cached) = markdown_cache.get(&cache_key) {
                        cached.clone()
                    } else {
                        let rendered =
                            MarkdownRenderer::render_muted(&content, style_tokens::THINKING_BG);
                        markdown_cache.insert(cache_key, rendered.clone());
                        rendered
                    };

                    let first_prefix = vec![Span::styled(
                        format!("{} ", style_tokens::THINKING_ICON),
                        thinking_style,
                    )];
                    let cont_prefix = vec![Span::styled(Indent::THINKING_CONT, thinking_style)];

                    if max_w > 0 {
                        let wrapped =
                            wrap_spans_to_lines(md_lines, first_prefix, cont_prefix, max_w);
                        lines.extend(wrapped);
                    } else {
                        let mut leading_consumed = false;
                        for md_line in md_lines {
                            let line_text: String = md_line
                                .spans
                                .iter()
                                .map(|s| s.content.to_string())
                                .collect();
                            let has_content = !line_text.trim().is_empty();

                            if !leading_consumed && has_content {
                                let mut spans = first_prefix.clone();
                                spans.extend(
                                    md_line
                                        .spans
                                        .into_iter()
                                        .map(|s| Span::styled(s.content.to_string(), s.style)),
                                );
                                lines.push(Line::from(spans));
                                leading_consumed = true;
                            } else {
                                let mut spans = cont_prefix.clone();
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
                }
            }
            DisplayRole::Plan => {
                let border_style = Style::default().fg(style_tokens::CYAN);
                let border_w: usize = if max_w > 0 { max_w } else { 32 };
                let inner_w = border_w.saturating_sub(1); // leave room for right │
                let label = " Plan ";
                let top_after = border_w.saturating_sub(3 + label.len() + 1); // 3 = ╭── prefix, +1 for ╮ suffix

                // Top border: ╭── Plan ──────────────────╮
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}{}", style_tokens::BOX_TL, style_tokens::BOX_H.repeat(2)),
                        border_style,
                    ),
                    Span::styled(
                        label.to_string(),
                        border_style.add_modifier(ratatui::style::Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(
                            "{}{}",
                            style_tokens::BOX_H.repeat(top_after),
                            style_tokens::BOX_TR
                        ),
                        border_style,
                    ),
                ]));

                // Top padding
                lines.push(Line::from(vec![
                    Span::styled(style_tokens::BOX_V.to_string(), border_style),
                    Span::raw(" ".repeat(inner_w.saturating_sub(1))),
                    Span::styled(style_tokens::BOX_V.to_string(), border_style),
                ]));

                // Markdown content with left border prefix
                let cache_key = markdown_cache_key(&msg.role, &content);
                let md_lines = if let Some(cached) = markdown_cache.get(&cache_key) {
                    cached.clone()
                } else {
                    let rendered = MarkdownRenderer::render(&content);
                    markdown_cache.insert(cache_key, rendered.clone());
                    rendered
                };

                let prefix_str = format!("{}  ", style_tokens::BOX_V);
                let prefix_span = vec![Span::styled(prefix_str.clone(), border_style)];
                let cont_span = vec![Span::styled(prefix_str, border_style)];

                let wrap_width = if max_w > 0 { inner_w } else { 0 };
                let content_lines = if wrap_width > 0 {
                    wrap_spans_to_lines(md_lines, prefix_span, cont_span, wrap_width)
                } else {
                    let mut out = Vec::new();
                    for md_line in md_lines {
                        let mut spans = prefix_span.clone();
                        spans.extend(
                            md_line
                                .spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.to_string(), s.style)),
                        );
                        out.push(Line::from(spans));
                    }
                    out
                };

                // Add right border to each content line
                for mut line in content_lines {
                    if border_w > 0 {
                        let line_w = line.width();
                        let pad = inner_w.saturating_sub(line_w);
                        if pad > 0 {
                            line.spans.push(Span::raw(" ".repeat(pad)));
                        }
                        line.spans
                            .push(Span::styled(style_tokens::BOX_V.to_string(), border_style));
                    }
                    lines.push(line);
                }

                // Bottom padding
                lines.push(Line::from(vec![
                    Span::styled(style_tokens::BOX_V.to_string(), border_style),
                    Span::raw(" ".repeat(inner_w.saturating_sub(1))),
                    Span::styled(style_tokens::BOX_V.to_string(), border_style),
                ]));

                // Bottom border: ╰──────────────────────────╯
                lines.push(Line::from(vec![Span::styled(
                    format!(
                        "{}{}{}",
                        style_tokens::BOX_BL,
                        style_tokens::BOX_H.repeat(border_w.saturating_sub(2)),
                        style_tokens::BOX_BR
                    ),
                    border_style,
                )]));
            }
        }

        // Tool call summary
        if let Some(ref tc) = msg.tool_call {
            let (icon, icon_color) = if tc.success {
                (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
            } else {
                (COMPLETED_CHAR, style_tokens::ERROR)
            };
            let (verb, arg) = format_tool_call_parts_short(&tc.name, &tc.arguments, shortener);
            lines.push(Line::from(vec![
                Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
                Span::styled(
                    verb,
                    Style::default()
                        .fg(style_tokens::PRIMARY)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {arg}"), Style::default().fg(style_tokens::SUBTLE)),
            ]));

            // Diff tools are never collapsed
            use crate::widgets::conversation::{
                is_diff_tool, parse_unified_diff, render_diff_entries,
            };
            let is_bash =
                crate::formatters::tool_registry::lookup_tool(&tc.name).result_format
                    == ResultFormat::Bash;
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
                        let shortened = shortener.shorten_text(result_line);
                        lines.push(Line::from(vec![
                            Span::styled(prefix_char, Style::default().fg(style_tokens::SUBTLE)),
                            Span::styled(shortened, Style::default().fg(style_tokens::SUBTLE)),
                        ]));
                    }
                }
            } else if effective_collapsed {
                if is_bash {
                    lines.extend(build_bash_preview(&tc.result_lines, tc.success));
                } else if !tc.result_lines.is_empty() {
                    let count = tc.result_lines.len();
                    let verb = crate::formatters::tool_registry::lookup_tool(&tc.name).verb;
                    let label = format!("  {}  {verb} {count} lines", CONTINUATION_CHAR);
                    lines.push(Line::from(Span::styled(
                        label,
                        Style::default().fg(style_tokens::SUBTLE),
                    )));
                }
            } else if tc.result_lines.is_empty() && is_bash {
                lines.extend(build_bash_preview(&tc.result_lines, tc.success));
            }

            for nested in &tc.nested_calls {
                let (n_icon, n_icon_color) = if nested.success {
                    (COMPLETED_CHAR, style_tokens::GREEN_BRIGHT)
                } else {
                    (COMPLETED_CHAR, style_tokens::ERROR)
                };
                let (n_verb, n_arg) =
                    format_tool_call_parts_short(&nested.name, &nested.arguments, shortener);
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
                        format!(" {n_arg}"),
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
            app.state.messages.push(DisplayMessage::new(
                DisplayRole::User,
                format!("Message {i}"),
            ));
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
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::Assistant,
            "Hello **world**",
        ));
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
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::Assistant,
            "Hello **world**",
        ));
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
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::Assistant,
            "# Title\nSome text",
        ));
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
        app.state
            .messages
            .push(DisplayMessage::new(DisplayRole::User, "First message"));
        app.rebuild_cached_lines();
        let lines_after_first = app.state.cached_lines.len();
        assert!(lines_after_first > 0);
        assert_eq!(app.state.per_message_hashes.len(), 1);
        assert_eq!(app.state.per_message_line_counts.len(), 1);
        let first_hash = app.state.per_message_hashes[0];
        let first_lines_snapshot = app.state.cached_lines.clone();

        // Append a second message
        app.state
            .messages
            .push(DisplayMessage::new(DisplayRole::User, "Second message"));
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
            app.state
                .messages
                .push(DisplayMessage::new(DisplayRole::User, content.to_string()));
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
            display_message_hash(&DisplayMessage::new(DisplayRole::User, "Second")),
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
            app.state.messages.push(DisplayMessage::new(
                if i % 2 == 0 {
                    DisplayRole::User
                } else {
                    DisplayRole::Assistant
                },
                format!("Message {i}"),
            ));
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
        app.state
            .messages
            .push(DisplayMessage::new(DisplayRole::User, "Hello"));
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
        app.state
            .messages
            .push(DisplayMessage::new(DisplayRole::User, "First"));
        app.state
            .messages
            .push(DisplayMessage::new(DisplayRole::User, "Second"));
        app.rebuild_cached_lines();
        assert_eq!(app.state.per_message_hashes.len(), 2);
        app.state.messages.pop();
        app.rebuild_cached_lines();
        assert_eq!(app.state.per_message_hashes.len(), 1);
        assert_eq!(app.state.per_message_line_counts.len(), 1);
    }

    #[test]
    fn test_reasoning_message_produces_lines() {
        let mut app = App::new();
        app.state.terminal_height = 24;
        app.state.terminal_width = 120;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Reasoning,
            content: "Let me think about this.\nFirst, I need to understand.\nThen solve.".into(),
            tool_call: None,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: Some(5),
        });
        app.rebuild_cached_lines();

        // Should have produced lines
        assert!(
            !app.state.cached_lines.is_empty(),
            "reasoning message should produce cached lines"
        );
        assert!(
            app.state.cached_lines.len() >= 3,
            "expected at least 3 lines (3 content + blank), got {}",
            app.state.cached_lines.len()
        );

        // Check that first line has ⟡ prefix
        let first_line = &app.state.cached_lines[0];
        let first_text: String = first_line
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            first_text.contains('⟡'),
            "first line should have ⟡ icon, got: {first_text}"
        );

        // Check that continuation lines have │ prefix
        for (i, line) in app.state.cached_lines.iter().enumerate().skip(1) {
            let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
            if text.trim().is_empty() {
                continue; // blank separator line
            }
            assert!(
                text.starts_with('│'),
                "line {i} should start with │, got: {text:?}"
            );
        }

        // Content should be preserved
        let all_text: String = app
            .state
            .cached_lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            all_text.contains("think about this"),
            "content lost: {all_text}"
        );
    }

    #[test]
    fn test_reasoning_via_cached_lines_widget() {
        use crate::widgets::conversation::ConversationWidget;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new();
        app.state.terminal_width = 80;
        app.state.terminal_height = 24;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Reasoning,
            content: "Thinking carefully about the problem at hand.".into(),
            tool_call: None,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: Some(5),
        });
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::Assistant,
            "The result is clear.",
        ));
        app.state.message_generation = 1;
        app.rebuild_cached_lines();

        assert!(
            !app.state.cached_lines.is_empty(),
            "cached lines should not be empty after rebuild"
        );

        // Render with cached lines through the widget
        let cached = app.state.cached_lines.clone();
        let msgs = app.state.messages.clone();
        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0).cached_lines(&cached);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let mut all_text = String::new();
        for y in 0..24u16 {
            for x in 0..80u16 {
                if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y)) {
                    all_text.push_str(cell.symbol());
                }
            }
            all_text.push('\n');
        }

        assert!(
            all_text.contains("Thinking carefully"),
            "Cached reasoning content missing from rendered buffer.\nBuffer:\n{all_text}"
        );
        assert!(
            all_text.contains("result is clear"),
            "Cached assistant content missing from rendered buffer.\nBuffer:\n{all_text}"
        );
    }

    #[test]
    fn test_active_subagents_keep_recent_reasoning_lines_cached() {
        let mut app = App::new();
        app.state.terminal_width = 60;
        app.state.terminal_height = 12;
        for i in 0..40 {
            app.state.messages.push(DisplayMessage::new(
                DisplayRole::User,
                format!("Older message {i}"),
            ));
        }
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Reasoning,
            content: "Thinking through how to split this work safely.".into(),
            tool_call: None,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: Some(5),
        });
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::Assistant,
            "I will spawn 2 agents to explore the codebase.",
        ));
        app.state.agent_active = true;
        app.state.active_tools.push(ToolExecution {
            id: "t0".into(),
            name: "spawn_subagent".into(),
            output_lines: vec![],
            state: ToolState::Running,
            elapsed_secs: 1,
            started_at: std::time::Instant::now(),
            tick_count: 0,
            parent_id: None,
            depth: 0,
            args: std::collections::HashMap::new(),
        });
        app.state.message_generation = 1;
        app.rebuild_cached_lines();

        let all_text: String = app
            .state
            .cached_lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            all_text.contains("Thinking through"),
            "recent reasoning line was culled while tools were active: {all_text}"
        );
        assert!(
            all_text.contains("spawn 2 agents"),
            "recent assistant line was culled while tools were active: {all_text}"
        );
    }

    #[test]
    fn test_reasoning_to_subagent_transition_remains_visible_in_short_tui() {
        use crate::widgets::conversation::ConversationWidget;
        use crate::widgets::nested_tool::SubagentDisplayState;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(60, 16);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new();
        app.state.terminal_width = 60;
        app.state.terminal_height = 16;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Reasoning,
            content: "Thinking through the codebase structure.".into(),
            tool_call: None,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: Some(5),
        });
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::Assistant,
            "I will spawn 2 agents to explore the codebase.",
        ));

        let tasks = ["Inspect auth flow", "Trace API routes"];
        let tools: Vec<ToolExecution> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut args = std::collections::HashMap::new();
                args.insert("task".into(), serde_json::Value::String(task.to_string()));
                args.insert(
                    "description".into(),
                    serde_json::Value::String(task.to_string()),
                );
                ToolExecution {
                    id: format!("t{i}"),
                    name: "spawn_subagent".into(),
                    output_lines: vec![],
                    state: ToolState::Running,
                    elapsed_secs: 2,
                    started_at: std::time::Instant::now(),
                    tick_count: 0,
                    parent_id: None,
                    depth: 0,
                    args,
                }
            })
            .collect();
        let subagents: Vec<SubagentDisplayState> = tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let mut sa =
                    SubagentDisplayState::new(format!("sa{i}"), "explore".into(), task.to_string());
                sa.parent_tool_id = Some(format!("t{i}"));
                sa
            })
            .collect();

        app.state.active_tools = tools.clone();
        app.state.active_subagents = subagents.clone();
        app.state.message_generation = 1;
        app.rebuild_cached_lines();

        let cached = app.state.cached_lines.clone();
        let msgs = app.state.messages.clone();
        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0)
                    .cached_lines(&cached)
                    .active_tools(&tools)
                    .active_subagents(&subagents);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let mut all_text = String::new();
        for y in 0..16u16 {
            for x in 0..60u16 {
                if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y)) {
                    all_text.push_str(cell.symbol());
                }
            }
            all_text.push('\n');
        }

        assert!(
            all_text.contains("spawn 2 agents"),
            "assistant handoff text disappeared from TUI.\nBuffer:\n{all_text}"
        );
        // Each subagent should be rendered individually (no grouping)
        assert!(
            all_text.contains("Inspect auth flow") || all_text.contains("Trace API routes"),
            "individual subagent spinners missing from TUI.\nBuffer:\n{all_text}"
        );
        assert!(
            !all_text.contains("2 subagents"),
            "subagents should not be grouped.\nBuffer:\n{all_text}"
        );
    }

    #[test]
    fn test_event_sequence_reasoning_then_spawn_subagent_keeps_context() {
        use crate::event::AppEvent;

        let mut app = App::new();
        app.state.terminal_width = 60;
        app.state.terminal_height = 8;

        app.handle_event(AppEvent::ReasoningContent(
            "Thinking through the codebase structure.".into(),
        ));
        app.handle_event(AppEvent::AgentChunk(
            "I will spawn 2 agents to explore the codebase.".into(),
        ));
        app.handle_event(AppEvent::ToolStarted {
            tool_id: "t1".into(),
            tool_name: "spawn_subagent".into(),
            args: {
                let mut args = std::collections::HashMap::new();
                args.insert(
                    "task".into(),
                    serde_json::Value::String("Inspect auth flow".into()),
                );
                args.insert(
                    "description".into(),
                    serde_json::Value::String("Inspect auth flow".into()),
                );
                args
            },
        });
        app.handle_event(AppEvent::ToolStarted {
            tool_id: "t2".into(),
            tool_name: "spawn_subagent".into(),
            args: {
                let mut args = std::collections::HashMap::new();
                args.insert(
                    "task".into(),
                    serde_json::Value::String("Trace API routes".into()),
                );
                args.insert(
                    "description".into(),
                    serde_json::Value::String("Trace API routes".into()),
                );
                args
            },
        });

        app.rebuild_cached_lines();
        let all_text: String = app
            .state
            .cached_lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            all_text.contains("Thought for"),
            "collapsed reasoning context disappeared after ToolStarted events: {all_text}"
        );
        assert!(
            all_text.contains("spawn 2 agents"),
            "assistant handoff disappeared after ToolStarted events: {all_text}"
        );
        assert_eq!(app.state.active_tools.len(), 2);
        assert_eq!(app.state.active_subagents.len(), 2);
    }

    #[test]
    fn test_25_subagents_render_to_terminal() {
        use crate::widgets::conversation::ConversationWidget;
        use crate::widgets::nested_tool::SubagentDisplayState;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let backend = TestBackend::new(120, 80);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new();
        app.state.terminal_width = 120;
        app.state.terminal_height = 80;
        app.state.messages.push(DisplayMessage {
            role: DisplayRole::Reasoning,
            content: "Planning 25 parallel explorations.".into(),
            tool_call: None,
            collapsed: false,
            thinking_started_at: None,
            thinking_duration_secs: Some(5),
        });
        app.state.messages.push(DisplayMessage::new(
            DisplayRole::Assistant,
            "I will spawn 25 agents to explore the codebase.",
        ));

        let tools: Vec<ToolExecution> = (0..25)
            .map(|i| {
                let mut args = std::collections::HashMap::new();
                args.insert(
                    "task".into(),
                    serde_json::Value::String(format!("Explore area {i}")),
                );
                args.insert(
                    "description".into(),
                    serde_json::Value::String(format!("Explore area {i}")),
                );
                ToolExecution {
                    id: format!("t{i}"),
                    name: "spawn_subagent".into(),
                    output_lines: vec![],
                    state: ToolState::Running,
                    elapsed_secs: 2,
                    started_at: std::time::Instant::now(),
                    tick_count: 0,
                    parent_id: None,
                    depth: 0,
                    args,
                }
            })
            .collect();

        let subagents: Vec<SubagentDisplayState> = (0..25)
            .map(|i| {
                let mut sa = SubagentDisplayState::new(
                    format!("sa{i}"),
                    "explore".into(),
                    format!("Explore area {i}"),
                );
                sa.parent_tool_id = Some(format!("t{i}"));
                sa
            })
            .collect();

        app.state.active_tools = tools.clone();
        app.state.active_subagents = subagents.clone();
        app.state.agent_active = true;
        app.state.message_generation = 1;
        app.rebuild_cached_lines();

        let cached = app.state.cached_lines.clone();
        let msgs = app.state.messages.clone();
        terminal
            .draw(|frame| {
                let widget = ConversationWidget::new(&msgs, 0)
                    .cached_lines(&cached)
                    .active_tools(&tools)
                    .active_subagents(&subagents);
                frame.render_widget(widget, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let mut all_text = String::new();
        for y in 0..80u16 {
            for x in 0..120u16 {
                if let Some(cell) = buf.cell(ratatui::layout::Position::new(x, y)) {
                    all_text.push_str(cell.symbol());
                }
            }
            all_text.push('\n');
        }

        // At least 20 of the 25 agent descriptions should appear (accounting for scroll)
        let mut found = 0;
        for i in 0..25 {
            if all_text.contains(&format!("Explore area {i}")) {
                found += 1;
            }
        }
        assert!(
            found >= 20,
            "expected at least 20 of 25 agent descriptions visible, found {found}.\nBuffer:\n{all_text}"
        );

        // No grouping text
        assert!(
            !all_text.contains("25 subagents"),
            "should not contain grouped subagent text.\nBuffer:\n{all_text}"
        );

        // Assistant handoff text should still be visible
        assert!(
            all_text.contains("spawn 25 agents"),
            "assistant handoff text should be visible.\nBuffer:\n{all_text}"
        );
    }

    // -- Slash command argument parsing tests --
}
