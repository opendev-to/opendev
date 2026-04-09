//! Popup panel and modal dialog rendering methods.
//!
//! Extracted from `render.rs` — all methods remain on `impl App`.

use ratatui::layout;

use super::App;

impl App {
    /// Shared helper that renders a popup panel matching the Python Textual style:
    /// bright_cyan border, `▸` pointer, bold white active label, dim descriptions.
    /// Padding (1, 2) = 1 empty line top/bottom, 2 spaces horizontal.
    pub(super) fn render_popup_panel(
        frame: &mut ratatui::Frame,
        input_area: layout::Rect,
        title: &str,
        content_lines: &[ratatui::text::Line<'_>],
        option_lines: &[ratatui::text::Line<'_>],
        hint: &str,
        max_width: Option<u16>,
    ) {
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

        let mut lines: Vec<Line> = Vec::new();

        // Top padding (1 empty line)
        lines.push(Line::from(""));

        // Content section
        for line in content_lines {
            lines.push(line.clone());
        }

        // Hint line
        lines.push(Line::from(Span::styled(
            format!("    {hint}"),
            Style::default().fg(style_tokens::DIM_GREY),
        )));

        // Option lines
        for line in option_lines {
            lines.push(line.clone());
        }

        // Bottom padding (1 empty line)
        lines.push(Line::from(""));

        let panel_width = max_width
            .map(|w| input_area.width.min(w))
            .unwrap_or(input_area.width);
        let panel_height = (lines.len() as u16 + 2).min(input_area.y);
        let popup_area = layout::Rect {
            x: input_area.x,
            y: input_area.y.saturating_sub(panel_height),
            width: panel_width,
            height: panel_height,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Self::PANEL_CYAN))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    /// Build a single option line matching the Python Textual style.
    /// Active: `▸` bright_cyan pointer + dim number + bold white label + dim description.
    /// Inactive: space pointer + dim number + white label + dim description.
    pub(super) fn build_option_line<'a>(
        is_selected: bool,
        number: &str,
        label: &str,
        description: &str,
    ) -> ratatui::text::Line<'a> {
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let pointer = if is_selected { "\u{25b8}" } else { " " };
        let pointer_style = if is_selected {
            Style::default()
                .fg(Self::PANEL_CYAN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(style_tokens::DIM_GREY)
        };
        let num_style = Style::default().fg(style_tokens::DIM_GREY);
        let label_style = if is_selected {
            Style::default()
                .fg(style_tokens::PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(style_tokens::PRIMARY)
        };
        let desc_style = Style::default().fg(style_tokens::DIM_GREY);

        let mut spans = vec![
            Span::styled(format!("    {pointer} "), pointer_style),
            Span::styled(format!("{number} "), num_style),
            Span::styled(label.to_string(), label_style),
        ];
        if !description.is_empty() {
            spans.push(Span::styled(format!("  {description}"), desc_style));
        }
        Line::from(spans)
    }

    /// Render autocomplete popup above the input area.
    pub(super) fn render_autocomplete(&self, frame: &mut ratatui::Frame, input_area: layout::Rect) {
        use crate::autocomplete::CompletionKind;
        use crate::formatters::style_tokens;
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

        let items = self.state.autocomplete.items();
        let selected_idx = self.state.autocomplete.selected_index();
        let max_show = items.len().min(10);
        let popup_height = max_show as u16 + 2; // +2 for borders

        // Determine title and width based on completion kind
        let is_file_mode = items
            .first()
            .is_some_and(|i| i.kind == CompletionKind::File);
        let popup_width = if is_file_mode { 60 } else { 50 };
        let title = if is_file_mode {
            " Files "
        } else {
            " Commands "
        };

        let popup_area = layout::Rect {
            x: input_area.x,
            y: input_area.y.saturating_sub(popup_height),
            width: input_area.width.min(popup_width),
            height: popup_height,
        };

        // Python uses BLUE_BG_ACTIVE (#1f2d3a) as active row bg
        let active_bg = Color::Rgb(31, 45, 58);

        let lines: Vec<Line> = items
            .iter()
            .take(max_show)
            .enumerate()
            .map(|(i, item)| {
                let selected = i == selected_idx;
                let (left, right) =
                    crate::autocomplete::formatters::CompletionFormatter::format(item);

                let pointer = if selected { "\u{25b8}" } else { "\u{2022}" };
                let pointer_style = if selected {
                    Style::default()
                        .fg(Self::PANEL_CYAN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(style_tokens::DIM_GREY)
                };
                let label_style = if selected {
                    Style::default()
                        .fg(Self::PANEL_CYAN)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(style_tokens::PRIMARY)
                };
                let desc_style = if selected {
                    Style::default().fg(style_tokens::GREY)
                } else {
                    Style::default().fg(style_tokens::SUBTLE)
                };

                let line = Line::from(vec![
                    Span::styled(format!(" {pointer} "), pointer_style),
                    Span::styled(left, label_style),
                    Span::styled(format!(" {right}"), desc_style),
                ]);
                if selected {
                    line.style(Style::default().bg(active_bg))
                } else {
                    line
                }
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(style_tokens::BORDER))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    /// Render the plan approval panel above the input area.
    pub(super) fn render_plan_approval(
        &self,
        frame: &mut ratatui::Frame,
        input_area: layout::Rect,
    ) {
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let plan_options = self.plan_approval_controller.options();
        let selected = self.plan_approval_controller.selected_action();

        let content_lines = vec![Line::from(vec![
            Span::styled("    Plan ", Style::default().fg(style_tokens::DIM_GREY)),
            Span::styled("\u{00b7} ", Style::default().fg(style_tokens::DIM_GREY)),
            Span::styled(
                "Ready for review",
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];

        let option_lines: Vec<Line> = plan_options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                Self::build_option_line(
                    i == selected,
                    &format!("{}.", i + 1),
                    &opt.label,
                    &opt.description,
                )
            })
            .collect();

        Self::render_popup_panel(
            frame,
            input_area,
            " Approval ",
            &content_lines,
            &option_lines,
            "\u{2191}/\u{2193} choose \u{00b7} Enter confirm \u{00b7} Esc cancel",
            None,
        );
    }

    /// Render the ask-user prompt panel.
    pub(super) fn render_ask_user(&self, frame: &mut ratatui::Frame, input_area: layout::Rect) {
        use crate::formatters::style_tokens;
        use ratatui::style::Style;
        use ratatui::text::{Line, Span};

        let ask_options = self.ask_user_controller.options();
        let selected = self.ask_user_controller.selected_index();
        let question = self.ask_user_controller.question();

        let content_lines = vec![Line::from(Span::styled(
            format!("    {question}"),
            Style::default().fg(style_tokens::PRIMARY),
        ))];

        if ask_options.is_empty() {
            // Free-text input mode
            let text = self.ask_user_controller.text_input();
            let input_line = Line::from(vec![
                Span::styled("    ", Style::default()),
                Span::styled(
                    if text.is_empty() {
                        "\u{2588}".to_string()
                    } else {
                        format!("{text}\u{2588}")
                    },
                    Style::default().fg(style_tokens::ACCENT),
                ),
            ]);

            Self::render_popup_panel(
                frame,
                input_area,
                " Question ",
                &content_lines,
                &[input_line],
                "Type answer \u{00b7} Enter confirm \u{00b7} Esc cancel",
                None,
            );
        } else {
            let option_lines: Vec<Line> = ask_options
                .iter()
                .enumerate()
                .map(|(i, opt)| {
                    Self::build_option_line(i == selected, &format!("{}.", i + 1), opt, "")
                })
                .collect();

            Self::render_popup_panel(
                frame,
                input_area,
                " Question ",
                &content_lines,
                &option_lines,
                "\u{2191}/\u{2193} choose \u{00b7} Enter confirm \u{00b7} Esc cancel",
                None,
            );
        }
    }

    /// Render the model picker panel above the input area.
    pub(super) fn render_model_picker(&self, frame: &mut ratatui::Frame, input_area: layout::Rect) {
        use crate::controllers::ModelPickerController;
        use crate::formatters::style_tokens;
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

        let picker = match self.model_picker_controller {
            Some(ref p) => p,
            None => return,
        };

        let visible = picker.visible_models();
        let selected_idx = picker.selected_index();
        let total = picker.filtered_count();
        let query = picker.search_query();

        let active_bg = Color::Rgb(31, 45, 58);
        let mut lines: Vec<Line> = Vec::new();

        // Search bar
        let search_display = if query.is_empty() {
            "Type to search...".to_string()
        } else {
            query.to_string()
        };
        let search_style = if query.is_empty() {
            Style::default().fg(style_tokens::DIM_GREY)
        } else {
            Style::default()
                .fg(Self::PANEL_CYAN)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(Line::from(vec![
            Span::styled("  \u{1f50d} ", Style::default().fg(style_tokens::DIM_GREY)),
            Span::styled(search_display, search_style),
        ]));

        // Separator
        lines.push(Line::from(Span::styled(
            "  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
            Style::default().fg(style_tokens::BORDER),
        )));

        // Track current provider for group headers
        let mut current_provider = String::new();

        for (display_idx, model) in &visible {
            // Provider group header
            if model.provider != current_provider {
                current_provider = model.provider.clone();
                let mut header_spans = vec![Span::styled(
                    format!("  {} {}", "\u{25cf}", model.provider_display),
                    Style::default()
                        .fg(if model.has_api_key {
                            style_tokens::GREY
                        } else {
                            style_tokens::DIM_GREY
                        })
                        .add_modifier(Modifier::BOLD),
                )];
                if !model.has_api_key {
                    header_spans.push(Span::styled(
                        "  \u{26a0} no key",
                        Style::default().fg(Color::Rgb(120, 120, 120)),
                    ));
                }
                lines.push(Line::from(header_spans));
            }

            let selected = *display_idx == selected_idx;
            let is_current = model.id == self.state.model;

            // Pointer
            let pointer = if selected { "\u{25b8}" } else { " " };
            let pointer_style = if selected {
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(style_tokens::DIM_GREY)
            };

            // Model name
            let name_style = if !model.has_api_key {
                Style::default().fg(style_tokens::DIM_GREY)
            } else if selected {
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default()
                    .fg(Color::Rgb(0, 200, 100))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(style_tokens::PRIMARY)
            };

            // Context and pricing info
            let ctx = ModelPickerController::format_context(model.context_length);
            let pricing =
                ModelPickerController::format_pricing(model.pricing_input, model.pricing_output);

            let mut spans = vec![
                Span::styled(format!("    {pointer} "), pointer_style),
                Span::styled(model.name.clone(), name_style),
            ];

            // Current model indicator
            if is_current {
                spans.push(Span::styled(
                    " \u{2713}",
                    Style::default().fg(Color::Rgb(0, 200, 100)),
                ));
            }

            // Recommended badge
            if model.recommended {
                spans.push(Span::styled(
                    " \u{2605}",
                    Style::default().fg(Color::Rgb(255, 200, 50)),
                ));
            }

            // Context length
            spans.push(Span::styled(
                format!("  {ctx}"),
                Style::default().fg(style_tokens::DIM_GREY),
            ));

            // Pricing
            spans.push(Span::styled(
                format!("  {pricing}"),
                Style::default().fg(style_tokens::SUBTLE),
            ));

            let line = Line::from(spans);
            if selected {
                lines.push(line.style(Style::default().bg(active_bg)));
            } else {
                lines.push(line);
            }
        }

        // Empty state
        if visible.is_empty() {
            lines.push(Line::from(Span::styled(
                "    No models match your search.",
                Style::default().fg(style_tokens::DIM_GREY),
            )));
        }

        // Bottom hint with count
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {total} model{}", if total == 1 { "" } else { "s" }),
                Style::default().fg(style_tokens::DIM_GREY),
            ),
            Span::styled(
                "  \u{2191}/\u{2193} navigate \u{00b7} Enter select \u{00b7} Esc cancel",
                Style::default().fg(style_tokens::DIM_GREY),
            ),
        ]));

        let panel_height = (lines.len() as u16 + 2).min(input_area.y);
        let panel_width = input_area.width.min(80);
        let popup_area = layout::Rect {
            x: input_area.x,
            y: input_area.y.saturating_sub(panel_height),
            width: panel_width,
            height: panel_height,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Self::PANEL_CYAN))
            .title(Span::styled(
                " Models ",
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    /// Render the debug panel overlay.
    pub(super) fn render_debug_panel(&self, frame: &mut ratatui::Frame, area: layout::Rect) {
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        let label_style = Style::default().fg(style_tokens::DIM_GREY);
        let value_style = Style::default()
            .fg(Self::PANEL_CYAN)
            .add_modifier(Modifier::BOLD);

        let stats = [
            ("Model", self.state.model.clone()),
            (
                "Tokens",
                format!("{} / {}", self.state.tokens_used, self.state.tokens_limit),
            ),
            ("Context", format!("{:.1}%", self.state.context_usage_pct)),
            ("Cost", format!("${:.4}", self.state.session_cost)),
            ("Messages", format!("{}", self.state.messages.len())),
            ("Active tools", format!("{}", self.state.active_tools.len())),
            (
                "Subagents",
                format!("{}", self.state.active_subagents.len()),
            ),
            (
                "Background tasks",
                format!("{}", self.state.background_task_count),
            ),
            ("Mode", format!("{}", self.state.mode)),
            ("Autonomy", format!("{}", self.state.autonomy)),
            ("Reasoning", format!("{}", self.state.reasoning_level)),
            (
                "Terminal",
                format!(
                    "{}x{}",
                    self.state.terminal_width, self.state.terminal_height
                ),
            ),
            ("Undo depth", format!("{}", self.state.undo_depth)),
        ];

        for (label, value) in &stats {
            lines.push(Line::from(vec![
                Span::styled(format!("    {label}: "), label_style),
                Span::styled(value.clone(), value_style),
            ]));
        }

        lines.push(Line::from(""));

        let panel_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(4));
        let panel_width = 50u16.min(area.width.saturating_sub(4));
        let popup_area = layout::Rect {
            x: (area.width.saturating_sub(panel_width)) / 2,
            y: (area.height.saturating_sub(panel_height)) / 2,
            width: panel_width,
            height: panel_height,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Self::PANEL_CYAN))
            .title(Span::styled(
                " Debug ",
                Style::default()
                    .fg(Self::PANEL_CYAN)
                    .add_modifier(Modifier::BOLD),
            ));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(paragraph, popup_area);
    }

    /// Render the tool approval prompt panel.
    pub(super) fn render_approval(&self, frame: &mut ratatui::Frame, input_area: layout::Rect) {
        use crate::formatters::style_tokens;
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};

        let approval_options = self.approval_controller.options();
        let selected = self.approval_controller.selected_index();
        let command = self.approval_controller.command();
        let working_dir = self.approval_controller.working_dir();

        let content_lines = vec![
            Line::from(vec![
                Span::styled("    Command ", Style::default().fg(style_tokens::DIM_GREY)),
                Span::styled("\u{00b7} ", Style::default().fg(style_tokens::DIM_GREY)),
                Span::styled(
                    command.to_string(),
                    Style::default()
                        .fg(Self::PANEL_CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                format!("    Directory \u{00b7} {working_dir}"),
                Style::default().fg(style_tokens::DIM_GREY),
            )),
        ];

        let option_lines: Vec<Line> = approval_options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                Self::build_option_line(
                    i == selected,
                    &format!("{}.", opt.choice),
                    &opt.label,
                    &opt.description,
                )
            })
            .collect();

        Self::render_popup_panel(
            frame,
            input_area,
            " Approval ",
            &content_lines,
            &option_lines,
            "\u{2191}/\u{2193} choose \u{00b7} Enter confirm \u{00b7} Esc cancel",
            None,
        );
    }
}
