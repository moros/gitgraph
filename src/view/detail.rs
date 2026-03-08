//! DetailView — commit detail with metadata header, file tree, and diff pane.

use crate::config::Config;
use crate::event::{AppEvent, Sender};
use crate::git::diff::{get_diff_summary, get_initial_commit_additions, FileChange};
use crate::keybind::{UserEvent, UserEventWithCount};
use crate::widget::commit_list::CommitInfo;
use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::path::Path;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// DetailView
// ---------------------------------------------------------------------------

pub struct DetailView {
    /// Absolute index of this commit in the full commits list.
    pub commit_index: usize,
    /// Commit metadata and refs for display.
    commit_info: CommitInfo,
    /// Files changed in this commit.
    file_changes: Vec<FileChange>,
    /// Selected file index in the file tree (0-based, within file_changes).
    selected_file: usize,
    /// Vertical scroll offset for the diff pane.
    diff_scroll_y: u16,
    /// Horizontal scroll offset for the diff pane (in columns).
    diff_scroll_x: u16,
    /// Cached diff content for the currently selected file.
    diff_content: Option<String>,
    config: Rc<Config>,
    tx: Sender,
}

impl DetailView {
    /// Construct a new DetailView for the given commit.
    ///
    /// `commit_index` is the absolute index in the full commits vec (offset + selected).
    pub fn new(
        commit_info: CommitInfo,
        commit_index: usize,
        config: Rc<Config>,
        tx: Sender,
    ) -> Self {
        let hash = &commit_info.commit.hash;
        let is_initial = commit_info.commit.parent_hashes.is_empty();

        let file_changes = if is_initial {
            get_initial_commit_additions(Path::new("."), hash)
        } else {
            get_diff_summary(Path::new("."), hash)
        };

        Self {
            commit_index,
            commit_info,
            file_changes,
            selected_file: 0,
            diff_scroll_y: 0,
            diff_scroll_x: 0,
            diff_content: None,
            config,
            tx,
        }
    }

    // ── Public API ─────────────────────────────────────────────────────────

    pub fn handle_event(&mut self, ewc: UserEventWithCount, _key: KeyEvent) {
        let event = ewc.event;
        let count = ewc.effective_count() as u16;

        match event {
            UserEvent::Cancel | UserEvent::Quit => {
                self.tx.send(AppEvent::CloseDetail);
            }
            UserEvent::NavigateDown | UserEvent::SelectDown => {
                self.select_file_next(count as usize);
            }
            UserEvent::NavigateUp | UserEvent::SelectUp => {
                self.select_file_prev(count as usize);
            }
            UserEvent::GoToTop => {
                self.selected_file = 0;
                self.invalidate_diff();
            }
            UserEvent::GoToBottom => {
                if !self.file_changes.is_empty() {
                    self.selected_file = self.file_changes.len() - 1;
                    self.invalidate_diff();
                }
            }
            UserEvent::ScrollDiffDown => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_add(count);
            }
            UserEvent::ScrollDiffUp => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count);
            }
            UserEvent::ScrollDiffPageDown => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_add(count * 20);
            }
            UserEvent::ScrollDiffPageUp => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count * 20);
            }
            UserEvent::ScrollDiffLeft => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_sub(count * 5);
            }
            UserEvent::ScrollDiffRight => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_add(count * 5);
            }
            UserEvent::ScrollDiffFastLeft => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_sub(count * 20);
            }
            UserEvent::ScrollDiffFastRight => {
                self.diff_scroll_x = self.diff_scroll_x.saturating_add(count * 20);
            }
            UserEvent::NextCommit => {
                self.tx.send(AppEvent::DetailNextCommit);
            }
            UserEvent::PrevCommit => {
                self.tx.send(AppEvent::DetailPrevCommit);
            }
            UserEvent::CopyShortHash => {
                let hash = self.commit_info.commit.hash.as_short_hash();
                self.tx.send(AppEvent::CopyToClipboard {
                    name: "short hash".to_string(),
                    value: hash,
                });
            }
            UserEvent::CopyFullHash => {
                let hash = self.commit_info.commit.hash.as_str().to_string();
                self.tx.send(AppEvent::CopyToClipboard {
                    name: "full hash".to_string(),
                    value: hash,
                });
            }
            _ => {}
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        // Layout: metadata header (fixed height) | body (file tree + diff)
        let header_height = self.header_height();
        let [header_area, body_area] =
            Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)])
                .areas(area);

        self.render_header(f, header_area);
        self.render_body(f, body_area);
    }

    // ── Private helpers ────────────────────────────────────────────────────

    fn header_height(&self) -> u16 {
        // border top + author + date + hash + subject + body lines + border bottom
        let body_line_count = self
            .commit_info
            .commit
            .body
            .lines()
            .count()
            .min(5) as u16;
        // author/committer row + date row + hash row + subject row + body rows
        4 + body_line_count + 2 // +2 for top/bottom borders
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let theme = &self.config.color;
        let commit = &self.commit_info.commit;

        let _hash_short = commit.hash.as_short_hash();
        let hash_full = commit.hash.as_str();

        // Build ref spans
        let mut ref_spans: Vec<Span> = Vec::new();
        for r in &self.commit_info.refs {
            ref_spans.push(Span::styled(
                format!(" {} ", r.name()),
                Style::default().fg(Color::Black).bg(theme.border.0),
            ));
            ref_spans.push(Span::raw(" "));
        }

        let mut lines: Vec<Line> = Vec::new();

        // Hash line
        lines.push(Line::from(vec![
            Span::styled("commit ", Style::default().fg(theme.border.0)),
            Span::styled(
                hash_full.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Refs line (if any)
        if !ref_spans.is_empty() {
            lines.push(Line::from(ref_spans));
        }

        // Author
        lines.push(Line::from(vec![
            Span::styled("Author: ", Style::default().fg(theme.border.0)),
            Span::raw(format!(
                "{} <{}>",
                commit.author.name, commit.author.email
            )),
        ]));

        // Date
        lines.push(Line::from(vec![
            Span::styled("Date:   ", Style::default().fg(theme.border.0)),
            Span::raw(commit.author.date.as_str()),
        ]));

        // Subject
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("    {}", commit.subject),
            Style::default().add_modifier(Modifier::BOLD),
        )));

        // Body
        if !commit.body.is_empty() {
            lines.push(Line::raw(""));
            for body_line in commit.body.lines().take(5) {
                lines.push(Line::raw(format!("    {body_line}")));
            }
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border.0))
                .title(" Commit "),
        );
        f.render_widget(paragraph, area);
    }

    fn render_body(&mut self, f: &mut Frame, area: Rect) {
        // Left: file tree (~30% width, min 20, max 50)
        let tree_width = ((area.width as u32 * 30 / 100) as u16).clamp(20, 50);
        let [tree_area, diff_area] =
            Layout::horizontal([Constraint::Length(tree_width), Constraint::Min(0)])
                .areas(area);

        self.render_file_tree(f, tree_area);
        self.render_diff_pane(f, diff_area);
    }

    fn render_file_tree(&self, f: &mut Frame, area: Rect) {
        let border_color = self.config.color.border.0;

        let items: Vec<ListItem> = self
            .file_changes
            .iter()
            .enumerate()
            .map(|(i, change)| {
                let (prefix, path) = match change {
                    FileChange::Add(p) => ("+ ", p.as_str()),
                    FileChange::Modify(p) => ("M ", p.as_str()),
                    FileChange::Delete(p) => ("- ", p.as_str()),
                    FileChange::Move(_from, to) => ("R ", to.as_str()),
                };
                let (prefix_color, path_style) = if i == self.selected_file {
                    (Color::Green, Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    let c = match change {
                        FileChange::Add(_) => Color::Green,
                        FileChange::Modify(_) => Color::Blue,
                        FileChange::Delete(_) => Color::Red,
                        FileChange::Move(_, _) => Color::Yellow,
                    };
                    (c, Style::default())
                };

                let line = Line::from(vec![
                    Span::styled(prefix, Style::default().fg(prefix_color)),
                    Span::styled(path, path_style),
                ]);
                ListItem::new(line)
            })
            .collect();

        let file_count = self.file_changes.len();
        let title = if file_count == 1 {
            " 1 file ".to_string()
        } else {
            format!(" {file_count} files ")
        };

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title),
        );
        f.render_widget(list, area);
    }

    fn render_diff_pane(&mut self, f: &mut Frame, area: Rect) {
        let border_color = self.config.color.border.0;

        // Load diff content (mutable borrow resolved before immutable uses)
        let content = self.ensure_diff_content();
        let content_clone = content.clone();

        let lines: Vec<Line> = content_clone
            .lines()
            .skip(self.diff_scroll_y as usize)
            .map(|line| {
                let line = if self.diff_scroll_x > 0 {
                    line.chars()
                        .skip(self.diff_scroll_x as usize)
                        .collect::<String>()
                } else {
                    line.to_string()
                };
                if line.starts_with('+') && !line.starts_with("+++") {
                    Line::styled(line, Style::default().fg(Color::Green))
                } else if line.starts_with('-') && !line.starts_with("---") {
                    Line::styled(line, Style::default().fg(Color::Red))
                } else if line.starts_with("@@") {
                    Line::styled(line, Style::default().fg(Color::Cyan))
                } else {
                    Line::raw(line)
                }
            })
            .collect();

        let file_name = self
            .file_changes
            .get(self.selected_file)
            .map(|fc| match fc {
                FileChange::Add(p) | FileChange::Modify(p) | FileChange::Delete(p) => {
                    p.as_str().to_string()
                }
                FileChange::Move(_, to) => to.clone(),
            })
            .unwrap_or_default();

        let title = if file_name.is_empty() {
            " Diff ".to_string()
        } else {
            format!(" {file_name} ")
        };

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title),
        );
        f.render_widget(paragraph, area);
    }

    /// Returns the diff content for the currently selected file, loading it if needed.
    fn ensure_diff_content(&mut self) -> String {
        if self.diff_content.is_none() {
            let content = self.load_diff_content();
            self.diff_content = Some(content);
        }
        self.diff_content.clone().unwrap_or_default()
    }

    fn load_diff_content(&self) -> String {
        let commit = &self.commit_info.commit;
        let filepath = match self.file_changes.get(self.selected_file) {
            Some(FileChange::Add(p) | FileChange::Modify(p) | FileChange::Delete(p)) => {
                p.clone()
            }
            Some(FileChange::Move(_, to)) => to.clone(),
            None => return String::new(),
        };

        let parent = commit.parent_hashes.first();
        let file_diff = match parent {
            Some(parent_hash) => {
                crate::git::diff::file_diff(Path::new("."), parent_hash, &commit.hash, &filepath)
            }
            None => crate::git::diff::FileDiff {
                filename: filepath.clone(),
                content: String::new(),
                ..Default::default()
            },
        };
        file_diff.content
    }

    fn select_file_next(&mut self, count: usize) {
        if self.file_changes.is_empty() {
            return;
        }
        let new_idx = (self.selected_file + count).min(self.file_changes.len() - 1);
        if new_idx != self.selected_file {
            self.selected_file = new_idx;
            self.invalidate_diff();
        }
    }

    fn select_file_prev(&mut self, count: usize) {
        let new_idx = self.selected_file.saturating_sub(count);
        if new_idx != self.selected_file {
            self.selected_file = new_idx;
            self.invalidate_diff();
        }
    }

    fn invalidate_diff(&mut self) {
        self.diff_content = None;
        self.diff_scroll_y = 0;
        self.diff_scroll_x = 0;
    }
}
