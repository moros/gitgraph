//! Application state machine, event loop, and rendering.

use crate::config::{Config, GraphWidth, InitialSelection};
use crate::event::{AppEvent, EventHandler, Sender};
use crate::git::{Head, Repository};
use crate::graph::Graph;
use crate::keybind::{UserEvent, UserEventWithCount};
use crate::view::View;
use crate::widget::commit_list::{CommitInfo, CommitListState};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Stylize},
    text::Line,
    widgets::{Block, Borders, Padding, Paragraph},
    Frame, Terminal,
};
use rustc_hash::FxHashMap;
use std::io::Stdout;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// StatusLine
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
enum StatusLine {
    #[default]
    None,
    Input(String, Option<u16>, Option<String>),
    NotificationInfo(String),
    NotificationSuccess(String),
    NotificationWarn(String),
    NotificationError(String),
}

// ---------------------------------------------------------------------------
// AppStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct AppStatus {
    status_line: StatusLine,
    numeric_prefix: String,
    view_area: Rect,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct App {
    config: Rc<Config>,
    view: View,
    app_status: AppStatus,
    tx: Sender,
    /// Saved list state when the detail view is open, restored on CloseDetail.
    saved_list_state: Option<CommitListState>,
}

impl App {
    pub fn new(
        repo: &Repository,
        graph: &Graph,
        config: Rc<Config>,
        tx: Sender,
    ) -> Self {
        // Build CommitInfo for each commit in display order
        let branches = &config.graph.color.branches;
        let lane_count = (graph.max_pos_x + 1) as u16;
        let graph_cell_width = match config.core.graph_width {
            GraphWidth::Single => lane_count,
            _ => lane_count * 2, // Auto or Double
        };

        let mut ref_name_to_commit_index_map: FxHashMap<String, usize> = FxHashMap::default();

        let commits: Vec<CommitInfo> = graph
            .commits
            .iter()
            .enumerate()
            .map(|(i, hash)| {
                // Commit
                let commit = repo.commit(hash).cloned().unwrap_or_default();
                // Refs
                let refs: Vec<crate::git::Ref> =
                    repo.refs(hash).into_iter().cloned().collect();
                for r in &refs {
                    ref_name_to_commit_index_map.insert(r.name().to_string(), i);
                }
                // Graph color
                let color_index = graph
                    .commit_pos_map
                    .get(hash)
                    .map(|p| p.x)
                    .unwrap_or(0);
                let graph_color = if branches.is_empty() {
                    Color::White
                } else {
                    branches[color_index % branches.len()].0
                };

                CommitInfo {
                    commit,
                    refs,
                    graph_color,
                    graph_line: String::new(), // populated by image renderer in Phase 3C
                }
            })
            .collect();

        let head = repo.head().clone();
        let mut commit_list_state = crate::widget::commit_list::CommitListState::new(
            commits,
            graph_cell_width,
            head,
            ref_name_to_commit_index_map,
            config.core.search.ignore_case,
            config.core.search.fuzzy,
        );

        // Apply initial selection
        if let InitialSelection::Head = config.core.initial_selection {
            match repo.head() {
                Head::Branch { name } => commit_list_state.select_ref(name),
                Head::Detached { target } => commit_list_state.select_commit_hash(target),
                Head::None => {}
            }
        }

        let view = View::of_list(commit_list_state, config.clone(), tx.clone());

        Self {
            config,
            view,
            app_status: AppStatus::default(),
            tx,
            saved_list_state: None,
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        events: &EventHandler,
    ) -> anyhow::Result<()> {
        loop {
            terminal.draw(|f| self.render(f))?;

            match events.next()? {
                AppEvent::Key(key) => {
                    // If a notification is showing, clear it on key press
                    match self.app_status.status_line {
                        StatusLine::None | StatusLine::Input(_, _, _) => {}
                        StatusLine::NotificationInfo(_)
                        | StatusLine::NotificationSuccess(_)
                        | StatusLine::NotificationWarn(_) => {
                            self.app_status.status_line = StatusLine::None;
                        }
                        StatusLine::NotificationError(_) => {
                            self.app_status.status_line = StatusLine::None;
                            continue;
                        }
                    }

                    let user_event = self.config.keybind.get(&key).copied();

                    // Clear numeric prefix on Cancel
                    if let Some(UserEvent::Cancel) = user_event {
                        if !self.app_status.numeric_prefix.is_empty() {
                            self.app_status.numeric_prefix.clear();
                            continue;
                        }
                    }

                    match user_event {
                        Some(UserEvent::ForceQuit) => break,
                        Some(ue) => {
                            let ewc = process_numeric_prefix(&self.app_status.numeric_prefix, ue);
                            self.view.handle_event(ewc, key);
                            self.app_status.numeric_prefix.clear();
                        }
                        None => {
                            if matches!(self.app_status.status_line, StatusLine::Input(_, _, _))
                                || self.view.is_in_input_mode()
                            {
                                // In input mode: forward raw key to view for search input
                                self.app_status.numeric_prefix.clear();
                                self.view.raw_key_input(key);
                            } else {
                                // Accumulate numeric prefix
                                if let crossterm::event::KeyCode::Char(c) = key.code {
                                    if c.is_ascii_digit()
                                        && (c != '0'
                                            || !self.app_status.numeric_prefix.is_empty())
                                    {
                                        self.app_status.numeric_prefix.push(c);
                                    }
                                }
                            }
                        }
                    }
                }

                AppEvent::Resize(_, _) => {}
                AppEvent::Tick => {}

                AppEvent::Quit => break,

                AppEvent::OpenDetail => {
                    if let Some(list_state) = self.view.take_list_state() {
                        let idx = list_state.selected + list_state.offset;
                        if idx < list_state.commits.len() {
                            let commit_info = list_state.commits[idx].clone();
                            self.saved_list_state = Some(list_state);
                            self.view = View::of_detail(
                                commit_info,
                                idx,
                                self.config.clone(),
                                self.tx.clone(),
                            );
                        } else {
                            // Restore list state if index is out of bounds
                            self.view.give_list_state(list_state);
                        }
                    }
                }
                AppEvent::CloseDetail => {
                    if let Some(list_state) = self.saved_list_state.take() {
                        self.view =
                            View::of_list(list_state, self.config.clone(), self.tx.clone());
                    }
                }
                AppEvent::DetailNextCommit => {
                    self.navigate_detail(1);
                }
                AppEvent::DetailPrevCommit => {
                    self.navigate_detail_prev();
                }
                AppEvent::OpenRefs => {
                    self.app_status.status_line =
                        StatusLine::NotificationInfo("Refs view not yet implemented".to_string());
                }
                AppEvent::OpenHelp => {
                    self.app_status.status_line =
                        StatusLine::NotificationInfo("Help view not yet implemented".to_string());
                }

                AppEvent::ClearStatusLine => {
                    self.app_status.status_line = StatusLine::None;
                }
                AppEvent::UpdateStatusInput(msg, cursor, transient) => {
                    self.app_status.status_line = StatusLine::Input(msg, cursor, transient);
                }
                AppEvent::NotifyInfo(msg) => {
                    self.app_status.status_line = StatusLine::NotificationInfo(msg);
                }
                AppEvent::NotifyWarn(msg) => {
                    self.app_status.status_line = StatusLine::NotificationWarn(msg);
                }
                AppEvent::NotifyError(msg) => {
                    self.app_status.status_line = StatusLine::NotificationError(msg);
                }
                AppEvent::NotifySuccess(msg) => {
                    self.app_status.status_line = StatusLine::NotificationSuccess(msg);
                }

                AppEvent::CopyToClipboard { name, value } => {
                    self.copy_to_clipboard(name, value);
                }
            }
        }

        Ok(())
    }

    fn render(&mut self, f: &mut Frame) {
        // Background fill
        let base = Block::default().bg(Color::Reset);
        f.render_widget(base, f.area());

        let [view_area, status_line_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(f.area());

        self.app_status.view_area = view_area;
        self.view.render(f, view_area);
        self.render_status_line(f, status_line_area);
    }

    fn render_status_line(&self, f: &mut Frame, area: Rect) {
        let theme = &self.config.color;

        let text: Line = match &self.app_status.status_line {
            StatusLine::None => {
                if self.app_status.numeric_prefix.is_empty() {
                    Line::raw("")
                } else {
                    Line::raw(self.app_status.numeric_prefix.as_str())
                        .fg(theme.status_fg.0)
                }
            }
            StatusLine::Input(msg, _, transient) => {
                let msg_w = console::measure_text_width(msg.as_str());
                if let Some(t_msg) = transient {
                    let t_w = console::measure_text_width(t_msg.as_str());
                    let pad_w = (area.width as usize)
                        .saturating_sub(msg_w)
                        .saturating_sub(t_w)
                        .saturating_sub(2);
                    Line::from(vec![
                        msg.as_str().fg(theme.status_search_fg.0),
                        " ".repeat(pad_w).into(),
                        t_msg.as_str().fg(theme.status_fg.0),
                    ])
                } else {
                    Line::raw(msg.as_str()).fg(theme.status_search_fg.0)
                }
            }
            StatusLine::NotificationInfo(msg) => Line::raw(msg.as_str()).fg(theme.status_fg.0),
            StatusLine::NotificationSuccess(msg) => {
                Line::raw(msg.as_str()).fg(theme.status_search_fg.0)
            }
            StatusLine::NotificationWarn(msg) => {
                Line::raw(msg.as_str()).fg(theme.status_search_fg.0)
            }
            StatusLine::NotificationError(msg) => Line::raw(format!("ERROR: {msg}"))
                .fg(theme.status_error_fg.0)
                .bg(theme.status_error_bg.0),
        };

        let paragraph = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(ratatui::style::Style::default().fg(theme.border.0))
                .padding(Padding::horizontal(1)),
        );
        f.render_widget(paragraph, area);

        // Cursor for search input
        if let StatusLine::Input(_, Some(cursor_pos), _) = &self.app_status.status_line {
            let x = area.x + cursor_pos + 1; // +1 for left padding
            let y = area.y + 1; // +1 for top border
            f.set_cursor_position((x, y));
        }
    }

    /// Navigate to the next commit (higher index) in detail view.
    fn navigate_detail(&mut self, delta: usize) {
        if let Some(current_idx) = self.view.detail_commit_index() {
            if let Some(list_state) = &self.saved_list_state {
                let new_idx = (current_idx + delta).min(list_state.commits.len().saturating_sub(1));
                if new_idx != current_idx {
                    let commit_info = list_state.commits[new_idx].clone();
                    let config = self.config.clone();
                    let tx = self.tx.clone();
                    self.view = View::of_detail(commit_info, new_idx, config, tx);
                }
            }
        }
    }

    /// Navigate to the previous commit (lower index) in detail view.
    fn navigate_detail_prev(&mut self) {
        if let Some(current_idx) = self.view.detail_commit_index() {
            if current_idx > 0 {
                let new_idx = current_idx - 1;
                if let Some(list_state) = &self.saved_list_state {
                    let commit_info = list_state.commits[new_idx].clone();
                    let config = self.config.clone();
                    let tx = self.tx.clone();
                    self.view = View::of_detail(commit_info, new_idx, config, tx);
                }
            }
        }
    }

    fn copy_to_clipboard(&mut self, name: String, value: String) {
        match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(&value)) {
            Ok(_) => {
                self.app_status.status_line =
                    StatusLine::NotificationSuccess(format!("Copied {name} to clipboard"));
            }
            Err(e) => {
                self.app_status.status_line =
                    StatusLine::NotificationError(format!("Clipboard error: {e}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn process_numeric_prefix(numeric_prefix: &str, user_event: UserEvent) -> UserEventWithCount {
    if user_event.is_countable() {
        let count = if numeric_prefix.is_empty() {
            None
        } else {
            numeric_prefix.parse::<usize>().ok().map(|n| n.max(1))
        };
        UserEventWithCount::new(user_event, count)
    } else {
        UserEventWithCount::new(user_event, None)
    }
}
