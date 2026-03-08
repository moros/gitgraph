//! Application state machine, event loop, and rendering.

use crate::config::{Config, GraphWidth, InitialSelection, Protocol, DEFAULT_GRAPH_MAX_WIDTH};
use crate::event::{AppEvent, EventHandler, Sender};
use crate::git::{CommitHash, Head, Ref, Repository};
use crate::graph::image::{
    CellWidthType, GraphImageManager, GraphStyle as ImageGraphStyle, ImageColors,
};
use crate::graph::{render_text_row, Graph};
use crate::keybind::{UserEvent, UserEventWithCount};
use crate::protocol::ImageProtocol;
use crate::theme::ThemeColor;
use crate::view::View;
use crate::widget::commit_list::{CommitInfo, CommitListState};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Stylize},
    text::Line,
    widgets::{Block, Borders, Clear, Padding, Paragraph},
    Frame, Terminal,
};
use rustc_hash::FxHashMap;
use std::io::Stdout;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// RunResult
// ---------------------------------------------------------------------------

/// Outcome of `App::run`.
pub enum RunResult {
    /// The user quit the application.
    Quit,
    /// The user requested a data refresh; the inner value is the currently
    /// selected commit hash (used to restore context after reload).
    Refresh(Option<CommitHash>),
}

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
    /// When true, the next render emits a full-screen Clear before drawing the
    /// new view.  Set on every view transition so ratatui's diff-based renderer
    /// resets all cells and prevents text bleed-through.
    clear: bool,
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
    /// All refs collected from the repository, passed to RefsView on Tab.
    all_refs: Vec<Ref>,
    /// Image protocol in use (None when rendering text graph).
    /// Used to clear the graphics layer when transitioning between views.
    image_protocol: Option<ImageProtocol>,
    /// Track terminal focus state
    focused: bool,
}

impl App {
    pub fn new(
        repo: &Repository,
        graph: &Graph,
        config: Rc<Config>,
        tx: Sender,
        restore_hash: Option<&CommitHash>,
        use_text_graph: bool,
    ) -> Self {
        // Build CommitInfo for each commit in display order
        let branches = &config.graph.color.branches;
        let lane_count = (graph.max_pos_x + 1) as u16;
        let graph_cell_width = effective_graph_cell_width(&config, lane_count);

        let mut ref_name_to_commit_index_map: FxHashMap<String, usize> = FxHashMap::default();

        // Build image manager for image-capable terminals (preloads all rows up front).
        let image_manager: Option<GraphImageManager> = if !use_text_graph {
            let branch_colors: Vec<image::Rgba<u8>> = config
                .graph
                .color
                .branches
                .iter()
                .map(theme_color_to_rgba)
                .collect();
            let edge_color = theme_color_to_rgba(&config.graph.color.edge);
            let bg_color = theme_color_to_rgba(&config.graph.color.background);
            let image_colors = ImageColors::new(branch_colors, edge_color, bg_color);

            let cell_width_type = match config.core.graph_width {
                GraphWidth::Single => CellWidthType::Single,
                _ => CellWidthType::Double,
            };

            let image_graph_style = match config.core.graph_style {
                crate::config::GraphStyle::Rounded => ImageGraphStyle::Rounded,
                crate::config::GraphStyle::Angular => ImageGraphStyle::Angular,
            };

            let image_protocol = match config.core.protocol {
                Protocol::Auto => crate::protocol::auto_detect(),
                Protocol::Iterm => ImageProtocol::Iterm2,
                Protocol::Kitty => ImageProtocol::Kitty,
            };

            Some(GraphImageManager::new(
                graph,
                &image_colors,
                cell_width_type,
                image_graph_style,
                image_protocol, // ImageProtocol is Copy
                true,
                graph_cell_width,
            ))
        } else {
            None
        };

        // Capture the protocol for later use in clear_images().
        let image_protocol: Option<ImageProtocol> = if !use_text_graph {
            Some(match config.core.protocol {
                Protocol::Auto => crate::protocol::auto_detect(),
                Protocol::Iterm => ImageProtocol::Iterm2,
                Protocol::Kitty => ImageProtocol::Kitty,
            })
        } else {
            None
        };

        let commits: Vec<CommitInfo> = graph
            .commits
            .iter()
            .enumerate()
            .map(|(i, hash)| {
                // Commit
                let commit = repo.commit(hash).cloned().unwrap_or_default();
                // Refs
                let refs: Vec<crate::git::Ref> = repo.refs(hash).into_iter().cloned().collect();
                for r in &refs {
                    ref_name_to_commit_index_map.insert(r.name().to_string(), i);
                }
                // Graph color
                let color_index = graph.commit_pos_map.get(hash).map(|p| p.x).unwrap_or(0);
                let graph_color = if branches.is_empty() {
                    Color::White
                } else {
                    branches[color_index % branches.len()].0
                };

                let graph_line = if use_text_graph {
                    // Ensure the text graph does not exceed the allocated cell width.
                    let full = render_text_row(graph, hash);
                    if full.chars().count() > graph_cell_width as usize {
                        full.chars().take(graph_cell_width as usize).collect()
                    } else {
                        full
                    }
                } else {
                    image_manager
                        .as_ref()
                        .map(|m| m.encoded_image(hash).to_string())
                        .unwrap_or_default()
                };
                CommitInfo {
                    commit,
                    refs,
                    graph_color,
                    graph_line,
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

        // Apply initial selection: prefer restore_hash (from refresh), then config.
        if let Some(hash) = restore_hash {
            commit_list_state.select_commit_hash(hash);
        } else if let InitialSelection::Head = config.core.initial_selection {
            match repo.head() {
                Head::Branch { name } => commit_list_state.select_ref(name),
                Head::Detached { target } => commit_list_state.select_commit_hash(target),
                Head::None => {}
            }
        }

        let all_refs: Vec<Ref> = repo.all_refs().into_iter().cloned().collect();

        let view = View::of_list(commit_list_state, config.clone(), tx.clone());

        Self {
            config,
            view,
            app_status: AppStatus::default(),
            tx,
            saved_list_state: None,
            all_refs,
            image_protocol,
            focused: true,
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        events: &EventHandler,
    ) -> anyhow::Result<RunResult> {
        loop {
            if self.focused {
                terminal.draw(|f| self.render(f))?;
            }

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
                        Some(UserEvent::ForceQuit) => return Ok(RunResult::Quit),
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
                                        && (c != '0' || !self.app_status.numeric_prefix.is_empty())
                                    {
                                        self.app_status.numeric_prefix.push(c);
                                    }
                                }
                            }
                        }
                    }
                }

                AppEvent::Resize(_, _) => continue,
                AppEvent::Tick => {}

                AppEvent::FocusLost => {
                    self.focused = false;
                    // Optionally clear screen to avoid stale UI
                    self.app_status.clear = true;
                }
                AppEvent::FocusGained => {
                    self.focused = true;
                }

                AppEvent::Quit => return Ok(RunResult::Quit),

                AppEvent::Refresh => {
                    let hash = if let Some(ls) = self.view.take_list_state() {
                        Some(ls.selected_commit_hash().clone())
                    } else {
                        self.saved_list_state
                            .as_ref()
                            .map(|ls| ls.selected_commit_hash().clone())
                    };
                    return Ok(RunResult::Refresh(hash));
                }

                AppEvent::OpenDetail => {
                    if let Some(list_state) = self.view.take_list_state() {
                        let idx = list_state.selected + list_state.offset;
                        if idx < list_state.commits.len() {
                            let commit_info = list_state.commits[idx].clone();
                            self.saved_list_state = Some(list_state);
                            self.transition();
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
                        self.transition();
                        self.view = View::of_list(list_state, self.config.clone(), self.tx.clone());
                    }
                }
                AppEvent::DetailNextCommit => {
                    self.navigate_detail(1);
                }
                AppEvent::DetailPrevCommit => {
                    self.navigate_detail_prev();
                }
                AppEvent::OpenRefs => {
                    if let Some(list_state) = self.view.take_list_state() {
                        self.transition();
                        self.view = View::of_refs(
                            list_state,
                            self.all_refs.clone(),
                            self.config.clone(),
                            self.tx.clone(),
                        );
                    }
                }
                AppEvent::CloseRefs => {
                    if let Some(list_state) = self.view.take_list_state() {
                        self.transition();
                        self.view = View::of_list(list_state, self.config.clone(), self.tx.clone());
                    }
                }
                AppEvent::OpenHelp => {
                    self.transition();
                    let before = std::mem::take(&mut self.view);
                    self.view = View::of_help(before, self.config.clone(), self.tx.clone());
                }
                AppEvent::CloseHelp => {
                    self.transition();
                    let current = std::mem::take(&mut self.view);
                    if let Some(before) = current.take_help_before() {
                        self.view = before;
                    }
                }
                AppEvent::OpenUserCommand(slot) => {
                    // Obtain the list state — prefer current List view, fall back to saved state.
                    let list_state = if let Some(state) = self.view.take_list_state() {
                        state
                    } else if let Some(state) = self.saved_list_state.take() {
                        state
                    } else {
                        continue;
                    };
                    self.transition();
                    self.view = View::of_user_command(
                        list_state,
                        slot,
                        self.config.clone(),
                        self.tx.clone(),
                    );
                }
                AppEvent::CloseUserCommand => {
                    if let Some(list_state) = self.view.take_list_state() {
                        self.transition();
                        self.view = View::of_list(list_state, self.config.clone(), self.tx.clone());
                    }
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
    }

    fn render(&mut self, f: &mut Frame) {
        // Clear frame: on view transitions emit a full-screen Clear so ratatui's
        // diff-based renderer resets all cells.  Return early — the next frame
        // will draw the new view on a clean canvas.
        if self.app_status.clear {
            f.render_widget(Clear, f.area());
            self.clear_images();
            self.app_status.clear = false;
            return;
        }

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
                    Line::raw(self.app_status.numeric_prefix.as_str()).fg(theme.status_fg.0)
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

    /// Prepare for a view transition: schedule a clear frame and clear images.
    ///
    /// Sets `app_status.clear` so the next render emits a full-screen
    /// [`Clear`] widget before drawing the incoming view, preventing ratatui
    /// text bleed-through.  Also clears any terminal inline images immediately
    /// so they do not persist into the new view.
    fn transition(&mut self) {
        self.app_status.clear = true;
        self.clear_images();
    }

    /// Clear all terminal inline images from the graphics layer.
    fn clear_images(&self) {
        if let Some(protocol) = &self.image_protocol {
            protocol.clear_all();
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

/// Convert a [`ThemeColor`] to an RGBA pixel value for image rendering.
///
/// - `Color::Rgb(r,g,b)` → opaque pixel
/// - `Color::Reset` (transparent) → alpha=0
/// - Named/indexed colors → approximate standard terminal RGB values
fn theme_color_to_rgba(tc: &ThemeColor) -> image::Rgba<u8> {
    match tc.0 {
        Color::Rgb(r, g, b) => image::Rgba([r, g, b, 255]),
        Color::Reset => image::Rgba([0, 0, 0, 0]),
        Color::Black => image::Rgba([0, 0, 0, 255]),
        Color::Red => image::Rgba([170, 0, 0, 255]),
        Color::Green => image::Rgba([0, 170, 0, 255]),
        Color::Yellow => image::Rgba([170, 85, 0, 255]),
        Color::Blue => image::Rgba([0, 0, 170, 255]),
        Color::Magenta => image::Rgba([170, 0, 170, 255]),
        Color::Cyan => image::Rgba([0, 170, 170, 255]),
        Color::Gray => image::Rgba([170, 170, 170, 255]),
        Color::DarkGray => image::Rgba([85, 85, 85, 255]),
        Color::LightRed => image::Rgba([255, 85, 85, 255]),
        Color::LightGreen => image::Rgba([85, 255, 85, 255]),
        Color::LightYellow => image::Rgba([255, 255, 85, 255]),
        Color::LightBlue => image::Rgba([85, 85, 255, 255]),
        Color::LightMagenta => image::Rgba([255, 85, 255, 255]),
        Color::LightCyan => image::Rgba([85, 255, 255, 255]),
        Color::White => image::Rgba([255, 255, 255, 255]),
        Color::Indexed(_) => image::Rgba([255, 255, 255, 255]),
    }
}

fn effective_graph_cell_width(config: &Config, lane_count: u16) -> u16 {
    let graph_cell_width = match config.core.graph_width {
        GraphWidth::Single => lane_count,
        _ => lane_count * 2, // Auto or Double
    };
    let max_width = if config.core.graph_max_width > 0 {
        config.core.graph_max_width
    } else {
        DEFAULT_GRAPH_MAX_WIDTH
    };
    graph_cell_width.min(max_width)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        color::ColorTheme,
        config::{CoreConfig, GraphConfig, UiConfig},
        keybind::KeyBind,
    };

    fn test_config() -> Config {
        Config {
            core: CoreConfig::default(),
            ui: UiConfig::default(),
            graph: GraphConfig::default(),
            color: ColorTheme::default(),
            keybind: KeyBind::new(None).expect("default keybinds should parse"),
        }
    }

    #[test]
    fn effective_graph_cell_width_uses_default_cap_when_graph_max_width_is_zero() {
        let mut config = test_config();
        config.core.graph_width = GraphWidth::Double;
        config.core.graph_max_width = 0;

        assert_eq!(
            effective_graph_cell_width(&config, 20),
            DEFAULT_GRAPH_MAX_WIDTH
        );
    }

    #[test]
    fn effective_graph_cell_width_respects_configured_max_width() {
        let mut config = test_config();
        config.core.graph_width = GraphWidth::Single;
        config.core.graph_max_width = 8;

        assert_eq!(effective_graph_cell_width(&config, 12), 8);
    }
}
