//! CommitList widget — renders the scrollable commit list with graph, refs, and columns.

use crate::color::ColorTheme;
use crate::config::{Config, ListUiConfig};
use crate::git::{Commit, CommitHash, Head, Ref};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use laurier::highlight::highlight_matched_text;
use once_cell::sync::Lazy;
use ratatui::{
    buffer::Buffer,
    crossterm::event::Event,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{List, ListItem, StatefulWidget, Widget},
};
use rustc_hash::FxHashMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use tui_input::backend::crossterm::EventHandler as TuiInputEventHandler;
use tui_input::Input;

static FUZZY_MATCHER: Lazy<SkimMatcherV2> = Lazy::new(SkimMatcherV2::default);

const ELLIPSIS: &str = "...";

// ---------------------------------------------------------------------------
// DiffCache — LRU cache for file diff content
// ---------------------------------------------------------------------------

const DIFF_CACHE_CAPACITY: usize = 64;

/// LRU cache for file diff content keyed by `(commit_hash, filepath)`.
///
/// When navigating commits with J/K, revisiting a previously-viewed diff is
/// served from this cache instead of re-running the git diff subprocess.
/// Eviction follows FIFO order (oldest-inserted entry evicted when full).
#[derive(Debug)]
pub struct DiffCache {
    map: HashMap<(CommitHash, String), String>,
    /// Insertion-order queue used for eviction.
    order: VecDeque<(CommitHash, String)>,
    capacity: usize,
}

impl Default for DiffCache {
    fn default() -> Self {
        Self {
            map: HashMap::with_capacity(DIFF_CACHE_CAPACITY),
            order: VecDeque::with_capacity(DIFF_CACHE_CAPACITY),
            capacity: DIFF_CACHE_CAPACITY,
        }
    }
}

impl DiffCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return cached diff content for `(commit_hash, filepath)`, if present.
    pub fn get(&self, commit_hash: &CommitHash, filepath: &str) -> Option<&str> {
        self.map
            .get(&(commit_hash.clone(), filepath.to_string()))
            .map(|s| s.as_str())
    }

    /// Insert diff content for `(commit_hash, filepath)`.
    ///
    /// No-op if the entry is already cached.  Evicts the oldest entry when
    /// the cache is at capacity.
    pub fn insert(&mut self, commit_hash: CommitHash, filepath: String, content: String) {
        let key = (commit_hash, filepath);
        if self.map.contains_key(&key) {
            return;
        }
        if self.map.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, content);
    }

    /// Number of entries currently in the cache.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

// ---------------------------------------------------------------------------
// CommitInfo
// ---------------------------------------------------------------------------

/// Owned data for one commit row in the list.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub commit: Commit,
    pub refs: Vec<Ref>,
    /// Lane color from the graph color palette.
    pub graph_color: Color,
    /// Pre-encoded graph image string (iTerm2/Kitty protocol). Empty until Phase 3C.
    pub graph_line: String,
}

// ---------------------------------------------------------------------------
// SearchState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchState {
    Inactive,
    Searching {
        start_index: usize,
        match_index: usize,
        ignore_case: bool,
        fuzzy: bool,
        transient_message: TransientMessage,
    },
    Applied {
        match_index: usize,
        total_match: usize,
    },
}

impl SearchState {
    fn update_match_index(&mut self, index: usize) {
        match self {
            SearchState::Searching { match_index, .. } => *match_index = index,
            SearchState::Applied { match_index, .. } => *match_index = index,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransientMessage {
    None,
    IgnoreCaseOff,
    IgnoreCaseOn,
    FuzzyOff,
    FuzzyOn,
}

// ---------------------------------------------------------------------------
// SearchMatch
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct SearchMatch {
    refs: FxHashMap<String, SearchMatchPosition>,
    subject: Option<SearchMatchPosition>,
    author_name: Option<SearchMatchPosition>,
    commit_hash: Option<SearchMatchPosition>,
    match_index: usize, // 1-based
}

impl SearchMatch {
    fn new(c: &Commit, refs: &[Ref], q: &str, ignore_case: bool, fuzzy: bool) -> Self {
        let matcher = SearchMatcher::new(q, ignore_case, fuzzy);
        let ref_matches = refs
            .iter()
            .filter(|r| !matches!(r, Ref::Stash { .. }))
            .filter_map(|r| {
                matcher
                    .matched_position(r.name())
                    .map(|pos| (r.name().to_string(), pos))
            })
            .collect();
        let subject = matcher.matched_position(&c.subject);
        let author_name = matcher.matched_position(&c.author.name);
        let commit_hash = matcher.matched_position(&c.hash.as_short_hash());
        Self {
            refs: ref_matches,
            subject,
            author_name,
            commit_hash,
            match_index: 0,
        }
    }

    fn matched(&self) -> bool {
        !self.refs.is_empty()
            || self.subject.is_some()
            || self.author_name.is_some()
            || self.commit_hash.is_some()
    }

    fn clear(&mut self) {
        self.refs.clear();
        self.subject = None;
        self.author_name = None;
        self.commit_hash = None;
    }
}

#[derive(Debug, Default, Clone)]
struct SearchMatchPosition {
    matched_indices: Vec<usize>,
}

impl SearchMatchPosition {
    fn new(matched_indices: Vec<usize>) -> Self {
        Self { matched_indices }
    }
}

struct SearchMatcher {
    query: String,
    ignore_case: bool,
    fuzzy: bool,
}

impl SearchMatcher {
    fn new(query: &str, ignore_case: bool, fuzzy: bool) -> Self {
        let query = if ignore_case {
            query.to_lowercase()
        } else {
            query.to_string()
        };
        Self {
            query,
            ignore_case,
            fuzzy,
        }
    }

    fn matched_position(&self, s: &str) -> Option<SearchMatchPosition> {
        if self.query.is_empty() {
            return None;
        }
        if self.fuzzy {
            let result = if self.ignore_case {
                FUZZY_MATCHER.fuzzy_indices(&s.to_lowercase(), &self.query)
            } else {
                FUZZY_MATCHER.fuzzy_indices(s, &self.query)
            };
            result
                .map(|(_, indices)| indices)
                .map(SearchMatchPosition::new)
        } else {
            let result = if self.ignore_case {
                s.to_lowercase().find(&self.query)
            } else {
                s.find(&self.query)
            };
            result
                .map(|p| (p..(p + self.query.len())).collect())
                .map(SearchMatchPosition::new)
        }
    }
}

// ---------------------------------------------------------------------------
// CommitListState
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct CommitListState {
    pub commits: Vec<CommitInfo>,
    commit_hash_set: HashSet<CommitHash>,
    graph_cell_width: u16,
    head: Head,

    ref_name_to_commit_index_map: FxHashMap<String, usize>,

    search_state: SearchState,
    search_input: Input,
    search_matches: Vec<SearchMatch>,

    pub selected: usize,
    pub offset: usize,
    pub total: usize,
    pub height: usize,

    default_ignore_case: bool,
    default_fuzzy: bool,

    /// LRU cache for file diff content; keyed by (commit_hash, filepath).
    pub diff_cache: DiffCache,
}

impl CommitListState {
    pub fn new(
        commits: Vec<CommitInfo>,
        graph_cell_width: u16,
        head: Head,
        ref_name_to_commit_index_map: FxHashMap<String, usize>,
        default_ignore_case: bool,
        default_fuzzy: bool,
    ) -> Self {
        let total = commits.len();
        let commit_hash_set = commits.iter().map(|c| c.commit.hash.clone()).collect();
        let search_matches = vec![SearchMatch::default(); total];
        Self {
            commits,
            commit_hash_set,
            graph_cell_width,
            head,
            ref_name_to_commit_index_map,
            search_state: SearchState::Inactive,
            search_input: Input::default(),
            search_matches,
            selected: 0,
            offset: 0,
            total,
            height: 0,
            default_ignore_case,
            default_fuzzy,
            diff_cache: DiffCache::new(),
        }
    }

    pub fn graph_area_cell_width(&self) -> u16 {
        self.graph_cell_width + 1 // right pad
    }

    // ── Navigation ─────────────────────────────────────────────────────────

    pub fn select_next(&mut self) {
        if self.total == 0 {
            return;
        }
        if self.selected < (self.total - 1).min(self.height.saturating_sub(1)) {
            self.selected += 1;
        } else if self.selected + self.offset < self.total - 1 {
            self.offset += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else if self.offset > 0 {
            self.offset -= 1;
        }
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }

    pub fn select_last(&mut self) {
        if self.total == 0 {
            return;
        }
        self.selected = (self.height.saturating_sub(1)).min(self.total - 1);
        if self.height < self.total {
            self.offset = self.total - self.height;
        }
    }

    pub fn scroll_down(&mut self) {
        if self.offset + self.height < self.total {
            self.offset += 1;
            if self.selected > 0 {
                self.selected -= 1;
            }
        }
    }

    pub fn scroll_up(&mut self) {
        if self.offset > 0 {
            self.offset -= 1;
            if self.selected < self.height.saturating_sub(1) {
                self.selected += 1;
            }
        }
    }

    pub fn scroll_down_page(&mut self) {
        self.scroll_down_height(self.height);
    }

    pub fn scroll_up_page(&mut self) {
        self.scroll_up_height(self.height);
    }

    pub fn scroll_down_half(&mut self) {
        self.scroll_down_height(self.height / 2);
    }

    pub fn scroll_up_half(&mut self) {
        self.scroll_up_height(self.height / 2);
    }

    fn scroll_down_height(&mut self, scroll_height: usize) {
        if scroll_height == 0 {
            return;
        }
        if self.offset + self.height + scroll_height < self.total {
            self.offset += scroll_height;
        } else {
            let old_offset = self.offset;
            let size = self.height.min(self.total);
            self.offset = self.total.saturating_sub(size);
            let moved = self.offset.saturating_sub(old_offset);
            self.selected =
                (self.selected + scroll_height.saturating_sub(moved)).min(size.saturating_sub(1));
        }
    }

    fn scroll_up_height(&mut self, scroll_height: usize) {
        if scroll_height == 0 {
            return;
        }
        if self.offset > scroll_height {
            self.offset -= scroll_height;
        } else {
            let old_offset = self.offset;
            self.offset = 0;
            self.selected = self.selected.saturating_sub(scroll_height - old_offset);
        }
    }

    pub fn select_high(&mut self) {
        self.selected = 0;
    }

    pub fn select_middle(&mut self) {
        if self.total > self.height {
            self.selected = self.height / 2;
        } else {
            self.selected = self.total / 2;
        }
    }

    pub fn select_low(&mut self) {
        if self.total > self.height {
            self.selected = self.height.saturating_sub(1);
        } else {
            self.selected = self.total.saturating_sub(1);
        }
    }

    pub fn select_parent(&mut self) {
        if let Some(parent_hash) = self.selected_commit_parent_hash().cloned() {
            if self.commit_hash_set.contains(&parent_hash) {
                while parent_hash != *self.selected_commit_hash() {
                    self.select_next();
                }
            }
        }
    }

    pub fn selected_commit_parent_hash(&self) -> Option<&CommitHash> {
        self.commits[self.current_selected_index()]
            .commit
            .parent_hashes
            .first()
    }

    fn select_index(&mut self, index: usize) {
        if index < self.total {
            if self.total > self.height && self.height > 0 {
                self.selected = 0;
                self.offset = index;
            } else {
                self.selected = index;
            }
        }
    }

    pub fn select_next_match(&mut self) {
        self.select_next_match_index(self.current_selected_index());
    }

    pub fn select_prev_match(&mut self) {
        self.select_prev_match_index(self.current_selected_index());
    }

    fn select_next_match_index(&mut self, current_index: usize) {
        if self.total == 0 {
            return;
        }
        let mut i = (current_index + 1) % self.total;
        while i != current_index {
            if self.search_matches[i].matched() {
                self.select_index(i);
                self.search_state
                    .update_match_index(self.search_matches[i].match_index);
                return;
            }
            i = if i == self.total - 1 { 0 } else { i + 1 };
        }
    }

    fn select_prev_match_index(&mut self, current_index: usize) {
        if self.total == 0 {
            return;
        }
        let mut i = (current_index + self.total - 1) % self.total;
        while i != current_index {
            if self.search_matches[i].matched() {
                self.select_index(i);
                self.search_state
                    .update_match_index(self.search_matches[i].match_index);
                return;
            }
            i = if i == 0 { self.total - 1 } else { i - 1 };
        }
    }

    pub fn selected_commit_hash(&self) -> &CommitHash {
        &self.commits[self.current_selected_index()].commit.hash
    }

    fn current_selected_index(&self) -> usize {
        self.offset + self.selected
    }

    pub fn current_list_status(&self) -> (usize, usize, usize) {
        (self.selected, self.offset, self.height)
    }

    pub fn reset_height(&mut self, height: usize) {
        self.height = height;
    }

    pub fn select_ref(&mut self, ref_name: &str) {
        if let Some(&index) = self.ref_name_to_commit_index_map.get(ref_name) {
            self.select_index(index);
        }
    }

    pub fn select_commit_hash(&mut self, hash: &CommitHash) {
        if !self.commit_hash_set.contains(hash) {
            return;
        }
        for (i, ci) in self.commits.iter().enumerate() {
            if ci.commit.hash == *hash {
                self.select_index(i);
                break;
            }
        }
    }

    // ── Search ─────────────────────────────────────────────────────────────

    pub fn search_state(&self) -> SearchState {
        self.search_state
    }

    pub fn start_search(&mut self) {
        if let SearchState::Inactive | SearchState::Applied { .. } = self.search_state {
            self.search_state = SearchState::Searching {
                start_index: self.current_selected_index(),
                match_index: 0,
                ignore_case: self.default_ignore_case,
                fuzzy: self.default_fuzzy,
                transient_message: TransientMessage::None,
            };
            self.search_input.reset();
            self.clear_search_matches();
        }
    }

    pub fn handle_search_input(&mut self, key: crossterm::event::KeyEvent) {
        if let SearchState::Searching {
            transient_message, ..
        } = &mut self.search_state
        {
            *transient_message = TransientMessage::None;
        }

        if let SearchState::Searching {
            start_index,
            ignore_case,
            fuzzy,
            ..
        } = self.search_state
        {
            self.search_input.handle_event(&Event::Key(key));
            self.update_search_matches(ignore_case, fuzzy);
            self.select_current_or_next_match_index(start_index);
        }
    }

    pub fn apply_search(&mut self) {
        if let SearchState::Searching { match_index, .. } = self.search_state {
            if self.search_input.value().is_empty() {
                self.search_state = SearchState::Inactive;
            } else {
                let total_match = self.search_matches.iter().filter(|m| m.matched()).count();
                self.search_state = SearchState::Applied {
                    match_index,
                    total_match,
                };
            }
        }
    }

    pub fn cancel_search(&mut self) {
        if let SearchState::Searching { .. } | SearchState::Applied { .. } = self.search_state {
            self.search_state = SearchState::Inactive;
            self.search_input.reset();
            self.clear_search_matches();
        }
    }

    pub fn toggle_ignore_case(&mut self) {
        if let SearchState::Searching {
            ignore_case,
            transient_message,
            ..
        } = &mut self.search_state
        {
            *ignore_case = !*ignore_case;
            *transient_message = if *ignore_case {
                TransientMessage::IgnoreCaseOn
            } else {
                TransientMessage::IgnoreCaseOff
            };
        }

        if let SearchState::Searching {
            start_index,
            ignore_case,
            fuzzy,
            ..
        } = self.search_state
        {
            self.update_search_matches(ignore_case, fuzzy);
            self.select_current_or_next_match_index(start_index);
        }
    }

    pub fn toggle_fuzzy(&mut self) {
        if let SearchState::Searching {
            fuzzy,
            transient_message,
            ..
        } = &mut self.search_state
        {
            *fuzzy = !*fuzzy;
            *transient_message = if *fuzzy {
                TransientMessage::FuzzyOn
            } else {
                TransientMessage::FuzzyOff
            };
        }

        if let SearchState::Searching {
            start_index,
            ignore_case,
            fuzzy,
            ..
        } = self.search_state
        {
            self.update_search_matches(ignore_case, fuzzy);
            self.select_current_or_next_match_index(start_index);
        }
    }

    pub fn search_query_string(&self) -> Option<String> {
        if let SearchState::Searching { .. } = self.search_state {
            let q = self.search_input.value();
            Some(format!("/{q}"))
        } else {
            None
        }
    }

    pub fn matched_query_string(&self) -> Option<(String, bool)> {
        if let SearchState::Applied {
            match_index,
            total_match,
        } = self.search_state
        {
            let q = self.search_input.value();
            if total_match == 0 {
                Some((format!("No matches found (query: \"{q}\")"), false))
            } else {
                Some((
                    format!("Match {match_index} of {total_match} (query: \"{q}\")"),
                    true,
                ))
            }
        } else {
            None
        }
    }

    pub fn search_query_cursor_position(&self) -> u16 {
        self.search_input.visual_cursor() as u16 + 1 // +1 for the leading "/"
    }

    pub fn transient_message_string(&self) -> Option<String> {
        if let SearchState::Searching {
            transient_message, ..
        } = self.search_state
        {
            match transient_message {
                TransientMessage::None => None,
                TransientMessage::IgnoreCaseOn => Some("Ignore case: ON ".to_string()),
                TransientMessage::IgnoreCaseOff => Some("Ignore case: OFF".to_string()),
                TransientMessage::FuzzyOn => Some("Fuzzy match: ON ".to_string()),
                TransientMessage::FuzzyOff => Some("Fuzzy match: OFF".to_string()),
            }
        } else {
            None
        }
    }

    fn update_search_matches(&mut self, ignore_case: bool, fuzzy: bool) {
        let q = self.search_input.value().to_string();
        let mut match_index = 1;
        for i in 0..self.commits.len() {
            let commit_info = &self.commits[i];
            let mut m = SearchMatch::new(
                &commit_info.commit,
                &commit_info.refs,
                &q,
                ignore_case,
                fuzzy,
            );
            if m.matched() {
                m.match_index = match_index;
                match_index += 1;
            }
            self.search_matches[i] = m;
        }
    }

    fn clear_search_matches(&mut self) {
        self.search_matches.iter_mut().for_each(|m| m.clear());
    }

    fn select_current_or_next_match_index(&mut self, current_index: usize) {
        if self.search_matches[current_index].matched() {
            self.select_index(current_index);
            let idx = self.search_matches[current_index].match_index;
            self.search_state.update_match_index(idx);
        } else {
            self.select_next_match_index(current_index);
        }
    }
}

// ---------------------------------------------------------------------------
// CommitList widget
// ---------------------------------------------------------------------------

pub struct CommitList {
    config: Rc<Config>,
}

impl CommitList {
    pub fn new(config: Rc<Config>) -> Self {
        Self { config }
    }
}

impl StatefulWidget for CommitList {
    type State = CommitListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Update height from render area
        state.height = area.height as usize;

        // Adjust offset if we're past the end
        if state.total > state.height && state.total - state.height < state.offset {
            let diff = state.offset - (state.total - state.height);
            state.selected += diff;
            state.offset -= diff;
        }
        if state.height > 0 && state.selected >= state.height {
            let diff = state.selected - state.height + 1;
            state.selected -= diff;
            state.offset += diff;
        }

        if state.total == 0 || area.is_empty() {
            return;
        }

        let constraints = calc_cell_widths(
            area.width,
            self.config.ui.list.subject_min_width as u16,
            state.graph_area_cell_width(),
            self.config.ui.list.name_width as u16,
            self.config.ui.list.date_width as u16,
            &self.config.ui.list.columns,
        );
        let chunks = Layout::horizontal(constraints).split(area);

        for (i, col) in self.config.ui.list.columns.iter().enumerate() {
            match col.as_str() {
                "Graph" => render_graph(buf, chunks[i], state),
                "Marker" => render_marker(buf, chunks[i], state),
                "Subject" => render_subject(buf, chunks[i], state, &self.config.color),
                "Name" => render_name(buf, chunks[i], state, &self.config.color),
                "Hash" => render_hash(buf, chunks[i], state, &self.config.color),
                "Date" => render_date(
                    buf,
                    chunks[i],
                    state,
                    &self.config.color,
                    &self.config.ui.list,
                ),
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Column renderers
// ---------------------------------------------------------------------------

fn render_graph(buf: &mut Buffer, area: Rect, state: &CommitListState) {
    if area.is_empty() {
        return;
    }
    for (i, ci) in rendering_iter(state).collect::<Vec<_>>() {
        if !ci.graph_line.is_empty() {
            let y = area.top() + i as u16;
            let graph_x = if area.left() == 0 && area.width > 1 {
                buf[(area.left(), y)].set_symbol(" ");
                area.left() + 1
            } else {
                area.left()
            };

            buf[(graph_x, y)].set_symbol(&ci.graph_line);
            for x in (graph_x + 1)..area.right() {
                buf[(x, y)].set_skip(true);
            }
        }
    }
}

fn render_marker(buf: &mut Buffer, area: Rect, state: &CommitListState) {
    if area.is_empty() {
        return;
    }
    let items: Vec<ListItem> = rendering_iter(state)
        .map(|(_, ci)| ListItem::new("│".fg(ci.graph_color)))
        .collect();
    Widget::render(List::new(items), area, buf);
}

fn render_subject(buf: &mut Buffer, area: Rect, state: &CommitListState, theme: &ColorTheme) {
    let max_width = (area.width as usize).saturating_sub(2);
    if area.is_empty() || max_width == 0 {
        return;
    }
    let items: Vec<ListItem> = rendering_iter(state)
        .map(|(i, ci)| {
            let mut spans = refs_spans(
                ci,
                &state.head,
                &state.search_matches[state.offset + i].refs,
                theme,
            );
            let refs_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            let subj_width = max_width.saturating_sub(refs_width);
            let commit = &ci.commit;
            if subj_width > ELLIPSIS.len() {
                let char_count = commit.subject.chars().count();
                let truncate = char_count > subj_width;
                let subject = if truncate {
                    let s: String = commit
                        .subject
                        .chars()
                        .take(subj_width.saturating_sub(ELLIPSIS.len()))
                        .collect();
                    format!("{s}{ELLIPSIS}")
                } else {
                    commit.subject.clone()
                };
                let sub_spans =
                    if let Some(pos) = state.search_matches[state.offset + i].subject.clone() {
                        highlighted_spans(
                            Span::raw(subject),
                            pos,
                            theme.list_subject.0,
                            Modifier::empty(),
                            theme.search_match.0,
                            theme.search_match_bg.0,
                            truncate,
                        )
                    } else {
                        vec![subject.fg(theme.list_subject.0)]
                    };
                spans.extend(sub_spans);
            }
            to_list_item(i, spans, state, theme)
        })
        .collect();
    Widget::render(List::new(items), area, buf);
}

fn render_name(buf: &mut Buffer, area: Rect, state: &CommitListState, theme: &ColorTheme) {
    let max_width = (area.width as usize).saturating_sub(2);
    if area.is_empty() || max_width == 0 {
        return;
    }
    let items: Vec<ListItem> = rendering_iter(state)
        .map(|(i, ci)| {
            let name = &ci.commit.author.name;
            let char_count = name.chars().count();
            let truncate = char_count > max_width;
            let display = if truncate {
                let s: String = name
                    .chars()
                    .take(max_width.saturating_sub(ELLIPSIS.len()))
                    .collect();
                format!("{s}{ELLIPSIS}")
            } else {
                name.clone()
            };
            let spans =
                if let Some(pos) = state.search_matches[state.offset + i].author_name.clone() {
                    highlighted_spans(
                        Span::raw(display),
                        pos,
                        theme.list_author.0,
                        Modifier::empty(),
                        theme.search_match.0,
                        theme.search_match_bg.0,
                        truncate,
                    )
                } else {
                    vec![display.fg(theme.list_author.0)]
                };
            to_list_item(i, spans, state, theme)
        })
        .collect();
    Widget::render(List::new(items), area, buf);
}

fn render_hash(buf: &mut Buffer, area: Rect, state: &CommitListState, theme: &ColorTheme) {
    if area.is_empty() {
        return;
    }
    let items: Vec<ListItem> = rendering_iter(state)
        .map(|(i, ci)| {
            let hash = ci.commit.hash.as_short_hash();
            let spans =
                if let Some(pos) = state.search_matches[state.offset + i].commit_hash.clone() {
                    highlighted_spans(
                        Span::raw(hash),
                        pos,
                        theme.list_hash.0,
                        Modifier::empty(),
                        theme.search_match.0,
                        theme.search_match_bg.0,
                        false,
                    )
                } else {
                    vec![hash.fg(theme.list_hash.0)]
                };
            to_list_item(i, spans, state, theme)
        })
        .collect();
    Widget::render(List::new(items), area, buf);
}

fn render_date(
    buf: &mut Buffer,
    area: Rect,
    state: &CommitListState,
    theme: &ColorTheme,
    list_cfg: &ListUiConfig,
) {
    if area.is_empty() {
        return;
    }
    let items: Vec<ListItem> = rendering_iter(state)
        .map(|(i, ci)| {
            let date_str = format_date(
                &ci.commit.author.date,
                &list_cfg.date_format,
                list_cfg.date_local,
            );
            to_list_item(i, vec![date_str.fg(theme.list_date.0)], state, theme)
        })
        .collect();
    Widget::render(List::new(items), area, buf);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rendering_iter(state: &CommitListState) -> impl Iterator<Item = (usize, &CommitInfo)> {
    state
        .commits
        .iter()
        .skip(state.offset)
        .take(state.height)
        .enumerate()
}

fn to_list_item<'a>(
    i: usize,
    spans: Vec<Span<'a>>,
    state: &CommitListState,
    theme: &ColorTheme,
) -> ListItem<'a> {
    let mut all_spans = vec![Span::raw(" ")];
    all_spans.extend(spans);
    all_spans.push(Span::raw(" "));
    let mut line = Line::from(all_spans);
    if i == state.selected {
        line = line.bg(theme.list_selected_bg.0);
    }
    ListItem::new(line)
}

fn refs_spans<'a>(
    ci: &CommitInfo,
    head: &Head,
    refs_matches: &FxHashMap<String, SearchMatchPosition>,
    theme: &'a ColorTheme,
) -> Vec<Span<'a>> {
    let refs = &ci.refs;

    // Single stash ref
    if refs.len() == 1 {
        if let Ref::Stash { name, .. } = &refs[0] {
            return vec![
                Span::raw(name.clone()).fg(theme.list_stash.0).bold(),
                Span::raw(" "),
            ];
        }
    }

    let ref_spans: Vec<(Vec<Span>, &str)> = refs
        .iter()
        .filter_map(|r| match r {
            Ref::Branch { name, .. } => Some((name.as_str(), theme.list_branch_local.0)),
            Ref::RemoteBranch { name, .. } => Some((name.as_str(), theme.list_branch_remote.0)),
            Ref::Tag { name, .. } => Some((name.as_str(), theme.list_tag.0)),
            Ref::Stash { .. } => None,
        })
        .map(|(name, fg)| {
            let spans = refs_matches
                .get(name)
                .map(|pos| {
                    highlighted_spans(
                        Span::raw(name.to_string()),
                        pos.clone(),
                        fg,
                        Modifier::BOLD,
                        theme.search_match.0,
                        theme.search_match_bg.0,
                        false,
                    )
                })
                .unwrap_or_else(|| vec![Span::raw(name.to_string()).fg(fg).bold()]);
            (spans, name)
        })
        .collect();

    if ref_spans.is_empty() {
        // Check for detached HEAD pointing at this commit
        if let Head::Detached { target } = head {
            if ci.commit.hash == *target {
                return vec![
                    Span::raw("(").fg(theme.list_head.0).bold(),
                    Span::raw("HEAD").fg(theme.list_head.0).bold(),
                    Span::raw(") ").fg(theme.list_head.0).bold(),
                ];
            }
        }
        return vec![];
    }

    let mut spans = vec![Span::raw("(").fg(theme.list_head.0).bold()];

    if let Head::Detached { target } = head {
        if ci.commit.hash == *target {
            spans.push(Span::raw("HEAD").fg(theme.list_head.0).bold());
            if !ref_spans.is_empty() {
                spans.push(Span::raw(", ").fg(theme.list_head.0).bold());
            }
        }
    }

    for (i, (ref_span_vec, ref_name)) in ref_spans.into_iter().enumerate() {
        if let Head::Branch { name } = head {
            if ref_name == name {
                spans.push(Span::raw("HEAD -> ").fg(theme.list_head.0).bold());
            }
        }
        spans.extend(ref_span_vec);
        if i < refs.len() - 1 {
            spans.push(Span::raw(", ").fg(theme.list_head.0).bold());
        }
    }

    spans.push(Span::raw(") ").fg(theme.list_head.0).bold());

    if spans.len() == 2 {
        spans.clear();
    }

    spans
}

fn highlighted_spans(
    s: Span<'_>,
    pos: SearchMatchPosition,
    base_fg: Color,
    base_modifier: Modifier,
    match_fg: Color,
    match_bg: Color,
    truncate: bool,
) -> Vec<Span<'static>> {
    let mut hm = highlight_matched_text(vec![s])
        .matched_indices(pos.matched_indices)
        .not_matched_style(Style::default().fg(base_fg).add_modifier(base_modifier))
        .matched_style(
            Style::default()
                .fg(match_fg)
                .bg(match_bg)
                .add_modifier(base_modifier),
        );
    if truncate {
        hm = hm.ellipsis(ELLIPSIS);
    }
    hm.into_spans()
}

fn calc_cell_widths(
    area_width: u16,
    subject_min_width: u16,
    graph_width: u16,
    name_width: u16,
    date_width: u16,
    columns: &[String],
) -> Vec<Constraint> {
    let pad: u16 = 2;
    let (mut graph_w, mut marker_w, mut name_w, mut hash_w, mut date_w) =
        (0u16, 0u16, 0u16, 0u16, 0u16);

    for col in columns {
        match col.as_str() {
            "Graph" => graph_w = graph_width,
            "Marker" => marker_w = 1,
            "Name" => name_w = name_width + pad,
            "Hash" => hash_w = 7 + pad,
            "Date" => date_w = date_width + pad,
            _ => {}
        }
    }

    let mut total = graph_w + marker_w + hash_w + name_w + date_w + subject_min_width;

    if total > area_width {
        total = total.saturating_sub(name_w);
        name_w = 0;
    }
    if total > area_width {
        total = total.saturating_sub(date_w);
        date_w = 0;
    }
    if total > area_width {
        let _ = total; // suppress unused warning
        hash_w = 0;
    }

    columns
        .iter()
        .map(|col| match col.as_str() {
            "Graph" => Constraint::Length(graph_w),
            "Marker" => Constraint::Length(marker_w),
            "Subject" => Constraint::Min(0),
            "Name" => Constraint::Length(name_w),
            "Hash" => Constraint::Length(hash_w),
            "Date" => Constraint::Length(date_w),
            _ => Constraint::Length(0),
        })
        .collect()
}

fn format_date(date_str: &str, fmt: &str, local: bool) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
        if local {
            dt.with_timezone(&chrono::Local).format(fmt).to_string()
        } else {
            dt.format(fmt).to_string()
        }
    } else {
        // Fallback: try parsing as "%Y-%m-%dT%H:%M:%S%z" (iso-strict without colon in offset)
        date_str.get(..10).unwrap_or(date_str).to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_commit(hash: &str) -> Commit {
        Commit {
            hash: CommitHash::from(hash),
            author: crate::git::Author {
                name: "Test Author".to_string(),
                email: "test@example.com".to_string(),
                date: "2024-01-15T10:00:00+00:00".to_string(),
            },
            committer: crate::git::Author {
                name: "Test Committer".to_string(),
                email: "test@example.com".to_string(),
                date: "2024-01-15T10:00:00+00:00".to_string(),
            },
            subject: format!("Commit {hash}"),
            body: String::new(),
            parent_hashes: vec![],
            commit_type: crate::git::CommitType::Commit,
        }
    }

    fn make_state(n: usize) -> CommitListState {
        let commits: Vec<CommitInfo> = (0..n)
            .map(|i| CommitInfo {
                commit: make_commit(&format!("{i:040}")),
                refs: vec![],
                graph_color: Color::White,
                graph_line: String::new(),
            })
            .collect();
        CommitListState::new(commits, 6, Head::None, FxHashMap::default(), false, false)
    }

    #[test]
    fn select_next_and_prev() {
        let mut state = make_state(5);
        state.height = 5;
        state.select_next();
        assert_eq!(state.selected, 1);
        state.select_prev();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn select_first_and_last() {
        let mut state = make_state(10);
        state.height = 5;
        state.select_last();
        // selected should be 4 (height-1), offset should be 5
        assert_eq!(state.selected, 4);
        assert_eq!(state.offset, 5);
        state.select_first();
        assert_eq!(state.selected, 0);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn scroll_down_moves_offset() {
        let mut state = make_state(10);
        state.height = 5;
        state.scroll_down();
        assert_eq!(state.offset, 1);
    }

    #[test]
    fn search_state_transitions() {
        let mut state = make_state(3);
        state.height = 3;
        assert_eq!(state.search_state(), SearchState::Inactive);
        state.start_search();
        assert!(matches!(
            state.search_state(),
            SearchState::Searching { .. }
        ));
        state.cancel_search();
        assert_eq!(state.search_state(), SearchState::Inactive);
    }

    #[test]
    fn format_date_valid() {
        let s = format_date("2024-01-15T10:00:00+00:00", "%Y-%m-%d", false);
        assert_eq!(s, "2024-01-15");
    }

    // ── DiffCache ───────────────────────────────────────────────────────────

    fn hash(s: &str) -> CommitHash {
        CommitHash::from(s)
    }

    #[test]
    fn diff_cache_miss_returns_none() {
        let cache = DiffCache::new();
        assert!(cache.get(&hash("abc"), "src/main.rs").is_none());
    }

    #[test]
    fn diff_cache_hit_after_insert() {
        let mut cache = DiffCache::new();
        cache.insert(
            hash("abc"),
            "src/main.rs".to_string(),
            "diff content".to_string(),
        );
        assert_eq!(cache.get(&hash("abc"), "src/main.rs"), Some("diff content"));
    }

    #[test]
    fn diff_cache_different_keys_independent() {
        let mut cache = DiffCache::new();
        cache.insert(hash("abc"), "foo.rs".to_string(), "diff-a".to_string());
        cache.insert(hash("abc"), "bar.rs".to_string(), "diff-b".to_string());
        assert_eq!(cache.get(&hash("abc"), "foo.rs"), Some("diff-a"));
        assert_eq!(cache.get(&hash("abc"), "bar.rs"), Some("diff-b"));
    }

    #[test]
    fn diff_cache_no_duplicate_on_repeated_insert() {
        let mut cache = DiffCache::new();
        cache.insert(hash("abc"), "foo.rs".to_string(), "v1".to_string());
        cache.insert(hash("abc"), "foo.rs".to_string(), "v2".to_string());
        // First insert wins; duplicate is a no-op.
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn diff_cache_evicts_oldest_when_full() {
        let mut cache = DiffCache::new();
        // Fill beyond default capacity (64); first entry should be evicted.
        for i in 0..=DIFF_CACHE_CAPACITY {
            cache.insert(
                hash(&format!("{i:040}")),
                "file.rs".to_string(),
                format!("diff-{i}"),
            );
        }
        assert_eq!(cache.len(), DIFF_CACHE_CAPACITY);
        // The very first entry (i=0) should have been evicted.
        assert!(cache.get(&hash(&format!("{:040}", 0)), "file.rs").is_none());
        // The last entry should be present.
        assert!(cache
            .get(&hash(&format!("{:040}", DIFF_CACHE_CAPACITY)), "file.rs")
            .is_some());
    }

    #[test]
    fn calc_cell_widths_basic() {
        let columns = vec![
            "Graph".to_string(),
            "Subject".to_string(),
            "Hash".to_string(),
        ];
        let widths = calc_cell_widths(80, 20, 6, 20, 10, &columns);
        assert_eq!(widths.len(), 3);
        assert!(matches!(widths[1], Constraint::Min(0)));
    }

    #[test]
    fn render_graph_insets_first_terminal_column() {
        let mut state = make_state(1);
        state.commits[0].graph_line = "abc".to_string();
        state.height = 1;

        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::empty(area);

        render_graph(&mut buf, area, &state);

        assert_eq!(buf[(0, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].symbol(), "abc");
    }
}
