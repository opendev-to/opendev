//! Interactive menu component with arrow-key navigation for the setup wizard.

use crossterm::cursor::{Hide, MoveUp, Show};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{self, Clear, ClearType};
use std::io::{self, IsTerminal, Write};

use super::SetupError;

// ── Style constants ────────────────────────────────────────────────────────

const ACCENT: Color = Color::Rgb {
    r: 130,
    g: 160,
    b: 255,
};
const SELECTED_BG: Color = Color::Rgb {
    r: 31,
    g: 45,
    b: 58,
};
const DIM: Color = Color::Rgb {
    r: 100,
    g: 110,
    b: 120,
};

// ── RawModeGuard ───────────────────────────────────────────────────────────

/// RAII guard that restores terminal state on drop (even on panic).
struct RawModeGuard;

impl RawModeGuard {
    fn new() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let mut w = io::stdout();
        let _ = crossterm::execute!(w, Show);
    }
}

// ── InteractiveMenu ────────────────────────────────────────────────────────

/// A menu item: `(id, name, description)`.
pub type MenuItem = (String, String, String);

/// Arrow-key navigable menu with search support.
pub struct InteractiveMenu {
    all_items: Vec<MenuItem>,
    filtered_items: Vec<MenuItem>,
    #[allow(dead_code)]
    title: String,
    window_size: usize,
    selected_index: usize,
    search_query: String,
    search_mode: bool,
    term_width: usize,
}

impl InteractiveMenu {
    pub fn new(items: Vec<MenuItem>, title: &str, window_size: usize) -> Self {
        let filtered = items.clone();
        let tw = terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
        Self {
            all_items: items,
            filtered_items: filtered,
            title: title.to_string(),
            window_size,
            selected_index: 0,
            search_query: String::new(),
            search_mode: false,
            term_width: tw,
        }
    }

    /// Display the menu and handle user interaction.
    ///
    /// Returns `Ok(Some(id))` on selection, `Ok(None)` on cancel.
    pub fn show(&mut self) -> Result<Option<String>, SetupError> {
        if self.all_items.is_empty() {
            eprintln!("No items available");
            return Ok(None);
        }

        // Check if stdin is a TTY; if not, fall back to numbered menu
        if !io::stdin().is_terminal() {
            return self.show_numbered_fallback();
        }

        let _guard = RawModeGuard::new()?;
        let mut stdout = io::stdout();
        let _ = crossterm::execute!(stdout, Hide);

        // Initial render
        let mut num_lines = self.render(&mut stdout)?;

        loop {
            let event = event::read()?;

            if let Event::Key(key) = event {
                if self.search_mode {
                    match key.code {
                        KeyCode::Esc => {
                            self.search_mode = false;
                            self.search_query.clear();
                            self.filter_items();
                        }
                        KeyCode::Enter => {
                            if !self.filtered_items.is_empty() {
                                let id = self.filtered_items[self.selected_index].0.clone();
                                self.clear_display(&mut stdout, num_lines)?;
                                return Ok(Some(id));
                            }
                        }
                        KeyCode::Backspace => {
                            self.search_query.pop();
                            self.filter_items();
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.clear_display(&mut stdout, num_lines)?;
                            return Ok(None);
                        }
                        KeyCode::Char(c) => {
                            self.search_query.push(c);
                            self.filter_items();
                        }
                        KeyCode::Up => {
                            if !self.filtered_items.is_empty() {
                                self.selected_index =
                                    (self.selected_index + self.filtered_items.len() - 1)
                                        % self.filtered_items.len();
                            }
                        }
                        KeyCode::Down => {
                            if !self.filtered_items.is_empty() {
                                self.selected_index =
                                    (self.selected_index + 1) % self.filtered_items.len();
                            }
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Up => {
                            if !self.filtered_items.is_empty() {
                                self.selected_index =
                                    (self.selected_index + self.filtered_items.len() - 1)
                                        % self.filtered_items.len();
                            }
                        }
                        KeyCode::Down => {
                            if !self.filtered_items.is_empty() {
                                self.selected_index =
                                    (self.selected_index + 1) % self.filtered_items.len();
                            }
                        }
                        KeyCode::Enter => {
                            if !self.filtered_items.is_empty() {
                                let id = self.filtered_items[self.selected_index].0.clone();
                                self.clear_display(&mut stdout, num_lines)?;
                                return Ok(Some(id));
                            }
                        }
                        KeyCode::Char('/') => {
                            self.search_mode = true;
                            self.search_query.clear();
                        }
                        KeyCode::Esc => {
                            self.clear_display(&mut stdout, num_lines)?;
                            return Ok(None);
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.clear_display(&mut stdout, num_lines)?;
                            return Ok(None);
                        }
                        _ => {}
                    }
                }

                // Re-render
                self.clear_display(&mut stdout, num_lines)?;
                num_lines = self.render(&mut stdout)?;
            }
        }
    }

    /// Numbered fallback for non-TTY environments.
    fn show_numbered_fallback(&self) -> Result<Option<String>, SetupError> {
        let mut stdout = io::stdout();
        for (i, (id, name, desc)) in self.all_items.iter().enumerate() {
            let _ = writeln!(stdout, "    {}. {} — {} ({})", i + 1, name, desc, id);
        }
        let _ = writeln!(stdout);
        let _ = write!(stdout, "    Enter number: ");
        stdout.flush()?;

        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        if let Ok(n) = buf.trim().parse::<usize>()
            && n >= 1
            && n <= self.all_items.len()
        {
            return Ok(Some(self.all_items[n - 1].0.clone()));
        }
        Ok(None)
    }

    /// Filter items based on search query.
    fn filter_items(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_items = self.all_items.clone();
        } else {
            let query_lower = self.search_query.to_lowercase();
            self.filtered_items = self
                .all_items
                .iter()
                .filter(|(_, name, desc)| {
                    name.to_lowercase().contains(&query_lower)
                        || desc.to_lowercase().contains(&query_lower)
                })
                .cloned()
                .collect();
        }
        if self.selected_index >= self.filtered_items.len() {
            self.selected_index = self.filtered_items.len().saturating_sub(1);
        }
    }

    /// Render the current menu state. Returns the number of lines rendered.
    fn render(&self, w: &mut impl Write) -> io::Result<usize> {
        let mut line_count = 0;

        // Search header
        if self.search_mode {
            let _ = crossterm::execute!(
                w,
                Print("  "),
                SetForegroundColor(Color::Yellow),
                Print("/ "),
                Print(&self.search_query),
                Print("_"),
                ResetColor
            );
            let _ = write!(w, "\r\n");
            line_count += 1;
        }

        let total = self.filtered_items.len();
        if total == 0 {
            let _ = crossterm::execute!(
                w,
                Print("    "),
                SetForegroundColor(DIM),
                Print("No matches"),
                ResetColor
            );
            let _ = write!(w, "\r\n");
            line_count += 1;
        } else {
            let half_window = self.window_size / 2;
            let mut start = self.selected_index.saturating_sub(half_window);
            let end = (start + self.window_size).min(total);

            if end - start < self.window_size && total >= self.window_size {
                start = end.saturating_sub(self.window_size);
            }

            // "... (N more above)"
            if start > 0 {
                let _ = crossterm::execute!(
                    w,
                    Print("    "),
                    SetForegroundColor(DIM),
                    Print(format!("  {start} more above")),
                    ResetColor
                );
                let _ = write!(w, "\r\n");
                line_count += 1;
            }

            // Items
            for i in start..end {
                let (_id, name, description) = &self.filtered_items[i];
                let is_selected = i == self.selected_index;

                let desc_display = if description.chars().count() > 42 {
                    let truncated: String = description.chars().take(39).collect();
                    format!("{truncated}...")
                } else {
                    description.clone()
                };

                if is_selected {
                    let name_field = format!("{:<22}", name);
                    let content = format!("  > {name_field} {desc_display}");
                    let pad_len = self.term_width.saturating_sub(content.len());
                    let pad = " ".repeat(pad_len);

                    let _ = crossterm::execute!(
                        w,
                        SetBackgroundColor(SELECTED_BG),
                        Print("  "),
                        SetForegroundColor(ACCENT),
                        SetAttribute(Attribute::Bold),
                        Print("> "),
                        SetForegroundColor(Color::White),
                        Print(&name_field),
                        SetAttribute(Attribute::Reset),
                        SetBackgroundColor(SELECTED_BG),
                        SetForegroundColor(DIM),
                        Print(" "),
                        Print(&desc_display),
                        Print(&pad),
                        ResetColor
                    );
                } else {
                    let _ = crossterm::execute!(
                        w,
                        Print("    "),
                        SetForegroundColor(Color::White),
                        Print(format!("{:<22}", name)),
                        ResetColor,
                        SetForegroundColor(DIM),
                        Print(" "),
                        Print(&desc_display),
                        ResetColor
                    );
                }
                let _ = write!(w, "\r\n");
                line_count += 1;
            }

            // "... (N more below)"
            if end < total {
                let _ = crossterm::execute!(
                    w,
                    Print("    "),
                    SetForegroundColor(DIM),
                    Print(format!("  {} more below", total - end)),
                    ResetColor
                );
                let _ = write!(w, "\r\n");
                line_count += 1;
            }
        }

        // Footer
        let _ = write!(w, "\r\n");
        line_count += 1;

        let _ = crossterm::execute!(w, Print("  "), SetForegroundColor(DIM));
        if self.search_mode {
            let _ = crossterm::execute!(
                w,
                Print(format!(
                    "{}/{} matched",
                    total,
                    self.all_items.len()
                ))
            );
        } else {
            let _ = crossterm::execute!(w, Print("↑↓ navigate · enter select · / search"));
        }
        let _ = crossterm::execute!(w, ResetColor);
        let _ = write!(w, "\r\n");
        line_count += 1;

        w.flush()?;
        Ok(line_count)
    }

    /// Clear previously rendered lines.
    fn clear_display(&self, w: &mut impl Write, num_lines: usize) -> io::Result<()> {
        for _ in 0..num_lines {
            let _ = crossterm::execute!(w, MoveUp(1), Clear(ClearType::CurrentLine));
        }
        Ok(())
    }
}
