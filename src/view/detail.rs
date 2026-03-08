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
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// LRU diff cache (process-scoped, thread-local for single-threaded TUI)
// ---------------------------------------------------------------------------

/// Maximum number of (commit_hash, filepath) → diff entries to keep in memory.
const DIFF_CACHE_CAPACITY: usize = 64;

/// A simple LRU cache keyed by `(commit_hash, filepath)`.
///
/// Most-recently used entries sit at the front of `entries`; eviction removes
/// from the back.  O(n) lookup is fine for the small capacities used here.
struct DiffLruCache {
    entries: VecDeque<((String, String), String)>,
    capacity: usize,
}

impl DiffLruCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get(&mut self, key: &(String, String)) -> Option<&str> {
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == key) {
            // Move to front (most-recently used).
            let entry = self.entries.remove(pos).unwrap();
            self.entries.push_front(entry);
            Some(&self.entries[0].1)
        } else {
            None
        }
    }

    fn insert(&mut self, key: (String, String), value: String) {
        // Update in place if already present.
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == &key) {
            self.entries.remove(pos);
        }
        self.entries.push_front((key, value));
        // Evict LRU entry when over capacity.
        if self.entries.len() > self.capacity {
            self.entries.pop_back();
        }
    }
}

thread_local! {
    static DIFF_CACHE: RefCell<DiffLruCache> =
        RefCell::new(DiffLruCache::new(DIFF_CACHE_CAPACITY));
}

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
    /// Vertical scroll offset for the commit metadata (body) in the header.
    metadata_scroll_y: u16,
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
            metadata_scroll_y: 0,
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
            UserEvent::ScrollDown => {
                if self.file_changes.is_empty() {
                    let total = self.commit_info.commit.body.lines().count() as u16;
                    let max_scroll = total.saturating_sub(5);
                    self.metadata_scroll_y =
                        self.metadata_scroll_y.saturating_add(count).min(max_scroll);
                } else {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_add(count);
                }
            }
            UserEvent::ScrollUp => {
                if self.file_changes.is_empty() {
                    self.metadata_scroll_y = self.metadata_scroll_y.saturating_sub(count);
                } else {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count);
                }
            }
            UserEvent::HalfPageDown => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_add(count * 10);
            }
            UserEvent::HalfPageUp => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count * 10);
            }
            UserEvent::PageDown => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_add(count * 20);
            }
            UserEvent::PageUp => {
                self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count * 20);
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
            Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).areas(area);

        self.render_header(f, header_area);
        self.render_body(f, body_area);
    }

    // ── Private helpers ────────────────────────────────────────────────────

    fn header_height(&self) -> u16 {
        // hash + author + date + blank separator + subject = 5 base lines
        let base = 5u16;
        let refs_line = if !self.commit_info.refs.is_empty() {
            1u16
        } else {
            0
        };
        let body_count = self.commit_info.commit.body.lines().count();
        let visible = body_count.min(5) as u16;
        let indicator = if body_count > 5 { 1u16 } else { 0 };
        // +1 blank line before body when body is present
        let body_lines = if body_count > 0 {
            1 + visible + indicator
        } else {
            0
        };
        base + refs_line + body_lines + 2 // +2 for top/bottom borders
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
            Span::raw(format!("{} <{}>", commit.author.name, commit.author.email)),
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
            let total_body_lines = commit.body.lines().count();
            let max_visible = 5usize;
            for body_line in commit
                .body
                .lines()
                .skip(self.metadata_scroll_y as usize)
                .take(max_visible)
            {
                lines.push(Line::raw(format!("    {body_line}")));
            }
            let remaining =
                total_body_lines.saturating_sub(self.metadata_scroll_y as usize + max_visible);
            if remaining > 0 {
                lines.push(Line::styled(
                    format!("    ... {remaining} more lines"),
                    Style::default().fg(theme.border.0),
                ));
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
            Layout::horizontal([Constraint::Length(tree_width), Constraint::Min(0)]).areas(area);

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
                    (
                        Color::Green,
                        Style::default().add_modifier(Modifier::REVERSED),
                    )
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
    ///
    /// Results are stored in the process-scoped LRU cache so that revisiting
    /// the same (commit, file) pair — even after navigating away — is instant.
    fn ensure_diff_content(&mut self) -> String {
        let key = self.diff_cache_key();
        // Check the global LRU cache first.
        let cached = DIFF_CACHE.with(|c| c.borrow_mut().get(&key).map(|s| s.to_owned()));
        if let Some(content) = cached {
            return content;
        }
        // Cache miss — load and store.
        let content = self.load_diff_content();
        DIFF_CACHE.with(|c| c.borrow_mut().insert(key, content.clone()));
        content
    }

    /// Build the cache key for the currently selected file.
    fn diff_cache_key(&self) -> (String, String) {
        let hash = self.commit_info.commit.hash.as_str().to_owned();
        let filepath = match self.file_changes.get(self.selected_file) {
            Some(FileChange::Add(p) | FileChange::Modify(p) | FileChange::Delete(p)) => p.clone(),
            Some(FileChange::Move(_, to)) => to.clone(),
            None => String::new(),
        };
        (hash, filepath)
    }

    fn load_diff_content(&self) -> String {
        let commit = &self.commit_info.commit;
        let filepath = match self.file_changes.get(self.selected_file) {
            Some(FileChange::Add(p) | FileChange::Modify(p) | FileChange::Delete(p)) => p.clone(),
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
        // Diff content is held in the global LRU cache; only reset scroll position.
        self.diff_scroll_y = 0;
        self.diff_scroll_x = 0;
    }
}
