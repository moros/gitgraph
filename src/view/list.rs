//! ListView — commit log with graph, navigation, and search.

use crate::config::Config;
use crate::event::{AppEvent, Sender};
use crate::keybind::{UserEvent, UserEventWithCount};
use crate::widget::commit_list::{CommitList, CommitListState, SearchState};
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::rc::Rc;

pub struct ListView {
    commit_list_state: Option<CommitListState>,
    config: Rc<Config>,
    tx: Sender,
}

impl ListView {
    pub fn new(commit_list_state: CommitListState, config: Rc<Config>, tx: Sender) -> Self {
        Self {
            commit_list_state: Some(commit_list_state),
            config,
            tx,
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

        match event {
            UserEvent::Quit | UserEvent::ForceQuit => {
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
                let hash = self.list_state().selected_commit_hash().as_str().to_string();
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
                self.tx.send(AppEvent::OpenDetail);
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
    }

    /// Forward raw key input when in search mode (called from App for unmapped keys).
    pub fn raw_key_input(&mut self, key: KeyEvent) {
        if let SearchState::Searching { .. } = self.list_state().search_state() {
            self.list_state_mut().handle_search_input(key);
            self.update_search_query();
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let widget = CommitList::new(self.config.clone());
        f.render_stateful_widget(widget, area, self.list_state_mut());
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
