//! Animated welcome panel with cyan/blue matrix rain and breathing logo.
//!
//! Features:
//! - Cyan-blue matrix braille rain with per-column depth variation
//! - Bright head glow on each rain column
//! - "O p e n D e v" as breathing negative space with per-letter hue shift
//! - Cohesive blue-themed gradient text and border
//! - Fade-out animation on first message submission

mod color;
mod state;

pub use state::WelcomePanelState;

use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

use crate::formatters::style_tokens;
use crate::widgets::spinner::SPINNER_FRAMES;

use color::hsl_to_rgb;

// ---------------------------------------------------------------------------
// WelcomePanelWidget — renders directly to the buffer
// ---------------------------------------------------------------------------

/// Stateless widget that renders the animated welcome panel from `WelcomePanelState`.
pub struct WelcomePanelWidget<'a> {
    state: &'a WelcomePanelState,
    version: &'a str,
    mode: &'a str,
}

impl<'a> WelcomePanelWidget<'a> {
    pub fn new(state: &'a WelcomePanelState) -> Self {
        Self {
            state,
            version: env!("CARGO_PKG_VERSION"),
            mode: "NORMAL",
        }
    }

    pub fn version(mut self, version: &'a str) -> Self {
        self.version = version;
        self
    }

    pub fn mode(mut self, mode: &'a str) -> Self {
        self.mode = mode;
        self
    }

    /// Write a single character with foreground color at (x, y), respecting area bounds.
    #[inline]
    fn put(buf: &mut Buffer, area: Rect, x: u16, y: u16, ch: char, fg: Color) {
        if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
            let cell = buf.cell_mut((x, y)).unwrap();
            cell.set_char(ch);
            cell.set_fg(fg);
        }
    }

    /// Write a string with cyan-blue gradient sweep, centered in the row.
    fn write_gradient_line(
        &self,
        buf: &mut Buffer,
        area: Rect,
        y: u16,
        text: &str,
        line_offset: u16,
    ) {
        let text_len = text.chars().count() as u16;
        let start_x = area.x + area.width.saturating_sub(text_len) / 2;
        for (i, ch) in text.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            // Sweep within 190-250 (cyan to blue-purple) instead of full rainbow
            let sweep = (i as u16 * 3 + line_offset * 10 + self.state.gradient_offset) % 360;
            let hue = 190.0 + (sweep as f64 / 360.0) * 60.0; // maps to 190..250
            let sat = 0.75 * self.state.fade_progress as f64;
            let lit = 0.55 * self.state.fade_progress as f64
                + 0.1 * (1.0 - self.state.fade_progress as f64);
            let color = hsl_to_rgb(hue, sat, lit);
            Self::put(buf, area, start_x + i as u16, y, ch, color);
        }
    }

    /// Draw the rounded border with slow cyan-blue sweep.
    fn draw_border(&self, buf: &mut Buffer, area: Rect, bx: u16, by: u16, bw: u16, bh: u16) {
        let offset = self.state.gradient_offset;
        let fade = self.state.fade_progress as f64;
        let perimeter = 2 * (bw + bh);

        let border_color = |idx: u16| -> Color {
            let t = ((idx as f64 / perimeter as f64) + offset as f64 / 360.0) % 1.0;
            let hue = 195.0 + t * 50.0; // 195..245 (cyan to blue)
            hsl_to_rgb(hue, 0.6 * fade, 0.35 * fade + 0.08 * (1.0 - fade))
        };

        // Top: ╭───╮
        Self::put(
            buf,
            area,
            bx,
            by,
            style_tokens::BOX_TL.chars().next().unwrap(),
            border_color(0),
        );
        for i in 1..bw - 1 {
            Self::put(
                buf,
                area,
                bx + i,
                by,
                style_tokens::BOX_H.chars().next().unwrap(),
                border_color(i),
            );
        }
        Self::put(
            buf,
            area,
            bx + bw - 1,
            by,
            style_tokens::BOX_TR.chars().next().unwrap(),
            border_color(bw),
        );

        // Bottom: ╰───╯
        Self::put(
            buf,
            area,
            bx,
            by + bh - 1,
            style_tokens::BOX_BL.chars().next().unwrap(),
            border_color(bw + bh),
        );
        for i in 1..bw - 1 {
            Self::put(
                buf,
                area,
                bx + i,
                by + bh - 1,
                style_tokens::BOX_H.chars().next().unwrap(),
                border_color(bw + bh + i),
            );
        }
        Self::put(
            buf,
            area,
            bx + bw - 1,
            by + bh - 1,
            style_tokens::BOX_BR.chars().next().unwrap(),
            border_color(2 * bw + bh),
        );

        // Sides: │
        let v = style_tokens::BOX_V.chars().next().unwrap();
        for j in 1..bh - 1 {
            Self::put(buf, area, bx, by + j, v, border_color(bw + j));
            Self::put(
                buf,
                area,
                bx + bw - 1,
                by + j,
                v,
                border_color(2 * bw + bh + j),
            );
        }
    }

    /// 3-row elegant logo: thin frame lines + wide-spaced text.
    const LOGO_LINES: [&'static str; 3] = [
        "─────────────────────────────",
        "  O   P   E   N   D   E   V  ",
        "─────────────────────────────",
    ];
    const LOGO_WIDTH: usize = 29;
    const LOGO_HEIGHT: usize = 3;

    /// Render the matrix braille rain field.
    fn render_rain(&self, buf: &mut Buffer, area: Rect, rx: u16, ry: u16, rw: usize, rh: usize) {
        let fade = self.state.fade_progress as f64;
        let base_hue = 200.0;

        // Decide between block logo (big) and compact single-line (small)
        let show_block = rh >= 5 && rw >= Self::LOGO_WIDTH;
        let compact_logo = "O p e n D e v";
        let compact_len = compact_logo.len();
        let show_compact = !show_block && rw >= compact_len;

        // Block logo zone
        let bl_start_col = rw.saturating_sub(Self::LOGO_WIDTH) / 2;
        let bl_end_col = bl_start_col + Self::LOGO_WIDTH;
        let bl_start_row = rh.saturating_sub(Self::LOGO_HEIGHT) / 2;
        let bl_end_row = bl_start_row + Self::LOGO_HEIGHT;

        // Compact logo zone
        let cl_start_col = rw.saturating_sub(compact_len) / 2;
        let cl_end_col = cl_start_col + compact_len;
        let cl_row = rh / 2;

        for (col_idx, rain_col) in self.state.rain_columns.iter().enumerate().take(rw) {
            let head_y = rain_col.y as i32;
            let trail = rain_col.trail_len as i32;
            let col_hue = base_hue + rain_col.hue_offset as f64;

            for row in 0..rh {
                let row_i = row as i32;
                let dist = head_y - row_i;

                // Block logo exclusion zone
                if show_block
                    && row >= bl_start_row
                    && row < bl_end_row
                    && col_idx >= bl_start_col
                    && col_idx < bl_end_col
                {
                    let logo_r = row - bl_start_row;
                    let logo_c = col_idx - bl_start_col;
                    if let Some(ch) = Self::LOGO_LINES[logo_r].chars().nth(logo_c)
                        && ch != ' '
                    {
                        let is_frame = ch == '─';
                        if is_frame {
                            // Frame lines: subtle, steady glow
                            let color = hsl_to_rgb(210.0, 0.5 * fade, 0.22 * fade);
                            Self::put(buf, area, rx + col_idx as u16, ry + row as u16, ch, color);
                        } else {
                            // Letters: bright breathing with per-letter hue shift
                            let letter_t = logo_c as f64 / Self::LOGO_WIDTH as f64;
                            let letter_hue = 190.0 + letter_t * 40.0;
                            let breathe = 0.35 + 0.25 * (1.0 + self.state.breathe_phase.sin());
                            let color = hsl_to_rgb(letter_hue, 0.9 * fade, breathe * fade);
                            Self::put(buf, area, rx + col_idx as u16, ry + row as u16, ch, color);
                        }
                    }
                    continue;
                }

                // Compact logo fallback (single line)
                if show_compact && row == cl_row && col_idx >= cl_start_col && col_idx < cl_end_col
                {
                    let ch_idx = col_idx - cl_start_col;
                    let ch = compact_logo.as_bytes()[ch_idx] as char;
                    if ch != ' ' {
                        let letter_t = ch_idx as f64 / compact_len as f64;
                        let letter_hue = 190.0 + letter_t * 40.0;
                        let breathe = 0.30 + 0.22 * (1.0 + self.state.breathe_phase.sin());
                        let color = hsl_to_rgb(letter_hue, 0.85 * fade, breathe * fade);
                        Self::put(buf, area, rx + col_idx as u16, ry + row as u16, ch, color);
                    }
                    continue;
                }

                // Rain drop within trail range
                if dist >= 0 && dist <= trail {
                    let t = dist as f64 / trail as f64;
                    if dist == 0 {
                        let color = hsl_to_rgb(col_hue, 0.5 * fade, 0.75 * fade);
                        Self::put(buf, area, rx + col_idx as u16, ry + row as u16, '▓', color);
                    } else if dist == 1 {
                        let color = hsl_to_rgb(col_hue, 0.75 * fade, 0.55 * fade);
                        Self::put(buf, area, rx + col_idx as u16, ry + row as u16, '░', color);
                    } else {
                        let lightness = 0.45 * (1.0 - t) + 0.10 * t;
                        let frame_idx =
                            (rain_col.char_offset as usize + row + self.state.braille_offset)
                                % SPINNER_FRAMES.len();
                        let ch = SPINNER_FRAMES[frame_idx];
                        let color = hsl_to_rgb(col_hue, 0.8 * fade, lightness * fade);
                        Self::put(buf, area, rx + col_idx as u16, ry + row as u16, ch, color);
                    }
                }
            }
        }
    }
}

impl Widget for WelcomePanelWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 3 {
            return;
        }

        // Content lines
        let line1 = format!(
            "\u{2550}\u{2550}\u{2550}  O P E N D E V  v{}  \u{2550}\u{2550}\u{2550}",
            self.version
        );
        let line3 =
            "/help  \u{2502}  /models  \u{2502}  Shift+Tab plan mode  \u{2502}  @file context";

        // 3-tier responsive layout
        if area.height < 5 {
            // Tier 1: just gradient text, centered
            let cy = area.y + area.height / 2;
            self.write_gradient_line(buf, area, cy, &line1, 0);
        } else if area.height < 11 {
            // Tier 2: border + gradient text
            let box_w = (area.width.saturating_sub(4)).min(90);
            let box_h = area.height.min(7);
            let bx = area.x + (area.width.saturating_sub(box_w)) / 2;
            let by = area.y + (area.height.saturating_sub(box_h)) / 2;

            self.draw_border(buf, area, bx, by, box_w, box_h);

            // Title line centered within box
            let title_y = by + 2;
            self.write_gradient_line(buf, area, title_y, &line1, 0);

            // Help line
            if box_h > 4 {
                let help_y = by + box_h.saturating_sub(2);
                self.write_gradient_line(buf, area, help_y, line3, 2);
            }
        } else {
            // Tier 3: rain field + border + gradient text
            let box_w = (area.width.saturating_sub(4)).min(90);
            let box_h = 5u16;
            let has_rain = !self.state.rain_columns.is_empty();
            let rain_h = if has_rain {
                (area.height.saturating_sub(box_h + 2)).clamp(4, 20) as usize
            } else {
                0
            };
            let rain_w = ((box_w as f32 * 0.7) as usize).clamp(20, 90);

            // Center everything vertically
            let total_h = rain_h as u16 + box_h;
            let start_y = area.y + area.height.saturating_sub(total_h) / 2;
            let center_x = area.x + (area.width.saturating_sub(box_w)) / 2;

            // Rain field (above the box)
            if has_rain {
                let rain_x = area.x + (area.width.saturating_sub(rain_w as u16)) / 2;
                let rain_y = start_y;
                self.render_rain(buf, area, rain_x, rain_y, rain_w, rain_h);
            }

            // Border box below rain (or centered if no rain yet)
            let by = start_y + rain_h as u16;
            self.draw_border(buf, area, center_x, by, box_w, box_h);

            // Title line centered within box
            let title_y = by + 1;
            self.write_gradient_line(buf, area, title_y, &line1, 0);

            // Help line
            let help_y = by + 3;
            self.write_gradient_line(buf, area, help_y, line3, 2);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
