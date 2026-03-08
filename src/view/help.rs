//! HelpView — full-screen scrollable keybinding overlay.
//!
//! Shows all keybindings grouped by context section. Pressing `?`, `q`, or
//! `Esc` closes the overlay and restores the previous view.

use std::rc::Rc;

use crossterm::event::KeyEvent;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Stylize},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph},
    Frame,
};

use crate::{
    config::Config,
    event::{AppEvent, Sender},
    keybind::{UserEvent, UserEventWithCount},
    view::View,
};

// ---------------------------------------------------------------------------
// HelpView
// ---------------------------------------------------------------------------

pub struct HelpView {
    /// The view that was active before help was opened (restored on close).
    pub before: View,

    help_key_lines: Vec<Line<'static>>,
    help_value_lines: Vec<Line<'static>>,
    help_key_line_max_width: u16,

    offset: usize,
    height: usize,

    tx: Sender,
}

impl HelpView {
    pub fn new(before: View, config: Rc<Config>, tx: Sender) -> Self {
        let (help_key_lines, help_value_lines) = build_lines(&config);
        let help_key_line_max_width = help_key_lines
            .iter()
            .map(|l| l.width())
            .max()
            .unwrap_or(0) as u16;
        let _ = config; // used for build_lines only
        Self {
            before,
            help_key_lines,
            help_value_lines,
            help_key_line_max_width,
            offset: 0,
            height: 0,
            tx,
        }
    }

    pub fn handle_event(&mut self, ewc: UserEventWithCount, _key: KeyEvent) {
        let event = ewc.event;
        let count = ewc.effective_count();
        match event {
            UserEvent::Quit | UserEvent::ForceQuit => {
                self.tx.send(AppEvent::Quit);
            }
            UserEvent::HelpToggle | UserEvent::Cancel => {
                self.tx.send(AppEvent::CloseHelp);
            }
            UserEvent::NavigateDown | UserEvent::SelectDown => {
                for _ in 0..count {
                    self.scroll_down();
                }
            }
            UserEvent::NavigateUp | UserEvent::SelectUp => {
                for _ in 0..count {
                    self.scroll_up();
                }
            }
            UserEvent::PageDown => {
                for _ in 0..count {
                    self.scroll_page_down();
                }
            }
            UserEvent::PageUp => {
                for _ in 0..count {
                    self.scroll_page_up();
                }
            }
            UserEvent::HalfPageDown => {
                for _ in 0..count {
                    self.scroll_half_page_down();
                }
            }
            UserEvent::HalfPageUp => {
                for _ in 0..count {
                    self.scroll_half_page_up();
                }
            }
            UserEvent::GoToTop => self.offset = 0,
            UserEvent::GoToBottom => self.offset = usize::MAX,
            _ => {}
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        self.update_state(area);

        let [key_area, value_area] = Layout::horizontal([
            Constraint::Length(self.help_key_line_max_width.max(20) + 4),
            Constraint::Min(0),
        ])
        .areas(area);

        let key_lines: Vec<Line> = self
            .help_key_lines
            .iter()
            .skip(self.offset)
            .take(area.height as usize)
            .cloned()
            .collect();
        let value_lines: Vec<Line> = self
            .help_value_lines
            .iter()
            .skip(self.offset)
            .take(area.height as usize)
            .cloned()
            .collect();

        let key_para = Paragraph::new(key_lines)
            .block(Block::default().padding(Padding::new(2, 1, 0, 0)))
            .right_aligned();
        let value_para = Paragraph::new(value_lines)
            .block(Block::default().padding(Padding::new(1, 2, 0, 0)))
            .left_aligned();

        f.render_widget(key_para, key_area);
        f.render_widget(value_para, value_area);
    }
}

impl HelpView {
    fn scroll_down(&mut self) {
        self.offset = self.offset.saturating_add(1);
    }
    fn scroll_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }
    fn scroll_page_down(&mut self) {
        self.offset = self.offset.saturating_add(self.height);
    }
    fn scroll_page_up(&mut self) {
        self.offset = self.offset.saturating_sub(self.height);
    }
    fn scroll_half_page_down(&mut self) {
        self.offset = self.offset.saturating_add(self.height / 2);
    }
    fn scroll_half_page_up(&mut self) {
        self.offset = self.offset.saturating_sub(self.height / 2);
    }

    fn update_state(&mut self, area: Rect) {
        self.height = area.height as usize;
        let max_offset = self.help_key_lines.len().saturating_sub(1);
        self.offset = self.offset.min(max_offset);
    }
}

// ---------------------------------------------------------------------------
// Help line builders
// ---------------------------------------------------------------------------

#[rustfmt::skip]
fn build_lines(config: &Config) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let keybind = &config.keybind;
    let color = &config.color;

    let user_cmd_items: Vec<(Vec<UserEvent>, String)> = config
        .core
        .user_command
        .commands()
        .into_iter()
        .map(|(slot, def)| {
            (
                vec![UserEvent::UserCommand],
                format!("User command {} – {slot}: {}", slot, def.name),
            )
        })
        .collect();

    // ── Common ──────────────────────────────────────────────────────────────
    let common = vec![
        (vec![UserEvent::Quit, UserEvent::ForceQuit], "Quit".to_string()),
        (vec![UserEvent::HelpToggle], "Toggle help".to_string()),
    ];
    let (ck, cv) = build_section("Common", common, config);

    // ── Help ────────────────────────────────────────────────────────────────
    let help_section = vec![
        (vec![UserEvent::HelpToggle, UserEvent::Cancel], "Close help".to_string()),
        (vec![UserEvent::NavigateDown], "Scroll down".to_string()),
        (vec![UserEvent::NavigateUp], "Scroll up".to_string()),
        (vec![UserEvent::PageDown], "Page down".to_string()),
        (vec![UserEvent::PageUp], "Page up".to_string()),
        (vec![UserEvent::GoToTop], "Go to top".to_string()),
        (vec![UserEvent::GoToBottom], "Go to bottom".to_string()),
    ];
    let (hk, hv) = build_section("Help", help_section, config);

    // ── List View ───────────────────────────────────────────────────────────
    let mut list_items = vec![
        (vec![UserEvent::NavigateDown, UserEvent::SelectDown], "Move down".to_string()),
        (vec![UserEvent::NavigateUp, UserEvent::SelectUp], "Move up".to_string()),
        (vec![UserEvent::GoToTop], "Go to top".to_string()),
        (vec![UserEvent::GoToBottom], "Go to bottom".to_string()),
        (vec![UserEvent::PageDown], "Page down".to_string()),
        (vec![UserEvent::PageUp], "Page up".to_string()),
        (vec![UserEvent::HalfPageDown], "Half page down".to_string()),
        (vec![UserEvent::HalfPageUp], "Half page up".to_string()),
        (vec![UserEvent::ScrollDown], "Scroll viewport down".to_string()),
        (vec![UserEvent::ScrollUp], "Scroll viewport up".to_string()),
        (vec![UserEvent::SelectTop], "Select top row".to_string()),
        (vec![UserEvent::SelectMiddle], "Select middle row".to_string()),
        (vec![UserEvent::SelectBottom], "Select bottom row".to_string()),
        (vec![UserEvent::JumpToParent], "Jump to parent commit".to_string()),
        (vec![UserEvent::Confirm], "Open commit detail".to_string()),
        (vec![UserEvent::SearchStart], "Start search".to_string()),
        (vec![UserEvent::Cancel], "Cancel search".to_string()),
        (vec![UserEvent::SearchNext], "Next search match".to_string()),
        (vec![UserEvent::SearchPrev], "Previous search match".to_string()),
        (vec![UserEvent::SearchToggleCase], "Toggle case sensitivity".to_string()),
        (vec![UserEvent::SearchToggleFuzzy], "Toggle fuzzy search".to_string()),
        (vec![UserEvent::Refresh], "Refresh".to_string()),
        (vec![UserEvent::CopyShortHash], "Copy short hash".to_string()),
        (vec![UserEvent::CopyFullHash], "Copy full hash".to_string()),
    ];
    list_items.extend(user_cmd_items.clone());
    let (lk, lv) = build_section("Commit List", list_items, config);

    // ── Detail View ─────────────────────────────────────────────────────────
    let detail_items = vec![
        (vec![UserEvent::Cancel, UserEvent::Confirm], "Close detail".to_string()),
        (vec![UserEvent::NavigateDown], "Scroll down".to_string()),
        (vec![UserEvent::NavigateUp], "Scroll up".to_string()),
        (vec![UserEvent::PageDown], "Page down".to_string()),
        (vec![UserEvent::PageUp], "Page up".to_string()),
        (vec![UserEvent::HalfPageDown], "Half page down".to_string()),
        (vec![UserEvent::HalfPageUp], "Half page up".to_string()),
        (vec![UserEvent::GoToTop], "Go to top".to_string()),
        (vec![UserEvent::GoToBottom], "Go to bottom".to_string()),
        (vec![UserEvent::SelectDown], "Next commit".to_string()),
        (vec![UserEvent::SelectUp], "Previous commit".to_string()),
        (vec![UserEvent::ToggleDirectory], "Toggle / select file".to_string()),
        (vec![UserEvent::CollapseDirectory], "Collapse directory".to_string()),
        (vec![UserEvent::ScrollDiffDown], "Scroll diff down".to_string()),
        (vec![UserEvent::ScrollDiffUp], "Scroll diff up".to_string()),
        (vec![UserEvent::ScrollDiffLeft], "Scroll diff left".to_string()),
        (vec![UserEvent::ScrollDiffRight], "Scroll diff right".to_string()),
        (vec![UserEvent::CopyShortHash], "Copy short hash".to_string()),
        (vec![UserEvent::CopyFullHash], "Copy full hash".to_string()),
    ];
    let (dk, dv) = build_section("Commit Detail", detail_items, config);

    // ── User Command ────────────────────────────────────────────────────────
    let user_cmd_section = vec![
        (vec![UserEvent::Cancel], "Close user command".to_string()),
        (vec![UserEvent::NavigateDown], "Scroll output down".to_string()),
        (vec![UserEvent::NavigateUp], "Scroll output up".to_string()),
        (vec![UserEvent::PageDown], "Page down".to_string()),
        (vec![UserEvent::PageUp], "Page up".to_string()),
        (vec![UserEvent::SelectDown], "Next commit".to_string()),
        (vec![UserEvent::SelectUp], "Previous commit".to_string()),
        (vec![UserEvent::GoToTop], "Go to top".to_string()),
        (vec![UserEvent::GoToBottom], "Go to bottom".to_string()),
    ];
    let (uk, uv) = build_section("User Command", user_cmd_section, config);

    let key_lines = join_groups(vec![ck, hk, lk, dk, uk]);
    let value_lines = join_groups(vec![cv, hv, lv, dv, uv]);

    let _ = (keybind, color); // suppress unused warnings
    (key_lines, value_lines)
}

fn build_section(
    title: &'static str,
    items: Vec<(Vec<UserEvent>, String)>,
    config: &Config,
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let color = &config.color;
    let keybind = &config.keybind;

    let mut key_lines: Vec<Line<'static>> = Vec::new();
    let mut value_lines: Vec<Line<'static>> = Vec::new();

    // Section header
    key_lines.push(
        Line::raw(title)
            .fg(color.help_section.0)
            .add_modifier(Modifier::BOLD),
    );
    value_lines.push(Line::raw(""));

    for (events, description) in items {
        // Collect all key strings for this row's events
        let key_spans: Vec<Span<'static>> = events
            .iter()
            .flat_map(|event| keybind.keys_for_event(*event))
            .enumerate()
            .flat_map(|(i, key_str)| {
                let mut spans = vec![
                    Span::raw("<"),
                    Span::raw(key_str).fg(color.help_key.0),
                    Span::raw(">"),
                ];
                if i > 0 {
                    spans.insert(0, Span::raw(" "));
                }
                spans
            })
            .collect();

        key_lines.push(Line::from(key_spans));
        value_lines.push(Line::raw(description).fg(color.help_description.0));
    }

    (key_lines, value_lines)
}

fn join_groups(groups: Vec<Vec<Line<'static>>>) -> Vec<Line<'static>> {
    let mut result: Vec<Line<'static>> = Vec::new();
    let n = groups.len();
    for (i, lines) in groups.into_iter().enumerate() {
        result.extend(lines);
        if i < n - 1 {
            result.push(Line::raw(""));
        }
    }
    result
}
