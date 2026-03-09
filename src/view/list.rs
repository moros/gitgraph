//! ListView — commit log with graph, navigation, and search.

use crate::config::Config;
use crate::event::{AppEvent, Sender};
use crate::git::FileChange;
use crate::keybind::{UserEvent, UserEventWithCount};
use crate::widget::commit_list::{CommitList, CommitListState, SearchState};
use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use std::rc::Rc;

pub struct ListView {
    commit_list_state: Option<CommitListState>,
    config: Rc<Config>,
    tx: Sender,
    /// Whether the inline split detail for the uncommitted row is currently visible.
    pub show_inline_detail: bool,
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

        // Dismiss inline uncommitted detail with q
        if self.show_inline_detail && matches!(event, UserEvent::Quit | UserEvent::Cancel) {
            self.show_inline_detail = false;
            self.tx.send(AppEvent::CloseUncommittedDetail);
            return;
        }

        match event {
            UserEvent::Quit => {
                if self.show_inline_detail {
                    // q collapses the inline split detail rather than quitting.
                    self.show_inline_detail = false;
                } else {
                    self.tx.send(AppEvent::Quit);
                }
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
                Constraint::Percentage(25),
                Constraint::Percentage(75),
            ])
            .areas(area);

            let widget = CommitList::new(self.config.clone());
            f.render_stateful_widget(widget, list_area, self.list_state_mut());

            // Collect files after the stateful render (borrow ends).
            let files: Vec<FileChange> = self
                .commit_list_state
                .as_ref()
                .and_then(|s| s.commits.first())
                .filter(|c| c.is_uncommitted)
                .map(|c| c.uncommitted_files.clone())
                .unwrap_or_default();

            render_inline_detail_pane(f, detail_area, &files);
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
}

// ---------------------------------------------------------------------------
// Inline detail pane renderer
// ---------------------------------------------------------------------------

/// Render the bottom split pane showing staged/unstaged file changes.
fn render_inline_detail_pane(f: &mut Frame, area: Rect, files: &[FileChange]) {
    let block = Block::default()
        .title(" Uncommitted Changes (q to close) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightRed));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if files.is_empty() {
        let msg = Line::from(
            Span::styled(
                "  No staged or unstaged changes detected.",
                Style::default().fg(Color::DarkGray),
            )
        );
        f.render_widget(
            ratatui::widgets::Paragraph::new(msg),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = files
        .iter()
        .map(|change| match change {
            FileChange::Add(path) => ListItem::new(Line::from(vec![
                Span::styled("A  ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                Span::raw(path.clone()),
            ])),
            FileChange::Modify(path) => ListItem::new(Line::from(vec![
                Span::styled("M  ", Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD)),
                Span::raw(path.clone()),
            ])),
            FileChange::Delete(path) => ListItem::new(Line::from(vec![
                Span::styled("D  ", Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)),
                Span::raw(path.clone()),
            ])),
            FileChange::Move(from, to) => ListItem::new(Line::from(vec![
                Span::styled("R  ", Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
                Span::raw(format!("{from} → {to}")),
            ])),
        })
        .collect();

    f.render_widget(List::new(items), inner);
}
