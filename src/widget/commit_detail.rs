//! CommitDetail widget — renders commit metadata at the top of the detail view.

use crate::config::Config;
use crate::git::diff::FileChange;
use crate::git::{Commit, Ref};
use chrono::DateTime;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, StatefulWidget, Widget},
};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// CommitDetailState
// ---------------------------------------------------------------------------

pub struct CommitDetailState {
    pub commit: Commit,
    pub file_changes: Vec<FileChange>,
    pub refs: Vec<Ref>,
}

impl CommitDetailState {
    pub fn new(commit: Commit, file_changes: Vec<FileChange>, refs: Vec<Ref>) -> Self {
        Self {
            commit,
            file_changes,
            refs,
        }
    }
}

// ---------------------------------------------------------------------------
// CommitDetail widget
// ---------------------------------------------------------------------------

pub struct CommitDetail {
    config: Rc<Config>,
}

impl CommitDetail {
    pub fn new(config: Rc<Config>) -> Self {
        Self { config }
    }

    fn format_date(&self, iso_date: &str) -> String {
        let cfg = &self.config.ui.detail;
        DateTime::parse_from_rfc3339(iso_date)
            .map(|dt| {
                if cfg.date_local {
                    let local: chrono::DateTime<chrono::Local> = dt.into();
                    local.format(&cfg.date_format).to_string()
                } else {
                    dt.format(&cfg.date_format).to_string()
                }
            })
            .unwrap_or_else(|_| iso_date.to_string())
    }

    fn ref_spans<'a>(&self, refs: &'a [Ref]) -> Vec<Span<'a>> {
        let theme = &self.config.color;
        let mut spans = Vec::new();
        for r in refs {
            let (label, color) = match r {
                Ref::Branch { name, .. } => (name.as_str(), theme.list_branch_local.0),
                Ref::RemoteBranch { name, .. } => (name.as_str(), theme.list_branch_remote.0),
                Ref::Tag { name, .. } => (name.as_str(), theme.list_tag.0),
                Ref::Stash { name, .. } => (name.as_str(), theme.list_stash.0),
            };
            if !spans.is_empty() {
                spans.push(Span::raw(", "));
            }
            spans.push(Span::styled(label, Style::default().fg(color)));
        }
        spans
    }
}

impl StatefulWidget for CommitDetail {
    type State = CommitDetailState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let theme = &self.config.color;
        let label_style = Style::default().fg(theme.detail_label.0);
        let hash_style = Style::default().fg(theme.detail_hash.0);
        let value_style = Style::default().fg(theme.detail_value.0);
        let msg_style = Style::default().fg(theme.detail_message.0);

        let commit = &state.commit;
        let short_hash = commit.hash.as_short_hash();
        let date_str = self.format_date(&commit.author.date);
        let author_str = format!("{} <{}>", commit.author.name, commit.author.email);

        // Build lines to render
        let mut lines: Vec<Line> = Vec::new();

        // Line 1: Commit hash + refs
        {
            let mut spans = vec![
                Span::styled("Commit: ", label_style),
                Span::styled(short_hash, hash_style),
            ];
            let ref_spans = self.ref_spans(&state.refs);
            if !ref_spans.is_empty() {
                spans.push(Span::raw(" ("));
                spans.extend(ref_spans);
                spans.push(Span::raw(")"));
            }
            lines.push(Line::from(spans));
        }

        // Line 2: Author
        lines.push(Line::from(vec![
            Span::styled("Author: ", label_style),
            Span::styled(author_str, value_style),
        ]));

        // Line 3: Date
        lines.push(Line::from(vec![
            Span::styled("Date:   ", label_style),
            Span::styled(date_str, value_style),
        ]));

        // Blank line separator
        lines.push(Line::raw(""));

        // Subject
        lines.push(Line::styled(commit.subject.as_str(), msg_style));

        // Body lines (truncated to remaining height)
        let max_lines = area.height.saturating_sub(1) as usize; // -1 for border
        let remaining = max_lines.saturating_sub(lines.len());
        if !commit.body.is_empty() && remaining > 0 {
            for body_line in commit.body.lines().take(remaining) {
                lines.push(Line::styled(body_line.to_string(), msg_style));
            }
        }

        // Render with bottom border (separator)
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.detail_separator.0));

        let inner = block.inner(area);
        block.render(area, buf);

        for (i, line) in lines.into_iter().enumerate() {
            let y = inner.y + i as u16;
            if y >= inner.y + inner.height {
                break;
            }
            let row_area = Rect {
                y,
                height: 1,
                ..inner
            };
            line.render(row_area, buf);
        }
    }
}
