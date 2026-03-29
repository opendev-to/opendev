//! Custom text selection state for mouse-based copy.

use ratatui::layout::Rect;

/// A position within the content (line index + character offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPosition {
    /// Index into the wrapped line array.
    pub line_index: usize,
    /// Character (column) offset within the wrapped line.
    pub char_offset: usize,
}

impl TextPosition {
    pub fn new(line_index: usize, char_offset: usize) -> Self {
        Self {
            line_index,
            char_offset,
        }
    }
}

impl PartialOrd for TextPosition {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TextPosition {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.line_index
            .cmp(&other.line_index)
            .then(self.char_offset.cmp(&other.char_offset))
    }
}

/// A selection range defined by anchor (where mouse-down happened) and cursor (current drag position).
#[derive(Debug, Clone, Copy)]
pub struct SelectionRange {
    /// Where the selection started (mouse-down).
    pub anchor: TextPosition,
    /// Current end of selection (follows the mouse).
    pub cursor: TextPosition,
}

impl SelectionRange {
    /// Returns (start, end) in document order.
    pub fn ordered(&self) -> (TextPosition, TextPosition) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    /// Returns true if the given line index is within the selection.
    pub fn contains_line(&self, line_index: usize) -> bool {
        let (start, end) = self.ordered();
        line_index >= start.line_index && line_index <= end.line_index
    }

    /// Returns the column range selected on a given line.
    /// Returns (start_col, end_col) where end_col is exclusive.
    pub fn columns_on_line(&self, line_index: usize, line_width: usize) -> Option<(usize, usize)> {
        let (start, end) = self.ordered();
        if line_index < start.line_index || line_index > end.line_index {
            return None;
        }
        let col_start = if line_index == start.line_index {
            start.char_offset
        } else {
            0
        };
        let col_end = if line_index == end.line_index {
            end.char_offset
        } else {
            line_width
        };
        if col_start >= col_end {
            None
        } else {
            Some((col_start, col_end))
        }
    }
}

/// State for tracking an active text selection.
#[derive(Debug, Default)]
pub struct SelectionState {
    /// Whether a selection is currently active (mouse button held).
    pub active: bool,
    /// The current selection range (if any text is selected).
    pub range: Option<SelectionRange>,
    /// The conversation content area rect (set after each render).
    pub conversation_area: Rect,
    /// The actual scroll position (lines from top) used in the last render.
    pub actual_scroll: usize,
    /// Total content lines in the last render.
    pub total_content_lines: usize,
    /// Auto-scroll direction: -1 = up (toward top), 1 = down (toward bottom), None = no auto-scroll.
    pub auto_scroll_direction: Option<i8>,
}

impl SelectionState {
    /// Map a screen position (col, row) to a content-space TextPosition.
    ///
    /// - `col`, `row`: absolute terminal coordinates
    /// - Returns `None` if the position is outside the conversation area.
    pub fn screen_to_text_position(&self, col: u16, row: u16) -> Option<TextPosition> {
        let area = self.conversation_area;
        if area.width == 0 || area.height == 0 {
            return None;
        }
        // Allow slightly out-of-area for auto-scroll (clamp later)
        let rel_col = col.saturating_sub(area.x) as usize;
        let rel_row = if row < area.y {
            0
        } else {
            (row - area.y) as usize
        };
        let line_index = self.actual_scroll + rel_row;
        let char_offset = rel_col.min(area.width as usize);
        Some(TextPosition::new(line_index, char_offset))
    }

    /// Check if a screen position is within the conversation area.
    pub fn is_in_conversation_area(&self, col: u16, row: u16) -> bool {
        let area = self.conversation_area;
        col >= area.x && col < area.x + area.width && row >= area.y && row < area.y + area.height
    }

    /// Clear the selection state.
    pub fn clear(&mut self) {
        self.active = false;
        self.range = None;
        self.auto_scroll_direction = None;
    }

    /// Start a selection at the given screen position.
    pub fn start(&mut self, col: u16, row: u16) {
        if let Some(pos) = self.screen_to_text_position(col, row) {
            self.active = true;
            self.range = Some(SelectionRange {
                anchor: pos,
                cursor: pos,
            });
            self.auto_scroll_direction = None;
        }
    }

    /// Extend the selection to the given screen position (during drag).
    pub fn extend(&mut self, col: u16, row: u16) {
        let area = self.conversation_area;

        // Set auto-scroll direction based on position relative to conversation area
        if row < area.y + 1 {
            self.auto_scroll_direction = Some(-1); // scroll up
        } else if row >= area.y + area.height.saturating_sub(1) {
            self.auto_scroll_direction = Some(1); // scroll down
        } else {
            self.auto_scroll_direction = None;
        }

        if let Some(pos) = self.screen_to_text_position(col, row)
            && let Some(ref mut range) = self.range
        {
            range.cursor = pos;
        }
    }

    /// Finalize the selection on mouse-up. Returns true if there's a non-empty selection.
    pub fn finalize(&mut self) -> bool {
        self.active = false;
        self.auto_scroll_direction = None;
        self.range.is_some_and(|r| {
            let (start, end) = r.ordered();
            start != end
        })
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
