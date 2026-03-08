pub mod detail;
pub mod help;
pub mod list;
pub mod user_command;

use crate::config::Config;
use crate::event::Sender;
use crate::git::Ref;
use crate::keybind::UserEventWithCount;
use crate::widget::commit_list::{CommitInfo, CommitListState};
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use std::rc::Rc;

#[derive(Default)]
pub enum View {
    #[default]
    Default,
    List(Box<list::ListView>),
    Detail(Box<detail::DetailView>),
    Help(Box<help::HelpView>),
    UserCommand(Box<user_command::UserCommandView>),
}

impl View {
    pub fn handle_event(&mut self, ewc: UserEventWithCount, key: KeyEvent) {
        match self {
            View::Default => {}
            View::List(view) => view.handle_event(ewc, key),
            View::Detail(view) => view.handle_event(ewc, key),
            View::Help(view) => view.handle_event(ewc, key),
            View::UserCommand(view) => view.handle_event(ewc, key),
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        match self {
            View::Default => {}
            View::List(view) => view.render(f, area),
            View::Detail(view) => view.render(f, area),
            View::Help(view) => view.render(f, area),
            View::UserCommand(view) => view.render(f, area),
        }
    }

    /// Forward a raw key event when no UserEvent mapped (for search input mode).
    pub fn raw_key_input(&mut self, key: KeyEvent) {
        match self {
            View::List(view) => view.raw_key_input(key),
            _ => {}
        }
    }

    pub fn of_list(commit_list_state: CommitListState, config: Rc<Config>, tx: Sender) -> Self {
        View::List(Box::new(list::ListView::new(commit_list_state, config, tx)))
    }

    pub fn of_detail(
        commit_info: CommitInfo,
        commit_index: usize,
        config: Rc<Config>,
        tx: Sender,
    ) -> Self {
        View::Detail(Box::new(detail::DetailView::new(
            commit_info,
            commit_index,
            config,
            tx,
        )))
    }

    pub fn of_help(before: View, config: Rc<Config>, tx: Sender) -> Self {
        View::Help(Box::new(help::HelpView::new(before, config, tx)))
    }

    pub fn of_user_command(
        commit_list_state: CommitListState,
        slot: u8,
        config: Rc<Config>,
        tx: Sender,
    ) -> Self {
        View::UserCommand(Box::new(user_command::UserCommandView::new(
            commit_list_state,
            slot,
            config,
            tx,
        )))
    }

    pub fn take_list_state(&mut self) -> Option<CommitListState> {
        match self {
            View::List(view) => view.take_list_state(),
            View::UserCommand(_) => {
                // Take the view out to get the list state
                let placeholder = std::mem::take(self);
                if let View::UserCommand(ucv) = placeholder {
                    let state = ucv.take_list_state();
                    Some(state)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn give_list_state(&mut self, state: CommitListState) {
        match self {
            View::List(view) => view.give_list_state(state),
            _ => {}
        }
    }

    /// Returns the absolute commit index of the currently displayed commit in Detail view.
    pub fn detail_commit_index(&self) -> Option<usize> {
        match self {
            View::Detail(view) => Some(view.commit_index),
            _ => None,
        }
    }

    /// Returns true if the status line is currently showing search input.
    pub fn is_in_input_mode(&self) -> bool {
        match self {
            View::List(view) => view.is_searching(),
            _ => false,
        }
    }

    /// Take the `before` view from a HelpView, consuming self.
    pub fn take_help_before(self) -> Option<View> {
        if let View::Help(hv) = self {
            Some(hv.before)
        } else {
            None
        }
    }
}
