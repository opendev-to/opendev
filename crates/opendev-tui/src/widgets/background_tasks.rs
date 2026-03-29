//! Task Watcher overlay panel — dynamic grid of scrollable cells.
//!
//! Each active task gets its own rounded-box cell with title, scrollable content, and footer.
//! The grid adapts to the number of active tasks and terminal size.

use std::collections::HashSet;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Widget},
};

use crate::formatters::PathShortener;
use crate::formatters::style_tokens;
use crate::formatters::tool_registry::format_tool_call_parts_short;
use crate::managers::BackgroundAgentManager;
use crate::widgets::nested_tool::SubagentDisplayState;
use crate::widgets::spinner::{COMPLETED_CHAR, FAILURE_CHAR, SPINNER_FRAMES, SUCCESS_CHAR};

/// Minimum cell width in the grid.
const MIN_W: u16 = 30;
/// Minimum cell height (2 border + 1 title + 3 content + 1 footer).
const MIN_H: u16 = 7;

/// Compute grid column count for given task count and available width.
/// Used by both the widget renderer and key_handler for navigation.
pub fn compute_grid_cols(task_count: usize, available_width: u16) -> usize {
    if task_count == 0 {
        return 1;
    }
    let max_cols = (available_width / MIN_W).max(1) as usize;
    task_count.min(max_cols)
}

/// Centered overlay widget showing a dynamic grid of task cells.
pub struct TaskWatcherPanel<'a> {
    subagents: &'a [SubagentDisplayState],
    bg_manager: &'a BackgroundAgentManager,
    /// bg_agent_manager task IDs that are "covered" by backgrounded subagents
    /// (shown as individual subagent panels instead of the parent panel).
    covered_bg_task_ids: &'a HashSet<String>,
    spinner_tick: usize,
    shortener: &'a PathShortener,
    focus: usize,
    cell_scrolls: &'a [usize],
    page: usize,
}

impl<'a> TaskWatcherPanel<'a> {
    pub fn new(
        subagents: &'a [SubagentDisplayState],
        bg_manager: &'a BackgroundAgentManager,
        covered_bg_task_ids: &'a HashSet<String>,
        spinner_tick: usize,
        shortener: &'a PathShortener,
    ) -> Self {
        Self {
            subagents,
            bg_manager,
            covered_bg_task_ids,
            spinner_tick,
            shortener,
            focus: 0,
            cell_scrolls: &[],
            page: 0,
        }
    }

    pub fn focus(mut self, focus: usize) -> Self {
        self.focus = focus;
        self
    }

    pub fn cell_scrolls(mut self, scrolls: &'a [usize]) -> Self {
        self.cell_scrolls = scrolls;
        self
    }

    pub fn page(mut self, page: usize) -> Self {
        self.page = page;
        self
    }

    /// Total number of tasks across all sources (excluding covered parent tasks).
    fn total_tasks(&self) -> usize {
        self.subagents.len() + self.filtered_bg_tasks().len()
    }

    /// bg_manager tasks excluding those covered by backgrounded subagents.
    fn filtered_bg_tasks(&self) -> Vec<&crate::managers::background_agents::BackgroundAgentTask> {
        self.bg_manager
            .all_tasks()
            .into_iter()
            .filter(|t| !t.hidden && !self.covered_bg_task_ids.contains(&t.task_id))
            .collect()
    }
}

/// A single activity line with split icon / verb / args styling.
/// Matches the foreground agent style: colored icon, bold verb, subtle args.
struct ActivityLine {
    icon: String,
    icon_color: ratatui::style::Color,
    verb: String,
    args: String,
}

/// Data needed to render a single cell.
struct TaskCellData {
    title: String,
    icon: String,
    icon_color: ratatui::style::Color,
    activity: Vec<ActivityLine>,
    footer: String,
    footer_color: ratatui::style::Color,
    is_focused: bool,
    is_running: bool,
    scroll_offset: usize,
}

impl Widget for TaskWatcherPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let total = self.total_tasks();

        // Build outer border title
        let filtered_bg = self.filtered_bg_tasks();
        let filtered_bg_running = filtered_bg
            .iter()
            .filter(|t| {
                t.state == crate::managers::background_agents::BackgroundAgentState::Running
            })
            .count();
        let running_count =
            self.subagents.iter().filter(|s| !s.finished).count() + filtered_bg_running;
        let done_count = total.saturating_sub(running_count);
        let spinner_ch = if running_count > 0 {
            let idx = self.spinner_tick % SPINNER_FRAMES.len();
            SPINNER_FRAMES[idx]
        } else {
            SUCCESS_CHAR
        };
        let title_str = format!(
            " {spinner_ch} Task Watcher \u{00b7} {running_count} running, {done_count} done "
        );

        let help_text = if total > 0 {
            " q:close  hjkl:focus  J/K:scroll  x:kill "
        } else {
            " q/Esc:close "
        };

        let outer_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(style_tokens::ACCENT))
            .title(Span::styled(
                title_str,
                Style::default()
                    .fg(style_tokens::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ))
            .title_bottom(Line::from(Span::styled(
                help_text,
                Style::default().fg(style_tokens::SUBTLE),
            )));

        let inner = outer_block.inner(area);
        outer_block.render(area, buf);

        if total == 0 {
            if inner.height > 0 && inner.width > 10 {
                let line = Line::from(Span::styled(
                    "  No active tasks.",
                    Style::default().fg(style_tokens::SUBTLE),
                ));
                buf.set_line(inner.x, inner.y, &line, inner.width);
            }
            return;
        }

        // Grid layout computation
        let cols = compute_grid_cols(total, inner.width);
        let mut rows = ceil_div(total, cols);
        let max_rows = (inner.height / MIN_H).max(1) as usize;

        if rows > max_rows {
            rows = max_rows;
        }

        let visible = cols * rows;
        let page_offset = (self.page * visible).min(total.saturating_sub(visible));

        // Build cell data for visible tasks
        for slot in 0..visible {
            let task_idx = page_offset + slot;
            if task_idx >= total {
                break;
            }

            let col = slot % cols;
            let row = slot / cols;
            let cell_area = cell_rect(col, row, inner, cols, rows);

            if cell_area.width < 4 || cell_area.height < 3 {
                continue;
            }

            let scroll_offset = self.cell_scrolls.get(task_idx).copied().unwrap_or(0);
            let is_focused = task_idx == self.focus;

            let data = if task_idx < self.subagents.len() {
                build_subagent_cell(
                    &self.subagents[task_idx],
                    self.spinner_tick,
                    self.shortener,
                    is_focused,
                    scroll_offset,
                )
            } else {
                let bg_idx = task_idx - self.subagents.len();
                if bg_idx < filtered_bg.len() {
                    build_bg_agent_cell(
                        filtered_bg[bg_idx],
                        self.spinner_tick,
                        is_focused,
                        scroll_offset,
                    )
                } else {
                    continue;
                }
            };

            render_cell(&data, cell_area, buf);
        }

        // Page indicator when tasks exceed grid
        if total > visible {
            let page_num = page_offset / visible + 1;
            let total_pages = ceil_div(total, visible);
            let hint = format!(" page {page_num}/{total_pages} ({total} tasks) ");
            let hint_x = inner.x + inner.width.saturating_sub(hint.len() as u16 + 1);
            let hint_y = inner.y + inner.height.saturating_sub(1);
            buf.set_string(
                hint_x,
                hint_y,
                &hint,
                Style::default()
                    .fg(style_tokens::SUBTLE)
                    .add_modifier(Modifier::ITALIC),
            );
        }
    }
}

/// Compute the rectangle for a cell at the given grid position.
fn cell_rect(col: usize, row: usize, inner: Rect, cols: usize, rows: usize) -> Rect {
    let base_w = inner.width / cols as u16;
    let extra_w = inner.width % cols as u16;
    let base_h = inner.height / rows as u16;
    let extra_h = inner.height % rows as u16;

    let x = inner.x
        + (0..col as u16)
            .map(|c| base_w + if c < extra_w { 1 } else { 0 })
            .sum::<u16>();
    let w = base_w + if (col as u16) < extra_w { 1 } else { 0 };
    let y = inner.y
        + (0..row as u16)
            .map(|r| base_h + if r < extra_h { 1 } else { 0 })
            .sum::<u16>();
    let h = base_h + if (row as u16) < extra_h { 1 } else { 0 };

    Rect::new(x, y, w, h)
}

/// Render a single cell in the grid.
fn render_cell(data: &TaskCellData, area: Rect, buf: &mut Buffer) {
    let (border_color, border_type) = if data.is_focused {
        (style_tokens::ACCENT, BorderType::Double)
    } else if !data.is_running {
        if data.footer_color == style_tokens::ERROR {
            (style_tokens::ERROR, BorderType::Plain)
        } else {
            // Completed cells get a subtle green-tinted border
            (style_tokens::SUCCESS, BorderType::Plain)
        }
    } else {
        (style_tokens::DIM_GREY, BorderType::Plain)
    };

    let title_mod = if data.is_focused {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(vec![
            Span::styled(
                format!(" {} ", data.icon),
                Style::default().fg(data.icon_color),
            ),
            Span::styled(
                truncate_str(&data.title, area.width.saturating_sub(8) as usize),
                Style::default()
                    .fg(style_tokens::PRIMARY)
                    .add_modifier(title_mod),
            ),
            Span::raw(" "),
        ]))
        .title_bottom(Line::from(Span::styled(
            format!(
                " {} ",
                truncate_str(&data.footer, area.width.saturating_sub(4) as usize)
            ),
            Style::default().fg(data.footer_color),
        )));

    let content_area = block.inner(area);
    block.render(area, buf);

    if content_area.height == 0 || content_area.width < 3 {
        return;
    }

    // 1-char padding on left/right for breathing room
    let padded = Rect::new(
        content_area.x + 1,
        content_area.y,
        content_area.width.saturating_sub(2),
        content_area.height,
    );

    if padded.width == 0 {
        return;
    }

    let visible_h = padded.height as usize;
    let total_lines = data.activity.len();

    // Compute visible window (default: auto-scroll to bottom, scroll_offset scrolls up)
    let scroll_up = data
        .scroll_offset
        .min(total_lines.saturating_sub(visible_h));
    let end = total_lines.saturating_sub(scroll_up);
    let start = end.saturating_sub(visible_h);

    // Scroll-up indicator
    let (render_start_y, render_count) = if start > 0 {
        let indicator = format!("\u{2191} {} more", start);
        buf.set_string(
            padded.x,
            padded.y,
            truncate_str(&indicator, padded.width as usize),
            Style::default()
                .fg(style_tokens::SUBTLE)
                .add_modifier(Modifier::ITALIC),
        );
        (padded.y + 1, visible_h.saturating_sub(1))
    } else {
        (padded.y, visible_h)
    };

    // Render visible activity lines: colored icon + bold verb + subtle args
    for (i, line) in data.activity[start..end]
        .iter()
        .take(render_count)
        .enumerate()
    {
        let y = render_start_y + i as u16;
        let max_w = padded.width as usize;
        let mut x = padded.x;
        let mut remaining = max_w;

        // 1) Icon segment (colored)
        let icon = line.icon.replace('\n', " ");
        let icon_w = icon.len().min(remaining);
        if icon_w > 0 {
            buf.set_string(x, y, &icon[..icon_w], Style::default().fg(line.icon_color));
            x += icon_w as u16;
            remaining -= icon_w;
        }

        // 2) Verb segment (bold PRIMARY)
        let verb = line.verb.replace('\n', " ");
        let verb_w = verb.len().min(remaining);
        if verb_w > 0 {
            buf.set_string(
                x,
                y,
                &verb[..verb_w],
                Style::default()
                    .fg(style_tokens::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            );
            x += verb_w as u16;
            remaining -= verb_w;
        }

        // 3) Args segment (subtle grey)
        if remaining > 0 && !line.args.is_empty() {
            let args = line.args.replace('\n', " ");
            let args_display = format!(" {args}");
            let args_truncated = truncate_str(&args_display, remaining);
            buf.set_string(
                x,
                y,
                &args_truncated,
                Style::default().fg(style_tokens::SUBTLE),
            );
        }
    }
}

/// Parse a plain-text activity line into a structured `ActivityLine`.
/// Used for bg_agent activity logs which arrive as plain strings.
fn parse_activity_line(line: &str) -> ActivityLine {
    let trimmed = line.trim_start();

    // Extract icon character and determine color
    let (icon_str, icon_color, rest) = if trimmed.starts_with('\u{25b8}') {
        // Running: ▸ ⠙ verb(arg) 3s — grab ▸ + spinner char
        let after = trimmed.trim_start_matches('\u{25b8}').trim_start();
        let icon_end = after.find(' ').unwrap_or(after.len());
        let spinner_part = &after[..icon_end];
        let text_part = after[icon_end..].trim_start();
        (
            format!("{spinner_part} "),
            style_tokens::BLUE_BRIGHT,
            text_part,
        )
    } else if trimmed.starts_with(SUCCESS_CHAR) || trimmed.starts_with(COMPLETED_CHAR) {
        let ch = trimmed.chars().next().unwrap();
        let rest = trimmed[ch.len_utf8()..].trim_start();
        (
            format!("{COMPLETED_CHAR} "),
            style_tokens::GREEN_BRIGHT,
            rest,
        )
    } else if trimmed.starts_with(FAILURE_CHAR) {
        let rest = trimmed[FAILURE_CHAR.len_utf8()..].trim_start();
        (format!("{COMPLETED_CHAR} "), style_tokens::ERROR, rest)
    } else if trimmed.starts_with('\u{27e1}') {
        let rest = trimmed['\u{27e1}'.len_utf8()..].trim_start();
        (format!("{COMPLETED_CHAR} "), style_tokens::SUBTLE, rest)
    } else {
        return ActivityLine {
            icon: String::new(),
            icon_color: style_tokens::PRIMARY,
            verb: String::new(),
            args: trimmed.to_string(),
        };
    };

    // Split rest into verb (first word) and args (remainder)
    let (verb, args) = match rest.find(['(', ' ']) {
        Some(i) => (rest[..i].to_string(), rest[i..].to_string()),
        None => (rest.to_string(), String::new()),
    };

    ActivityLine {
        icon: icon_str,
        icon_color,
        verb,
        args,
    }
}

/// Build cell data from a SubagentDisplayState.
fn build_subagent_cell(
    sa: &SubagentDisplayState,
    spinner_tick: usize,
    shortener: &PathShortener,
    is_focused: bool,
    scroll_offset: usize,
) -> TaskCellData {
    let (icon, icon_color) = if sa.finished {
        if sa.success {
            (SUCCESS_CHAR.to_string(), style_tokens::SUCCESS)
        } else {
            (FAILURE_CHAR.to_string(), style_tokens::ERROR)
        }
    } else {
        let slow_tick = spinner_tick / 3;
        let idx = slow_tick % SPINNER_FRAMES.len();
        (SPINNER_FRAMES[idx].to_string(), style_tokens::BLUE_BRIGHT)
    };

    let label = sa.display_label();
    let title = format!("{}: {}", sa.name, label);

    // Build activity lines
    let mut activity: Vec<ActivityLine> = Vec::new();

    for completed in &sa.completed_tools {
        let (verb, arg) =
            format_tool_call_parts_short(&completed.tool_name, &completed.args, shortener);
        let ic = if completed.success {
            style_tokens::GREEN_BRIGHT
        } else {
            style_tokens::ERROR
        };
        activity.push(ActivityLine {
            icon: format!("{COMPLETED_CHAR} "),
            icon_color: ic,
            verb,
            args: arg,
        });
    }

    for tool_state in sa.active_tools.values() {
        let (verb, arg) =
            format_tool_call_parts_short(&tool_state.tool_name, &tool_state.args, shortener);
        let slow_tick = tool_state.tick / 3;
        let spinner_idx = slow_tick % SPINNER_FRAMES.len();
        let spinner_ch = SPINNER_FRAMES[spinner_idx];
        let elapsed = tool_state.started_at.elapsed().as_secs();
        let elapsed_str = if elapsed > 0 {
            format!(" {elapsed}s")
        } else {
            String::new()
        };
        activity.push(ActivityLine {
            icon: format!("{spinner_ch} "),
            icon_color: style_tokens::BLUE_BRIGHT,
            verb,
            args: format!("{arg}{elapsed_str}"),
        });
    }

    // Footer
    let elapsed = sa.elapsed_secs();
    let elapsed_str = if elapsed >= 60 {
        format!("{}m {}s", elapsed / 60, elapsed % 60)
    } else {
        format!("{elapsed}s")
    };
    let tool_count = sa.completed_tools.len() + sa.active_tools.len();
    let status_str = if sa.finished {
        if sa.success { "Done" } else { "Failed" }
    } else {
        "Working\u{2026}"
    };
    let footer = format!("{status_str} · {elapsed_str} · {tool_count} tools");
    let footer_color = if sa.finished && !sa.success {
        style_tokens::ERROR
    } else if sa.finished {
        style_tokens::SUCCESS
    } else {
        style_tokens::SUBTLE
    };

    TaskCellData {
        title,
        icon,
        icon_color,
        activity,
        footer,
        footer_color,
        is_focused,
        is_running: !sa.finished,
        scroll_offset,
    }
}

/// Build cell data from a BackgroundAgentTask.
fn build_bg_agent_cell(
    task: &crate::managers::background_agents::BackgroundAgentTask,
    spinner_tick: usize,
    is_focused: bool,
    scroll_offset: usize,
) -> TaskCellData {
    let (icon, icon_color) = match task.state {
        crate::managers::background_agents::BackgroundAgentState::Running => {
            let slow_tick = spinner_tick / 3;
            let idx = slow_tick % SPINNER_FRAMES.len();
            (SPINNER_FRAMES[idx].to_string(), style_tokens::BLUE_BRIGHT)
        }
        crate::managers::background_agents::BackgroundAgentState::Completed => {
            (SUCCESS_CHAR.to_string(), style_tokens::SUCCESS)
        }
        crate::managers::background_agents::BackgroundAgentState::Failed => {
            (FAILURE_CHAR.to_string(), style_tokens::ERROR)
        }
        crate::managers::background_agents::BackgroundAgentState::Killed => {
            (FAILURE_CHAR.to_string(), style_tokens::WARNING)
        }
    };

    let title = format!("A: {}", task.query);
    let mut activity: Vec<ActivityLine> = task
        .activity_log
        .iter()
        .map(|line| parse_activity_line(line))
        .collect();

    // Append cleaned error summary for failed/killed tasks
    if matches!(
        task.state,
        crate::managers::background_agents::BackgroundAgentState::Failed
            | crate::managers::background_agents::BackgroundAgentState::Killed
    ) && let Some(ref summary) = task.result_summary
    {
        let clean: String = summary.split_whitespace().collect::<Vec<_>>().join(" ");
        let truncated = if clean.len() > 200 {
            format!("{}...", &clean[..197])
        } else {
            clean
        };
        activity.push(ActivityLine {
            icon: format!("{COMPLETED_CHAR} "),
            icon_color: style_tokens::ERROR,
            verb: String::new(),
            args: truncated,
        });
    }

    let elapsed = task.runtime_seconds();
    let elapsed_str = if elapsed >= 60.0 {
        format!("{}m {}s", elapsed as u64 / 60, elapsed as u64 % 60)
    } else {
        format!("{:.0}s", elapsed)
    };
    let status_str = match task.state {
        crate::managers::background_agents::BackgroundAgentState::Running => "Working\u{2026}",
        crate::managers::background_agents::BackgroundAgentState::Completed => "Done",
        crate::managers::background_agents::BackgroundAgentState::Failed => "Failed",
        crate::managers::background_agents::BackgroundAgentState::Killed => "Killed",
    };
    let footer = format!(
        "{status_str} · {elapsed_str} · {} tools",
        task.tool_call_count
    );
    let footer_color = match task.state {
        crate::managers::background_agents::BackgroundAgentState::Running => style_tokens::SUBTLE,
        crate::managers::background_agents::BackgroundAgentState::Completed => {
            style_tokens::SUCCESS
        }
        _ => style_tokens::ERROR,
    };

    let is_running =
        task.state == crate::managers::background_agents::BackgroundAgentState::Running;

    TaskCellData {
        title,
        icon,
        icon_color,
        activity,
        footer,
        footer_color,
        is_focused,
        is_running,
        scroll_offset,
    }
}

/// Integer ceiling division.
fn ceil_div(a: usize, b: usize) -> usize {
    a.div_ceil(b)
}

/// Truncate a string to fit within a given width.
fn truncate_str(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width > 3 {
        format!("{}...", &s[..max_width - 3])
    } else {
        s[..max_width].to_string()
    }
}

#[cfg(test)]
#[path = "background_tasks_tests.rs"]
mod tests;
