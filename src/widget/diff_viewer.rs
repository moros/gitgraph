//! DiffViewer widget — renders diff content for a selected file.

use crate::config::Config;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    text::Line,
    widgets::{Paragraph, StatefulWidget, Widget},
};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// DiffLine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum DiffLine {
    Header(String),
    HunkHeader(String),
    Added(String),
    Removed(String),
    Context(String),
    Empty,
}

// ---------------------------------------------------------------------------
// DiffViewerState
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct DiffViewerState {
    pub lines: Vec<DiffLine>,
    pub filename: String,
    pub vertical_offset: usize,
    pub horizontal_offset: usize,
    pub height: usize,
}

impl DiffViewerState {
    pub fn scroll_down(&mut self, n: usize) {
        let max = self.lines.len().saturating_sub(self.height);
        self.vertical_offset = (self.vertical_offset + n).min(max);
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.vertical_offset = self.vertical_offset.saturating_sub(n);
    }

    pub fn scroll_down_page(&mut self) {
        self.scroll_down(self.height);
    }

    pub fn scroll_up_page(&mut self) {
        self.scroll_up(self.height);
    }

    pub fn scroll_left(&mut self, n: usize) {
        self.horizontal_offset = self.horizontal_offset.saturating_sub(n);
    }

    pub fn scroll_right(&mut self, n: usize) {
        self.horizontal_offset += n;
    }

    /// Parse raw diff text into DiffLines. Resets offsets.
    pub fn set_content(&mut self, filename: &str, diff_content: &str) {
        self.filename = filename.to_string();
        self.vertical_offset = 0;
        self.horizontal_offset = 0;
        self.lines = diff_content
            .lines()
            .map(|line| {
                if line.starts_with("@@") {
                    DiffLine::HunkHeader(line.to_string())
                } else if line.starts_with("diff ")
                    || line.starts_with("---")
                    || line.starts_with("+++")
                    || line.starts_with("index ")
                {
                    DiffLine::Header(line.to_string())
                } else if let Some(stripped) = line.strip_prefix('+') {
                    DiffLine::Added(stripped.to_string())
                } else if let Some(stripped) = line.strip_prefix('-') {
                    DiffLine::Removed(stripped.to_string())
                } else if line.is_empty() {
                    DiffLine::Empty
                } else {
                    DiffLine::Context(line.to_string())
                }
            })
            .collect();
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.filename.clear();
        self.vertical_offset = 0;
        self.horizontal_offset = 0;
    }
}

// ---------------------------------------------------------------------------
// DiffViewer widget
// ---------------------------------------------------------------------------

pub struct DiffViewer {
    config: Rc<Config>,
}

impl DiffViewer {
    pub fn new(config: Rc<Config>) -> Self {
        Self { config }
    }
}

impl StatefulWidget for DiffViewer {
    type State = DiffViewerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        state.height = area.height as usize;

        if state.lines.is_empty() {
            let msg = Paragraph::new("Select a file to view diff")
                .alignment(Alignment::Center)
                .style(Style::default().fg(self.config.color.detail_label.0));
            msg.render(area, buf);
            return;
        }

        let theme = &self.config.color;
        let gutter_width = 4u16;
        let content_x = area.x + gutter_width;
        let content_width = area.width.saturating_sub(gutter_width);

        let visible = state
            .lines
            .iter()
            .skip(state.vertical_offset)
            .take(area.height as usize);

        for (row_idx, diff_line) in visible.enumerate() {
            let y = area.y + row_idx as u16;
            let line_no = state.vertical_offset + row_idx + 1;

            // Gutter with line number
            let gutter_area = Rect {
                x: area.x,
                y,
                width: gutter_width,
                height: 1,
            };
            let gutter_text = format!("{:>3} ", line_no);
            Line::styled(gutter_text, Style::default().fg(theme.detail_label.0))
                .render(gutter_area, buf);

            if content_width == 0 {
                continue;
            }

            let content_area = Rect {
                x: content_x,
                y,
                width: content_width,
                height: 1,
            };

            let (text, style) = match diff_line {
                DiffLine::Header(s) => (s.as_str(), Style::default().fg(theme.diff_file_header.0)),
                DiffLine::HunkHeader(s) => {
                    (s.as_str(), Style::default().fg(theme.diff_hunk_header.0))
                }
                DiffLine::Added(s) => (s.as_str(), Style::default().fg(theme.diff_added.0)),
                DiffLine::Removed(s) => (s.as_str(), Style::default().fg(theme.diff_removed.0)),
                DiffLine::Context(s) => (s.as_str(), Style::default().fg(theme.diff_context.0)),
                DiffLine::Empty => ("", Style::default()),
            };

            // Apply horizontal scroll
            let h_off = state.horizontal_offset;
            let chars: String = text.chars().skip(h_off).collect();
            Line::styled(chars, style).render(content_area, buf);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_diff() -> &'static str {
        "diff --git a/foo.rs b/foo.rs\n\
         index abc..def 100644\n\
         --- a/foo.rs\n\
         +++ b/foo.rs\n\
         @@ -1,3 +1,4 @@\n\
          context line\n\
         -removed line\n\
         +added line\n\
          another context"
    }

    #[test]
    fn set_content_parses_diff_lines() {
        let mut state = DiffViewerState::default();
        state.set_content("foo.rs", sample_diff());

        assert_eq!(state.filename, "foo.rs");
        assert_eq!(state.vertical_offset, 0);
        assert_eq!(state.horizontal_offset, 0);

        let kinds: Vec<&str> = state
            .lines
            .iter()
            .map(|l| match l {
                DiffLine::Header(_) => "header",
                DiffLine::HunkHeader(_) => "hunk",
                DiffLine::Added(_) => "added",
                DiffLine::Removed(_) => "removed",
                DiffLine::Context(_) => "context",
                DiffLine::Empty => "empty",
            })
            .collect();

        assert_eq!(
            kinds,
            [
                "header", "header", "header", "header", "hunk", "context", "removed", "added",
                "context"
            ]
        );
    }

    #[test]
    fn set_content_resets_offsets() {
        let mut state = DiffViewerState::default();
        state.vertical_offset = 5;
        state.horizontal_offset = 10;
        state.set_content("x.rs", sample_diff());
        assert_eq!(state.vertical_offset, 0);
        assert_eq!(state.horizontal_offset, 0);
    }

    #[test]
    fn scroll_down_capped_at_max() {
        let mut state = DiffViewerState::default();
        state.set_content("x.rs", sample_diff());
        state.height = 3;
        let total = state.lines.len();
        state.scroll_down(1000);
        assert_eq!(state.vertical_offset, total - 3);
    }

    #[test]
    fn scroll_up_stops_at_zero() {
        let mut state = DiffViewerState::default();
        state.set_content("x.rs", sample_diff());
        state.scroll_up(100);
        assert_eq!(state.vertical_offset, 0);
    }

    #[test]
    fn scroll_page_moves_by_height() {
        let mut state = DiffViewerState::default();
        state.set_content("x.rs", sample_diff());
        state.height = 3;
        state.scroll_down_page();
        assert_eq!(state.vertical_offset, 3);
        state.scroll_up_page();
        assert_eq!(state.vertical_offset, 0);
    }

    #[test]
    fn clear_empties_state() {
        let mut state = DiffViewerState::default();
        state.set_content("x.rs", sample_diff());
        state.clear();
        assert!(state.lines.is_empty());
        assert!(state.filename.is_empty());
    }

    #[test]
    fn added_and_removed_lines_strip_prefix() {
        let diff = "+added content\n-removed content\n context line";
        let mut state = DiffViewerState::default();
        state.set_content("f.rs", diff);

        match &state.lines[0] {
            DiffLine::Added(s) => assert_eq!(s, "added content"),
            other => panic!("expected Added, got {other:?}"),
        }
        match &state.lines[1] {
            DiffLine::Removed(s) => assert_eq!(s, "removed content"),
            other => panic!("expected Removed, got {other:?}"),
        }
        match &state.lines[2] {
            DiffLine::Context(s) => assert_eq!(s, " context line"),
            other => panic!("expected Context, got {other:?}"),
        }
    }

    #[test]
    fn header_lines_detected_correctly() {
        let diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\nindex 000..fff 100644";
        let mut state = DiffViewerState::default();
        state.set_content("x", diff);
        assert_eq!(state.lines.len(), 4);
        assert!(matches!(state.lines[0], DiffLine::Header(_)));
        assert!(matches!(state.lines[1], DiffLine::Header(_)));
        assert!(matches!(state.lines[2], DiffLine::Header(_)));
        assert!(matches!(state.lines[3], DiffLine::Header(_)));
    }

    #[test]
    fn hunk_header_detected() {
        let diff = "@@ -1,3 +1,4 @@ fn foo()";
        let mut state = DiffViewerState::default();
        state.set_content("x", diff);
        assert!(matches!(state.lines[0], DiffLine::HunkHeader(_)));
    }

    #[test]
    fn empty_lines_become_empty_variant() {
        let diff = "+added\n\n-removed";
        let mut state = DiffViewerState::default();
        state.set_content("f.rs", diff);
        assert!(matches!(state.lines[1], DiffLine::Empty));
    }

    #[test]
    fn scroll_right_and_left() {
        let mut state = DiffViewerState::default();
        state.scroll_right(5);
        assert_eq!(state.horizontal_offset, 5);
        state.scroll_left(3);
        assert_eq!(state.horizontal_offset, 2);
        state.scroll_left(100);
        assert_eq!(state.horizontal_offset, 0);
    }

    #[test]
    fn set_content_empty_string() {
        let mut state = DiffViewerState::default();
        state.set_content("empty.rs", "");
        assert!(state.lines.is_empty());
        assert_eq!(state.filename, "empty.rs");
    }
}
