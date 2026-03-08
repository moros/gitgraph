//! RefsView — side-panel refs browser with commit list on the left.
//!
//! Tab from the list view opens this view. The refs tree is displayed on the
//! right; navigating to a ref scrolls the commit list to that ref's commit.
//! Tab/q/Esc returns to the list view.

use std::rc::Rc;

use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    Frame,
};

use crate::{
    config::Config,
    event::{AppEvent, Sender},
    git::Ref,
    keybind::{UserEvent, UserEventWithCount},
    widget::{
        commit_list::{CommitList, CommitListState},
        ref_list::{RefList, RefListState},
    },
};

pub struct RefsView {
    commit_list_state: Option<CommitListState>,
    ref_list_state: RefListState,
    refs: Vec<Ref>,
    config: Rc<Config>,
    tx: Sender,
}

impl RefsView {
    pub fn new(
        commit_list_state: CommitListState,
        refs: Vec<Ref>,
        config: Rc<Config>,
        tx: Sender,
    ) -> Self {
        Self {
            commit_list_state: Some(commit_list_state),
            ref_list_state: RefListState::new(),
            refs,
            config,
            tx,
        }
    }

    pub fn take_list_state(&mut self) -> Option<CommitListState> {
        self.commit_list_state.take()
    }

    pub fn handle_event(&mut self, ewc: UserEventWithCount, _key: KeyEvent) {
        let event = ewc.event;
        let count = ewc.effective_count();

        match event {
            // Tab (OpenRefs), q (Quit), Esc (Cancel) all close the refs panel
            UserEvent::Quit | UserEvent::Cancel | UserEvent::OpenRefs => {
                self.tx.send(AppEvent::CloseRefs);
            }
            UserEvent::NavigateDown | UserEvent::SelectDown => {
                for _ in 0..count {
                    self.ref_list_state.select_next();
                }
                self.update_commit_list_selected();
            }
            UserEvent::NavigateUp | UserEvent::SelectUp => {
                for _ in 0..count {
                    self.ref_list_state.select_prev();
                }
                self.update_commit_list_selected();
            }
            UserEvent::GoToTop => {
                self.ref_list_state.select_first();
                self.update_commit_list_selected();
            }
            UserEvent::GoToBottom => {
                self.ref_list_state.select_last();
                self.update_commit_list_selected();
            }
            UserEvent::Confirm => {
                // Enter: jump the list to the selected ref's commit (already synced)
                self.update_commit_list_selected();
            }
            UserEvent::CopyShortHash => {
                self.copy_ref_name();
            }
            _ => {}
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let refs_width = self.config.ui.refs.width as u16;
        let [list_area, refs_area] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(refs_width)])
                .areas(area);

        let commit_list = CommitList::new(self.config.clone());
        f.render_stateful_widget(commit_list, list_area, self.list_state_mut());

        let ref_list = RefList::new(&self.refs, self.config.clone());
        f.render_stateful_widget(ref_list, refs_area, &mut self.ref_list_state);
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn list_state_mut(&mut self) -> &mut CommitListState {
        self.commit_list_state.as_mut().unwrap()
    }

    fn update_commit_list_selected(&mut self) {
        if let Some(ref_name) = self.ref_list_state.selected_ref_name() {
            self.list_state_mut().select_ref(&ref_name);
        }
    }

    fn copy_ref_name(&self) {
        if let Some(name) = self.ref_list_state.selected_branch() {
            self.tx.send(AppEvent::CopyToClipboard {
                name: "Branch Name".to_string(),
                value: name,
            });
        } else if let Some(name) = self.ref_list_state.selected_tag() {
            self.tx.send(AppEvent::CopyToClipboard {
                name: "Tag Name".to_string(),
                value: name,
            });
        }
    }
}
