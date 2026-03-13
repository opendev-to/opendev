//! Animated welcome panel with cyan/blue matrix rain and breathing logo.
//!
//! Features:
//! - Cyan-blue matrix braille rain with per-column depth variation
//! - Bright head glow on each rain column
//! - "O p e n D e v" as breathing negative space with per-letter hue shift
//! - Cohesive blue-themed gradient text and border
//! - Fade-out animation on first message submission

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::Widget,
};

use crate::formatters::style_tokens;
use crate::widgets::spinner::SPINNER_FRAMES;

// ---------------------------------------------------------------------------
// HSL → RGB helper
// ---------------------------------------------------------------------------

/// Convert HSL to ratatui `Color::Rgb`. Hue in 0..360, saturation/lightness in 0.0..1.0.
fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> Color {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = hue / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    Color::Rgb(
        ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8,
    )
}

// ---------------------------------------------------------------------------
// Inline pseudo-random (LCG — no `rand` dependency)
// ---------------------------------------------------------------------------

fn pseudo_rand(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    (*seed >> 33) as f32 / (1u64 << 31) as f32
}

// ---------------------------------------------------------------------------
// Rain column
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RainColumn {
    y: f32,
    speed: f32,
    trail_len: u8,
    char_offset: u8,
    hue_offset: f32, // per-column hue variation for depth
}

// ---------------------------------------------------------------------------
// WelcomePanelState — animation state ticked by App
// ---------------------------------------------------------------------------

/// Persistent animation state for the welcome panel.
#[derive(Debug, Clone)]
pub struct WelcomePanelState {
    gradient_offset: u16,
    braille_offset: usize,
    braille_tick: u8,
    breathe_phase: f64,
    rain_columns: Vec<RainColumn>,
    rain_width: usize,
    rain_height: usize,
    fade_progress: f32,
    /// Whether the panel is currently fading out.
    pub is_fading: bool,
    /// Set to `true` once the fade-out completes; the panel should no longer be rendered.
    pub fade_complete: bool,
    rng_seed: u64,
}

impl WelcomePanelState {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        Self {
            gradient_offset: 0,
            braille_offset: 0,
            braille_tick: 0,
            breathe_phase: 0.0,
            rain_columns: Vec::new(),
            rain_width: 0,
            rain_height: 0,
            fade_progress: 1.0,
            is_fading: false,
            fade_complete: false,
            rng_seed: seed,
        }
    }

    /// Advance animations by one tick (~60ms).
    pub fn tick(&mut self, _terminal_width: u16, _terminal_height: u16) {
        if self.is_fading {
            self.fade_progress -= 0.1;
            if self.fade_progress <= 0.0 {
                self.fade_progress = 0.0;
                self.fade_complete = true;
            }
            return;
        }

        // Gradient rotation (slower, more elegant sweep)
        self.gradient_offset = (self.gradient_offset + 3) % 360;

        // Braille offset advances every 2 ticks
        self.braille_tick += 1;
        if self.braille_tick >= 2 {
            self.braille_tick = 0;
            self.braille_offset = (self.braille_offset + 1) % SPINNER_FRAMES.len();
        }

        // Breathing phase: full cycle in ~67 ticks (~4s at 60ms tick)
        self.breathe_phase += std::f64::consts::TAU / 67.0;
        if self.breathe_phase >= std::f64::consts::TAU {
            self.breathe_phase -= std::f64::consts::TAU;
        }

        // Advance rain
        self.step_rain();
    }

    /// Begin the fade-out animation.
    pub fn start_fade(&mut self) {
        self.is_fading = true;
    }

    /// Lazily initialize or resize the rain field.
    pub fn ensure_rain_field(&mut self, w: usize, h: usize) {
        if self.rain_width == w && self.rain_height == h && !self.rain_columns.is_empty() {
            return;
        }
        self.rain_width = w;
        self.rain_height = h;
        self.rain_columns.clear();
        self.rain_columns.reserve(w);
        for _ in 0..w {
            let y = pseudo_rand(&mut self.rng_seed) * h as f32;
            let speed = 0.10 + pseudo_rand(&mut self.rng_seed) * 0.40;
            let trail_len = 4 + (pseudo_rand(&mut self.rng_seed) * 6.0) as u8;
            let char_offset = (pseudo_rand(&mut self.rng_seed) * SPINNER_FRAMES.len() as f32) as u8;
            let hue_offset = pseudo_rand(&mut self.rng_seed) * 30.0 - 15.0; // -15..+15
            self.rain_columns.push(RainColumn {
                y,
                speed,
                trail_len,
                char_offset,
                hue_offset,
            });
        }
    }

    /// Advance rain drop positions, resetting off-screen columns.
    fn step_rain(&mut self) {
        let h = self.rain_height as f32;
        if h <= 0.0 {
            return;
        }
        for col in &mut self.rain_columns {
            col.y += col.speed;
            if col.y > h + col.trail_len as f32 {
                col.y = -(pseudo_rand(&mut (col.char_offset as u64 ^ 0xDEAD_BEEF)) * 6.0);
                col.speed = 0.10 + (col.char_offset as f32 % 9.0) * 0.04;
            }
        }
    }
}

impl Default for WelcomePanelState {
    fn default() -> Self {
        Self::new()
    }
}

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
            version: "0.1.0",
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
        Self::put(buf, area, bx, by, style_tokens::BOX_TL.chars().next().unwrap(), border_color(0));
        for i in 1..bw - 1 {
            Self::put(buf, area, bx + i, by, style_tokens::BOX_H.chars().next().unwrap(), border_color(i));
        }
        Self::put(buf, area, bx + bw - 1, by, style_tokens::BOX_TR.chars().next().unwrap(), border_color(bw));

        // Bottom: ╰───╯
        Self::put(buf, area, bx, by + bh - 1, style_tokens::BOX_BL.chars().next().unwrap(), border_color(bw + bh));
        for i in 1..bw - 1 {
            Self::put(buf, area, bx + i, by + bh - 1, style_tokens::BOX_H.chars().next().unwrap(), border_color(bw + bh + i));
        }
        Self::put(buf, area, bx + bw - 1, by + bh - 1, style_tokens::BOX_BR.chars().next().unwrap(), border_color(2 * bw + bh));

        // Sides: │
        let v = style_tokens::BOX_V.chars().next().unwrap();
        for j in 1..bh - 1 {
            Self::put(buf, area, bx, by + j, v, border_color(bw + j));
            Self::put(buf, area, bx + bw - 1, by + j, v, border_color(2 * bw + bh + j));
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
                if show_compact
                    && row == cl_row
                    && col_idx >= cl_start_col
                    && col_idx < cl_end_col
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
                        let frame_idx = (rain_col.char_offset as usize
                            + row
                            + self.state.braille_offset)
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
        let line3 = "/help  \u{2502}  /models  \u{2502}  Shift+Tab plan mode  \u{2502}  @file context";

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
mod tests {
    use super::*;

    #[test]
    fn test_hsl_primary_colors() {
        // Red: hue=0, sat=1.0, lit=0.5
        let Color::Rgb(r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5) else {
            panic!("expected Rgb");
        };
        assert_eq!(r, 255);
        assert!(g < 5);
        assert!(b < 5);

        // Green: hue=120
        let Color::Rgb(r, g, b) = hsl_to_rgb(120.0, 1.0, 0.5) else {
            panic!("expected Rgb");
        };
        assert!(r < 5);
        assert_eq!(g, 255);
        assert!(b < 5);

        // Blue: hue=240
        let Color::Rgb(r, g, b) = hsl_to_rgb(240.0, 1.0, 0.5) else {
            panic!("expected Rgb");
        };
        assert!(r < 5);
        assert!(g < 5);
        assert_eq!(b, 255);
    }

    #[test]
    fn test_state_tick_gradient() {
        let mut state = WelcomePanelState::new();
        assert_eq!(state.gradient_offset, 0);
        state.tick(80, 24);
        assert_eq!(state.gradient_offset, 3);
        state.tick(80, 24);
        assert_eq!(state.gradient_offset, 6);

        // Wraps at 360
        state.gradient_offset = 358;
        state.tick(80, 24);
        assert_eq!(state.gradient_offset, 1); // (358+3) % 360 = 1
    }

    #[test]
    fn test_braille_cycles() {
        let mut state = WelcomePanelState::new();
        assert_eq!(state.braille_offset, 0);
        state.tick(80, 24); // braille_tick = 1
        assert_eq!(state.braille_offset, 0); // not yet
        state.tick(80, 24); // braille_tick wraps, offset advances
        assert_eq!(state.braille_offset, 1);
    }

    #[test]
    fn test_fade_completes() {
        let mut state = WelcomePanelState::new();
        assert!(!state.fade_complete);
        state.start_fade();
        // fade_progress starts at 1.0, decrements 0.1 per tick
        for _ in 0..10 {
            state.tick(80, 24);
        }
        assert!(state.fade_complete);
        assert!(state.fade_progress <= 0.0);
    }

    #[test]
    fn test_rain_init() {
        let mut state = WelcomePanelState::new();
        state.ensure_rain_field(40, 10);
        assert_eq!(state.rain_columns.len(), 40);
        for col in &state.rain_columns {
            assert!(col.speed >= 0.10);
            assert!(col.speed <= 0.50);
            assert!(col.trail_len >= 4 && col.trail_len <= 9);
        }
    }

    #[test]
    fn test_rain_step() {
        let mut state = WelcomePanelState::new();
        state.ensure_rain_field(5, 10);
        let initial_ys: Vec<f32> = state.rain_columns.iter().map(|c| c.y).collect();
        state.step_rain();
        for (i, col) in state.rain_columns.iter().enumerate() {
            // Either advanced or reset (if it went off-screen)
            assert!(col.y != initial_ys[i] || col.speed == 0.0 || initial_ys[i] > 10.0 + col.trail_len as f32);
        }
    }

    #[test]
    fn test_widget_renders_visible_output() {
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

        let state = WelcomePanelState::new();
        let widget = WelcomePanelWidget::new(&state).version("0.1.0").mode("NORMAL");

        // Simulate a typical conversation area: 100 wide, 20 tall
        let area = Rect::new(0, 0, 100, 20);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Count non-space cells that were written
        let modified_cells: usize = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let cell = buf.cell((x, y)).unwrap();
                cell.symbol() != " "
            })
            .count();

        // Should have written border chars + gradient text
        assert!(
            modified_cells > 20,
            "Expected visible output, got only {modified_cells} non-space cells"
        );
    }

    #[test]
    fn test_widget_renders_small_terminal() {
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

        let state = WelcomePanelState::new();
        let widget = WelcomePanelWidget::new(&state).version("0.1.0").mode("NORMAL");

        // Tier 1: very small (height < 5)
        let area = Rect::new(0, 0, 80, 4);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let modified: usize = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf.cell((x, y)).unwrap().symbol() != " ")
            .count();
        assert!(modified > 5, "Tier 1 should render gradient text, got {modified} cells");
    }

    #[test]
    fn test_pseudo_rand_range() {
        let mut seed = 12345u64;
        for _ in 0..100 {
            let v = pseudo_rand(&mut seed);
            assert!((0.0..1.0).contains(&v), "pseudo_rand out of range: {v}");
        }
    }

    #[test]
    fn test_render_buffer_centered_without_rain() {
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

        let state = WelcomePanelState::new();
        let widget = WelcomePanelWidget::new(&state).version("0.1.0").mode("NORMAL");

        let area = Rect::new(0, 0, 100, 20);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Find which rows have content
        let mut content_rows = Vec::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if buf.cell((x, y)).unwrap().symbol() != " " {
                    content_rows.push(y);
                    break;
                }
            }
        }
        assert!(!content_rows.is_empty(), "No visible content");
        // Box should be roughly centered (middle third of the area)
        let center = area.height / 2;
        let first = content_rows[0];
        let last = *content_rows.last().unwrap();
        assert!(
            first <= center && last >= center - 2,
            "Box not centered: rows {first}..{last} in height {}",
            area.height
        );
    }

    #[test]
    fn test_render_buffer_with_rain() {
        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

        let mut state = WelcomePanelState::new();
        // Initialize rain and tick a few times
        state.ensure_rain_field(60, 13);
        for _ in 0..5 {
            state.tick(100, 20);
        }
        let widget = WelcomePanelWidget::new(&state).version("0.1.0").mode("NORMAL");

        let area = Rect::new(0, 0, 100, 20);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let mut content_rows = Vec::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if buf.cell((x, y)).unwrap().symbol() != " " {
                    content_rows.push(y);
                    break;
                }
            }
        }
        // With rain, should have content in the rain area + box area
        assert!(
            content_rows.len() > 5,
            "Expected rain + box, got {} rows",
            content_rows.len()
        );
    }
}
