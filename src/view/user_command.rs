//! UserCommandView — split layout showing the commit list on top and inline
//! user-command output on the bottom.
//!
//! `d` (or `Nd` for slot N) executes the configured user command for the
//! selected commit and renders the ANSI-coloured stdout in the lower panel.
//! Navigating the commit list re-executes the command for the newly selected
//! commit.

use std::rc::Rc;

use ansi_to_tui::IntoText as _;
use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Padding, Paragraph},
    Frame,
};

use crate::{
    config::{Config, UserCommandKind},
    event::{AppEvent, Sender},
    external::{exec_user_command, ExternalCommandParameters},
    git::{Commit, Ref},
    keybind::{UserEvent, UserEventWithCount},
    widget::commit_list::{CommitList, CommitListState},
};

// ---------------------------------------------------------------------------
// UserCommandView
// ---------------------------------------------------------------------------

pub struct UserCommandView {
    commit_list_state: CommitListState,
    command_output: Vec<Line<'static>>,
    scroll_offset: usize,
    scroll_height: usize,
    user_command_slot: u8,
    config: Rc<Config>,
    tx: Sender,
}

impl UserCommandView {
    pub fn new(
        commit_list_state: CommitListState,
        user_command_slot: u8,
        config: Rc<Config>,
        tx: Sender,
    ) -> Self {
        // Execute the command for the initial selection
        let command_output = Self::run_command(&commit_list_state, user_command_slot, &config, &tx);
        Self {
            commit_list_state,
            command_output,
            scroll_offset: 0,
            scroll_height: 0,
            user_command_slot,
            config,
            tx,
        }
    }

    /// Take the stored `CommitListState` back (called when closing this view).
    pub fn take_list_state(self) -> CommitListState {
        self.commit_list_state
    }

    pub fn handle_event(&mut self, ewc: UserEventWithCount, _key: KeyEvent) {
        let event = ewc.event;
        let count = ewc.effective_count();
        match event {
            UserEvent::Quit | UserEvent::ForceQuit => {
                self.tx.send(AppEvent::Quit);
            }
            UserEvent::Cancel => {
                self.tx.send(AppEvent::CloseUserCommand);
            }
            UserEvent::HelpToggle => {
                self.tx.send(AppEvent::OpenHelp);
            }
            UserEvent::NavigateDown => {
                for _ in 0..count {
                    self.scroll_output_down();
                }
            }
            UserEvent::NavigateUp => {
                for _ in 0..count {
                    self.scroll_output_up();
                }
            }
            UserEvent::PageDown => {
                for _ in 0..count {
                    self.scroll_output_page_down();
                }
            }
            UserEvent::PageUp => {
                for _ in 0..count {
                    self.scroll_output_page_up();
                }
            }
            UserEvent::HalfPageDown => {
                for _ in 0..count {
                    self.scroll_output_half_page_down();
                }
            }
            UserEvent::HalfPageUp => {
                for _ in 0..count {
                    self.scroll_output_half_page_up();
                }
            }
            UserEvent::GoToTop => {
                self.scroll_offset = 0;
            }
            UserEvent::GoToBottom => {
                self.scroll_offset = usize::MAX;
            }
            UserEvent::SelectDown => {
                self.commit_list_state.select_next();
                self.refresh_output();
            }
            UserEvent::SelectUp => {
                self.commit_list_state.select_prev();
                self.refresh_output();
            }
            UserEvent::JumpToParent => {
                for _ in 0..count {
                    self.commit_list_state.select_parent();
                }
                self.refresh_output();
            }
            UserEvent::Confirm => {
                self.tx.send(AppEvent::OpenDetail);
            }
            UserEvent::UserCommand => {
                let slot = ewc.count.unwrap_or(1) as u8;
                if slot == self.user_command_slot {
                    // Same slot — toggle close
                    self.tx.send(AppEvent::CloseUserCommand);
                } else {
                    self.tx.send(AppEvent::OpenUserCommand(slot));
                }
            }
            _ => {}
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let output_height = (area.height / 2).max(5).min(area.height.saturating_sub(3));
        let [list_area, output_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(output_height)])
                .areas(area);

        // Commit list (top)
        let list_widget = CommitList::new(self.config.clone());
        f.render_stateful_widget(list_widget, list_area, &mut self.commit_list_state);

        // Output panel (bottom)
        self.scroll_height = output_area.height.saturating_sub(2) as usize; // subtract border
        let max_offset = self.command_output.len().saturating_sub(1);
        self.scroll_offset = self.scroll_offset.min(max_offset);

        let visible: Vec<Line> = self
            .command_output
            .iter()
            .skip(self.scroll_offset)
            .take(self.scroll_height)
            .cloned()
            .collect();

        let slot_name = self
            .config
            .core
            .user_command
            .commands()
            .into_iter()
            .find(|(s, _)| *s == self.user_command_slot)
            .map(|(_, def)| def.name.clone())
            .unwrap_or_else(|| format!("Command {}", self.user_command_slot));

        let theme = &self.config.color;
        let block = Block::default()
            .title(format!(" {} ", slot_name))
            .borders(Borders::TOP)
            .border_style(ratatui::style::Style::default().fg(theme.border.0))
            .padding(Padding::horizontal(1));

        let para = Paragraph::new(visible).block(block);
        f.render_widget(para, output_area);
    }

    /// Returns the selected `CommitListState` reference (for reading selected commit).
    pub fn list_state(&self) -> &CommitListState {
        &self.commit_list_state
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl UserCommandView {
    fn scroll_output_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }
    fn scroll_output_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }
    fn scroll_output_page_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(self.scroll_height);
    }
    fn scroll_output_page_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_height);
    }
    fn scroll_output_half_page_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(self.scroll_height / 2);
    }
    fn scroll_output_half_page_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(self.scroll_height / 2);
    }

    fn refresh_output(&mut self) {
        let config = self.config.clone();
        let tx = self.tx.clone();
        self.command_output =
            Self::run_command(&self.commit_list_state, self.user_command_slot, &config, &tx);
        self.scroll_offset = 0;
    }

    fn run_command(
        state: &CommitListState,
        slot: u8,
        config: &Config,
        tx: &Sender,
    ) -> Vec<Line<'static>> {
        let idx = state.selected + state.offset;
        if idx >= state.commits.len() {
            return vec![];
        }
        let commit_info = &state.commits[idx];
        let commit = &commit_info.commit;
        let refs = &commit_info.refs;

        let cmd_def = config.core.user_command.commands().into_iter().find(|(s, _)| *s == slot);
        let Some((_, def)) = cmd_def else {
            tx.send(AppEvent::NotifyError(format!("No user command configured for slot {slot}")));
            return vec![];
        };

        if matches!(def.kind, UserCommandKind::Silent) {
            // Silent commands: run in background, don't capture output
            let _ = execute_silent_command(def.commands.clone(), commit, refs);
            return vec![];
        }

        let params = build_params(def.commands.clone(), commit, refs, 0, 0);
        match exec_user_command(params) {
            Ok(output) => {
                let tab_spaces = " ".repeat(config.core.user_command.tab_width);
                let processed = output.replace('\t', &tab_spaces);
                match processed.into_text() {
                    Ok(text) => text.into_iter().collect(),
                    Err(_) => vec![Line::raw("(failed to parse command output)")],
                }
            }
            Err(e) => {
                tx.send(AppEvent::NotifyError(e));
                vec![]
            }
        }
    }
}

fn build_params(
    command: Vec<String>,
    commit: &Commit,
    refs: &[Ref],
    area_width: u16,
    area_height: u16,
) -> ExternalCommandParameters {
    let target_hash = commit.hash.as_str().to_string();
    let parent_hashes: Vec<String> = commit
        .parent_hashes
        .iter()
        .map(|h| h.as_str().to_string())
        .collect();
    ExternalCommandParameters::from_commit(
        command,
        target_hash,
        parent_hashes,
        refs,
        area_width,
        area_height,
    )
}

fn execute_silent_command(command: Vec<String>, commit: &Commit, refs: &[Ref]) {
    let params = build_params(command, commit, refs, 0, 0);
    if params.command.is_empty() {
        return;
    }
    // Fire-and-forget: ignore result
    let _ = std::process::Command::new(&params.command[0])
        .args(&params.command[1..])
        .spawn();
}
