//! Setup wizard UI primitives.
//!
//! Clean terminal UI with section headers and minimal chrome.

use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ── Colors ─────────────────────────────────────────────────────────────────

const ACCENT: Color = Color::Rgb {
    r: 130,
    g: 160,
    b: 255,
};
const DIM_COLOR: Color = Color::Rgb {
    r: 100,
    g: 110,
    b: 120,
};
const SUCCESS_COLOR: Color = Color::Rgb {
    r: 106,
    g: 209,
    b: 143,
};
const ERROR_COLOR: Color = Color::Rgb {
    r: 255,
    g: 92,
    b: 87,
};
const TITLE_COLOR: Color = Color::Rgb {
    r: 0,
    g: 200,
    b: 200,
};

// ── Helpers ────────────────────────────────────────────────────────────────

fn term_width() -> usize {
    terminal::size().map(|(w, _)| w as usize).unwrap_or(80)
}

fn thin_line(w: &mut impl Write) {
    let width = term_width().min(56);
    let line: String = "─".repeat(width);
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(DIM_COLOR),
        Print(&line),
        ResetColor
    );
    let _ = writeln!(w);
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Opening banner.
pub fn rail_intro() {
    let mut w = io::stdout();
    let _ = writeln!(w);
    thin_line(&mut w);
    let _ = writeln!(w);
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(TITLE_COLOR),
        SetAttribute(Attribute::Bold),
        Print("OpenDev"),
        SetAttribute(Attribute::Reset),
        ResetColor,
        SetForegroundColor(DIM_COLOR),
        Print("  First-time setup"),
        ResetColor
    );
    let _ = writeln!(w);
    let _ = writeln!(w);
}

/// Section header — visual break between wizard phases.
pub fn rail_section(title: &str) {
    let mut w = io::stdout();
    thin_line(&mut w);
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(ACCENT),
        SetAttribute(Attribute::Bold),
        Print(title),
        SetAttribute(Attribute::Reset),
        ResetColor
    );
    let _ = writeln!(w);
    let _ = writeln!(w);
}

/// Label for a field/step — name + description on one line.
pub fn rail_label(name: &str, description: &str) {
    let mut w = io::stdout();
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetAttribute(Attribute::Bold),
        Print(name),
        SetAttribute(Attribute::Reset),
        SetForegroundColor(DIM_COLOR),
        Print(format!("  {description}")),
        ResetColor
    );
    let _ = writeln!(w);
}

/// Closing banner.
pub fn rail_outro() {
    let mut w = io::stdout();
    let _ = writeln!(w);
    thin_line(&mut w);
    let _ = writeln!(w);
}

/// Show a selected answer.
pub fn rail_answer(value: &str) {
    let mut w = io::stdout();
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(ACCENT),
        Print("→ "),
        ResetColor,
        Print(value)
    );
    let _ = writeln!(w);
    let _ = writeln!(w);
}

/// Success message.
pub fn rail_success(message: &str) {
    let mut w = io::stdout();
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(SUCCESS_COLOR),
        Print(format!("✓ {message}")),
        ResetColor
    );
    let _ = writeln!(w);
}

/// Error message.
pub fn rail_error(message: &str) {
    let mut w = io::stdout();
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(ERROR_COLOR),
        Print(format!("✗ {message}")),
        ResetColor
    );
    let _ = writeln!(w);
}

/// Dimmed/muted text.
pub fn rail_dim(message: &str) {
    let mut w = io::stdout();
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(DIM_COLOR),
        Print(message),
        ResetColor
    );
    let _ = writeln!(w);
}

/// Y/n confirm prompt. Returns bool.
pub fn rail_confirm(prompt_text: &str, default: bool) -> io::Result<bool> {
    let mut w = io::stdout();
    let hint = if default { "Y/n" } else { "y/N" };

    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(ACCENT),
        Print("? "),
        ResetColor,
        Print(prompt_text),
        SetForegroundColor(DIM_COLOR),
        Print(format!(" ({hint}) ")),
        ResetColor
    );
    w.flush()?;

    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    let answer = buf.trim().to_lowercase();

    if answer.is_empty() {
        return Ok(default);
    }
    Ok(answer.starts_with('y'))
}

/// Text input prompt. Password mode reads char-by-char with `*` echo.
pub fn rail_prompt(prompt_text: &str, password: bool) -> io::Result<String> {
    let mut w = io::stdout();

    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetForegroundColor(ACCENT),
        Print("? "),
        ResetColor,
        Print(prompt_text),
        Print(" ")
    );
    w.flush()?;

    if password {
        read_password()
    } else {
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
        Ok(buf.trim().to_string())
    }
}

/// Read a password with `*` echo using crossterm raw mode.
fn read_password() -> io::Result<String> {
    use crossterm::event::{self, Event, KeyCode, KeyModifiers};

    terminal::enable_raw_mode()?;
    let mut password = String::new();
    let mut stdout = io::stdout();

    loop {
        if let Event::Key(key_event) = event::read()? {
            match key_event.code {
                KeyCode::Enter => {
                    terminal::disable_raw_mode()?;
                    let _ = writeln!(stdout);
                    return Ok(password);
                }
                KeyCode::Backspace => {
                    if password.pop().is_some() {
                        let _ = write!(stdout, "\x08 \x08");
                        stdout.flush()?;
                    }
                }
                KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                    terminal::disable_raw_mode()?;
                    let _ = writeln!(stdout);
                    return Ok(String::new());
                }
                KeyCode::Esc => {
                    terminal::disable_raw_mode()?;
                    let _ = writeln!(stdout);
                    return Ok(String::new());
                }
                KeyCode::Char(c) => {
                    password.push(c);
                    let _ = write!(stdout, "*");
                    stdout.flush()?;
                }
                _ => {}
            }
        }
    }
}

/// Start a spinner with a message. Returns a handle to stop it.
pub fn rail_spinner_start(message: &str) -> SpinnerHandle {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    let msg = message.to_string();

    let handle = std::thread::spawn(move || {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let mut i = 0;
        let mut stdout = io::stdout();

        while running_clone.load(Ordering::Relaxed) {
            let frame = frames[i % frames.len()];
            let _ = crossterm::execute!(
                stdout,
                Print("\r"),
                Clear(ClearType::CurrentLine),
                Print("  "),
                SetForegroundColor(ACCENT),
                Print(frame),
                ResetColor,
                Print(format!(" {msg}"))
            );
            let _ = stdout.flush();
            i += 1;
            std::thread::sleep(std::time::Duration::from_millis(80));
        }

        let _ = crossterm::execute!(stdout, Print("\r"), Clear(ClearType::CurrentLine));
        let _ = stdout.flush();
    });

    SpinnerHandle {
        running,
        thread: Some(handle),
    }
}

/// Handle returned by `rail_spinner_start` to stop the spinner.
pub struct SpinnerHandle {
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SpinnerHandle {
    /// Stop the spinner and clear its line.
    pub fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for SpinnerHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Summary box before saving.
pub fn rail_summary_box(rows: &[(&str, &str)], key_lines: &[String]) {
    let mut w = io::stdout();
    let _ = writeln!(w);
    thin_line(&mut w);
    let _ = crossterm::execute!(
        w,
        Print("  "),
        SetAttribute(Attribute::Bold),
        Print("Configuration"),
        SetAttribute(Attribute::Reset)
    );
    let _ = writeln!(w);
    let _ = writeln!(w);

    for (label, value) in rows {
        let _ = crossterm::execute!(
            w,
            Print("    "),
            SetForegroundColor(DIM_COLOR),
            Print(format!("{:<10}", label)),
            ResetColor,
            Print(*value)
        );
        let _ = writeln!(w);
    }

    if !key_lines.is_empty() {
        let _ = writeln!(w);
        let _ = crossterm::execute!(
            w,
            Print("    "),
            SetForegroundColor(DIM_COLOR),
            Print("API Keys"),
            ResetColor
        );
        let _ = writeln!(w);
        for line in key_lines {
            let _ = crossterm::execute!(
                w,
                Print("    "),
                SetForegroundColor(DIM_COLOR),
                Print(line),
                ResetColor
            );
            let _ = writeln!(w);
        }
    }

    let _ = writeln!(w);
    thin_line(&mut w);
    let _ = writeln!(w);
}
