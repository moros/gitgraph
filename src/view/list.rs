//! ListView — commit log with graph, navigation, and search.

use crate::config::Config;
use crate::event::{AppEvent, Sender};
use crate::git::diff::{uncommitted_file_diff, FileChange};
use crate::keybind::{UserEvent, UserEventWithCount};
use crate::widget::commit_list::{CommitList, CommitListState, SearchState};
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

pub struct ListView {
    commit_list_state: Option<CommitListState>,
    config: Rc<Config>,
    tx: Sender,
    /// Whether the inline split detail for the uncommitted row is currently visible.
    pub show_inline_detail: bool,
    /// Index of the currently selected file in the uncommitted detail pane.
    selected_file: usize,
    /// Vertical scroll offset for the diff pane.
    diff_scroll_y: u16,
    /// Horizontal scroll offset for the diff pane.
    diff_scroll_x: u16,
    /// Cached diff: (filepath, content) for the currently selected file.
    diff_cache: Option<(String, String)>,
}

impl ListView {
    pub fn new(commit_list_state: CommitListState, config: Rc<Config>, tx: Sender) -> Self {
        let show_inline_detail = commit_list_state
            .commits
            .first()
            .map(|ci| ci.is_uncommitted)
            .unwrap_or(false)
            && config.core.show_uncommitted_detail;
        Self {
            commit_list_state: Some(commit_list_state),
            config,
            tx,
            show_inline_detail,
            selected_file: 0,
            diff_scroll_y: 0,
            diff_scroll_x: 0,
            diff_cache: None,
        }
    }

    pub fn take_list_state(&mut self) -> Option<CommitListState> {
        self.commit_list_state.take()
    }

    pub fn give_list_state(&mut self, state: CommitListState) {
        self.commit_list_state = Some(state);
    }

    pub fn is_searching(&self) -> bool {
        matches!(
            self.list_state().search_state(),
            SearchState::Searching { .. }
        )
    }

    /// Force-dismiss the inline uncommitted detail pane.
    pub fn dismiss_inline_detail(&mut self) {
        self.show_inline_detail = false;
    }

    pub fn handle_event(&mut self, ewc: UserEventWithCount, key: KeyEvent) {
        let event = ewc.event;
        let count = ewc.effective_count();

        // When searching: special keys are handled, everything else goes to search input
        if let SearchState::Searching { .. } = self.list_state().search_state() {
            match event {
                UserEvent::Confirm => {
                    self.list_state_mut().apply_search();
                    self.update_matched_message();
                }
                UserEvent::Cancel => {
                    self.list_state_mut().cancel_search();
                    self.tx.send(AppEvent::ClearStatusLine);
                }
                UserEvent::SearchToggleCase => {
                    self.list_state_mut().toggle_ignore_case();
                    self.update_search_query();
                }
                UserEvent::SearchToggleFuzzy => {
                    self.list_state_mut().toggle_fuzzy();
                    self.update_search_query();
                }
                _ => {
                    // All other keys (including unknown-mapped) go to search input
                    self.list_state_mut().handle_search_input(key);
                    self.update_search_query();
                }
            }
            return;
        }

        // When inline uncommitted detail is showing, intercept all navigation.
        // BUG 2 fix: j/k and scroll events navigate the detail pane, not the commit list.
        if self.show_inline_detail {
            match event {
                UserEvent::Quit | UserEvent::Cancel => {
                    self.show_inline_detail = false;
                    self.tx.send(AppEvent::CloseUncommittedDetail);
                }
                UserEvent::NavigateDown | UserEvent::SelectDown => {
                    self.select_detail_file_next(count);
                }
                UserEvent::NavigateUp | UserEvent::SelectUp => {
                    self.select_detail_file_prev(count);
                }
                UserEvent::GoToTop => {
                    self.selected_file = 0;
                    self.invalidate_diff();
                }
                UserEvent::GoToBottom => {
                    let file_count = self.uncommitted_files().len();
                    if file_count > 0 {
                        self.selected_file = file_count - 1;
                        self.invalidate_diff();
                    }
                }
                UserEvent::ScrollDown => {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_add(count as u16);
                }
                UserEvent::ScrollUp => {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count as u16);
                }
                UserEvent::HalfPageDown => {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_add(count as u16 * 10);
                }
                UserEvent::HalfPageUp => {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count as u16 * 10);
                }
                UserEvent::PageDown => {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_add(count as u16 * 20);
                }
                UserEvent::PageUp => {
                    self.diff_scroll_y = self.diff_scroll_y.saturating_sub(count as u16 * 20);
                }
                _ => {}
            }
            return;
        }

        match event {
            UserEvent::Quit => {
                self.tx.send(AppEvent::Quit);
            }
            UserEvent::ForceQuit => {
                self.tx.send(AppEvent::Quit);
            }
            UserEvent::NavigateDown | UserEvent::SelectDown => {
                for _ in 0..count {
                    self.list_state_mut().select_next();
                }
            }
            UserEvent::NavigateUp | UserEvent::SelectUp => {
                for _ in 0..count {
                    self.list_state_mut().select_prev();
                }
            }
            UserEvent::JumpToParent => {
                for _ in 0..count {
                    self.list_state_mut().select_parent();
                }
            }
            UserEvent::GoToTop => {
                self.list_state_mut().select_first();
            }
            UserEvent::GoToBottom => {
                self.list_state_mut().select_last();
            }
            UserEvent::ScrollDown => {
                for _ in 0..count {
                    self.list_state_mut().scroll_down();
                }
            }
            UserEvent::ScrollUp => {
                for _ in 0..count {
                    self.list_state_mut().scroll_up();
                }
            }
            UserEvent::PageDown => {
                for _ in 0..count {
                    self.list_state_mut().scroll_down_page();
                }
            }
            UserEvent::PageUp => {
                for _ in 0..count {
                    self.list_state_mut().scroll_up_page();
                }
            }
            UserEvent::HalfPageDown => {
                for _ in 0..count {
                    self.list_state_mut().scroll_down_half();
                }
            }
            UserEvent::HalfPageUp => {
                for _ in 0..count {
                    self.list_state_mut().scroll_up_half();
                }
            }
            UserEvent::SelectTop => {
                self.list_state_mut().select_high();
            }
            UserEvent::SelectMiddle => {
                self.list_state_mut().select_middle();
            }
            UserEvent::SelectBottom => {
                self.list_state_mut().select_low();
            }
            UserEvent::CopyShortHash => {
                let hash = self.list_state().selected_commit_hash().as_short_hash();
                self.tx.send(AppEvent::CopyToClipboard {
                    name: "short hash".to_string(),
                    value: hash,
                });
            }
            UserEvent::CopyFullHash => {
                let hash = self
                    .list_state()
                    .selected_commit_hash()
                    .as_str()
                    .to_string();
                self.tx.send(AppEvent::CopyToClipboard {
                    name: "full hash".to_string(),
                    value: hash,
                });
            }
            UserEvent::SearchStart => {
                self.list_state_mut().start_search();
                self.update_search_query();
            }
            UserEvent::Cancel => {
                self.list_state_mut().cancel_search();
                self.tx.send(AppEvent::ClearStatusLine);
            }
            UserEvent::Confirm => {
                let ls = self.list_state();
                let is_uncommitted = ls
                    .commits
                    .get(ls.selected + ls.offset)
                    .map(|ci| ci.is_uncommitted)
                    .unwrap_or(false);
                if is_uncommitted {
                    self.show_inline_detail = true;
                    self.selected_file = 0;
                    self.diff_scroll_y = 0;
                    self.diff_scroll_x = 0;
                    self.diff_cache = None;
                    self.tx.send(AppEvent::OpenUncommittedDetail);
                } else {
                    self.tx.send(AppEvent::OpenDetail);
                }
            }
            UserEvent::OpenRefs => {
                self.tx.send(AppEvent::OpenRefs);
            }
            UserEvent::HelpToggle => {
                self.tx.send(AppEvent::OpenHelp);
            }
            UserEvent::Refresh => {
                self.tx.send(AppEvent::Refresh);
            }
            UserEvent::UserCommand => {
                let slot = ewc.count.unwrap_or(1).clamp(1, 9) as u8;
                self.tx.send(AppEvent::OpenUserCommand(slot));
            }
            _ => {}
        }

        // Search navigation when Applied
        if let SearchState::Applied { .. } = self.list_state().search_state() {
            match event {
                UserEvent::SearchNext => {
                    self.list_state_mut().select_next_match();
                    self.update_matched_message();
                }
                UserEvent::SearchPrev => {
                    self.list_state_mut().select_prev_match();
                    self.update_matched_message();
                }
                _ => {}
            }
        }

        self.check_load_more();
    }

    /// Forward raw key input when in search mode (called from App for unmapped keys).
    pub fn raw_key_input(&mut self, key: KeyEvent) {
        if let SearchState::Searching { .. } = self.list_state().search_state() {
            self.list_state_mut().handle_search_input(key);
            self.update_search_query();
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if self.show_inline_detail {
            let [list_area, detail_area] = Layout::vertical([
                Constraint::Percentage(35),
                Constraint::Percentage(65),
            ])
            .areas(area);

            let widget = CommitList::new(self.config.clone());
            f.render_stateful_widget(widget, list_area, self.list_state_mut());

            self.render_uncommitted_detail_pane(f, detail_area);
        } else {
            let widget = CommitList::new(self.config.clone());
            f.render_stateful_widget(widget, area, self.list_state_mut());
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn list_state(&self) -> &CommitListState {
        self.commit_list_state.as_ref().unwrap()
    }

    fn list_state_mut(&mut self) -> &mut CommitListState {
        self.commit_list_state.as_mut().unwrap()
    }

    fn update_search_query(&self) {
        if let Some(query) = self.list_state().search_query_string() {
            let cursor_pos = self.list_state().search_query_cursor_position();
            let transient = self.list_state().transient_message_string();
            self.tx.send(AppEvent::UpdateStatusInput(
                query,
                Some(cursor_pos),
                transient,
            ));
        }
    }

    /// If the cursor is within 50 commits of the bottom, request more data.
    fn check_load_more(&self) {
        if self.list_state().near_bottom(50) {
            self.tx.send(AppEvent::LoadMore);
        }
    }

    fn update_matched_message(&self) {
        if let Some((msg, matched)) = self.list_state().matched_query_string() {
            if matched {
                self.tx.send(AppEvent::NotifyInfo(msg));
            } else {
                self.tx.send(AppEvent::NotifyWarn(msg));
            }
        } else {
            self.tx.send(AppEvent::ClearStatusLine);
        }
    }

    /// Returns a clone of the uncommitted files list (empty if none).
    fn uncommitted_files(&self) -> Vec<FileChange> {
        self.commit_list_state
            .as_ref()
            .and_then(|s| s.commits.first())
            .filter(|c| c.is_uncommitted)
            .map(|c| c.uncommitted_files.clone())
            .unwrap_or_default()
    }

    fn select_detail_file_next(&mut self, count: usize) {
        let file_count = self.uncommitted_files().len();
        if file_count == 0 {
            return;
        }
        let new_idx = (self.selected_file + count).min(file_count - 1);
        if new_idx != self.selected_file {
            self.selected_file = new_idx;
            self.invalidate_diff();
        }
    }

    fn select_detail_file_prev(&mut self, count: usize) {
        let new_idx = self.selected_file.saturating_sub(count);
        if new_idx != self.selected_file {
            self.selected_file = new_idx;
            self.invalidate_diff();
        }
    }

    fn invalidate_diff(&mut self) {
        self.diff_cache = None;
        self.diff_scroll_y = 0;
        self.diff_scroll_x = 0;
    }

    /// BUG 1 fix: Render a proper split detail pane with file tree (left) and diff (right).
    fn render_uncommitted_detail_pane(&mut self, f: &mut Frame, area: Rect) {
        // Collect files before any mutable borrows
        let files = self.uncommitted_files();

        if files.is_empty() {
            let block = Block::default()
                .title(" Uncommitted Changes (q to close) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::LightRed));
            let inner = block.inner(area);
            f.render_widget(block, area);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  No staged or unstaged changes detected.",
                    Style::default().fg(Color::DarkGray),
                ))),
                inner,
            );
            return;
        }

        let border_color = self.config.color.border.0;
        let tree_width = ((area.width as u32 * 30 / 100) as u16).clamp(20, 50);
        let [tree_area, diff_area] =
            Layout::horizontal([Constraint::Length(tree_width), Constraint::Min(0)]).areas(area);

        // ── File tree (left) ──────────────────────────────────────────────
        let selected_file = self.selected_file;
        let items: Vec<ListItem> = files
            .iter()
            .enumerate()
            .map(|(i, change)| {
                let (prefix, path) = match change {
                    FileChange::Add(p) => ("+ ", p.as_str()),
                    FileChange::Modify(p) => ("M ", p.as_str()),
                    FileChange::Delete(p) => ("- ", p.as_str()),
                    FileChange::Move(_from, to) => ("R ", to.as_str()),
                };
                let (prefix_color, path_style) = if i == selected_file {
                    (
                        Color::Green,
                        Style::default().add_modifier(Modifier::REVERSED),
                    )
                } else {
                    let c = match change {
                        FileChange::Add(_) => Color::Green,
                        FileChange::Modify(_) => Color::Yellow,
                        FileChange::Delete(_) => Color::Red,
                        FileChange::Move(_, _) => Color::Cyan,
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

        let file_count = files.len();
        let tree_title = format!(
            " {} file{} (j/k) ",
            file_count,
            if file_count == 1 { "" } else { "s" }
        );
        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(tree_title),
        );
        f.render_widget(list, tree_area);

        // ── Diff pane (right) ─────────────────────────────────────────────
        let content = self.ensure_uncommitted_diff(&files);
        let scroll_y = self.diff_scroll_y;
        let scroll_x = self.diff_scroll_x;

        let lines: Vec<Line> = content
            .lines()
            .skip(scroll_y as usize)
            .map(|line| {
                let line = if scroll_x > 0 {
                    line.chars()
                        .skip(scroll_x as usize)
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

        let diff_title = match files.get(self.selected_file) {
            Some(FileChange::Add(p) | FileChange::Modify(p) | FileChange::Delete(p)) => {
                format!(" {} ", p)
            }
            Some(FileChange::Move(_, to)) => format!(" {} ", to),
            None => " Diff ".to_string(),
        };

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(diff_title),
        );
        f.render_widget(paragraph, diff_area);
    }

    /// Returns the diff content for the selected uncommitted file, loading and caching it.
    fn ensure_uncommitted_diff(&mut self, files: &[FileChange]) -> String {
        let filepath = match files.get(self.selected_file) {
            Some(FileChange::Add(p) | FileChange::Modify(p) | FileChange::Delete(p)) => {
                p.clone()
            }
            Some(FileChange::Move(_, to)) => to.clone(),
            None => return String::new(),
        };

        // Check simple single-entry cache
        if let Some((cached_path, cached_content)) = &self.diff_cache {
            if cached_path == &filepath {
                return cached_content.clone();
            }
        }

        // Cache miss — load from git
        let diff = uncommitted_file_diff(Path::new("."), &filepath);
        let content = diff.content;
        self.diff_cache = Some((filepath, content.clone()));
        content
    }
}
