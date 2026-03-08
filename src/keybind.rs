//! Keybinding system for gitpeek.
//!
//! Loads default bindings from `assets/default-keybind.toml` (embedded at
//! compile time) and merges any user overrides from the config file.
//!
//! Architecture:
//! - `UserEvent` — all triggerable actions
//! - `KeyBind`   — flat `FxHashMap<KeyEvent, UserEvent>` for global + list actions
//! - Detail/refs views handle their own context-specific key mappings directly,
//!   since some keys overlap (e.g., "d" = user_command in list, scroll in detail)

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// UserEvent
// ---------------------------------------------------------------------------

/// All possible user-triggered actions in gitpeek.
///
/// Global and list-view actions are mapped via [`KeyBind`].
/// Detail-view and refs-view specific actions are handled by those views
/// directly (since some keys conflict across contexts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UserEvent {
    // ── Global ────────────────────────────────────────────────────────────
    Quit,
    ForceQuit,
    HelpToggle,
    Cancel,
    Refresh,

    // ── Navigation (list + detail) ─────────────────────────────────────────
    NavigateDown,
    NavigateUp,
    GoToTop,
    GoToBottom,
    SelectTop,
    SelectMiddle,
    SelectBottom,
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    /// Scroll viewport down without moving selection
    ScrollDown,
    /// Scroll viewport up without moving selection
    ScrollUp,
    /// Move selection down, scrolling if needed
    SelectDown,
    /// Move selection up, scrolling if needed
    SelectUp,

    // ── List view ──────────────────────────────────────────────────────────
    /// Open detail view for selected commit
    Confirm,
    /// Open refs panel
    OpenRefs,
    SearchStart,
    SearchNext,
    SearchPrev,
    SearchToggleCase,
    SearchToggleFuzzy,
    /// Jump to the parent commit
    JumpToParent,
    /// Copy 7-char short hash to clipboard
    CopyShortHash,
    /// Copy full 40-char hash to clipboard
    CopyFullHash,
    /// Run configured user command (slot given by numeric prefix, default 1)
    UserCommand,

    // ── Detail view ────────────────────────────────────────────────────────
    /// Toggle directory expand/collapse, or select file in file tree
    ToggleDirectory,
    /// Collapse current directory (or move to parent)
    CollapseDirectory,
    /// Scroll diff content down 1 line
    ScrollDiffDown,
    /// Scroll diff content up 1 line
    ScrollDiffUp,
    /// Scroll diff content down half a page
    ScrollDiffPageDown,
    /// Scroll diff content up half a page
    ScrollDiffPageUp,
    /// Horizontal scroll diff left (5 cols)
    ScrollDiffLeft,
    /// Horizontal scroll diff right (5 cols)
    ScrollDiffRight,
    /// Fast horizontal scroll left (20 cols)
    ScrollDiffFastLeft,
    /// Fast horizontal scroll right (20 cols)
    ScrollDiffFastRight,
    /// Navigate to next commit (stay in detail view)
    NextCommit,
    /// Navigate to previous commit (stay in detail view)
    PrevCommit,

    // ── Internal ───────────────────────────────────────────────────────────
    /// Internal marker for raw key pass-through (e.g. typing in search box).
    /// Not exposed in keybind config.
    Unknown,
}

impl UserEvent {
    /// Returns true if a numeric prefix should be applied to this event (e.g. `5j` moves 5 down).
    pub fn is_countable(self) -> bool {
        matches!(
            self,
            UserEvent::NavigateDown
                | UserEvent::NavigateUp
                | UserEvent::GoToTop
                | UserEvent::GoToBottom
                | UserEvent::SelectTop
                | UserEvent::SelectMiddle
                | UserEvent::SelectBottom
                | UserEvent::HalfPageDown
                | UserEvent::HalfPageUp
                | UserEvent::PageDown
                | UserEvent::PageUp
                | UserEvent::ScrollDown
                | UserEvent::ScrollUp
                | UserEvent::SelectDown
                | UserEvent::SelectUp
                | UserEvent::JumpToParent
                | UserEvent::SearchNext
                | UserEvent::SearchPrev
                | UserEvent::UserCommand
        )
    }
}

impl FromStr for UserEvent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quit" => Ok(Self::Quit),
            "force_quit" => Ok(Self::ForceQuit),
            "help_toggle" => Ok(Self::HelpToggle),
            "cancel" => Ok(Self::Cancel),
            "refresh" => Ok(Self::Refresh),
            "navigate_down" => Ok(Self::NavigateDown),
            "navigate_up" => Ok(Self::NavigateUp),
            "go_to_top" => Ok(Self::GoToTop),
            "go_to_bottom" => Ok(Self::GoToBottom),
            "select_top" => Ok(Self::SelectTop),
            "select_middle" => Ok(Self::SelectMiddle),
            "select_bottom" => Ok(Self::SelectBottom),
            "half_page_down" => Ok(Self::HalfPageDown),
            "half_page_up" => Ok(Self::HalfPageUp),
            "page_down" => Ok(Self::PageDown),
            "page_up" => Ok(Self::PageUp),
            "scroll_down" => Ok(Self::ScrollDown),
            "scroll_up" => Ok(Self::ScrollUp),
            "select_down" => Ok(Self::SelectDown),
            "select_up" => Ok(Self::SelectUp),
            "confirm" => Ok(Self::Confirm),
            "open_refs" => Ok(Self::OpenRefs),
            "search_start" => Ok(Self::SearchStart),
            "search_next" => Ok(Self::SearchNext),
            "search_prev" => Ok(Self::SearchPrev),
            "search_toggle_case" => Ok(Self::SearchToggleCase),
            "search_toggle_fuzzy" => Ok(Self::SearchToggleFuzzy),
            "jump_to_parent" => Ok(Self::JumpToParent),
            "copy_short_hash" => Ok(Self::CopyShortHash),
            "copy_full_hash" => Ok(Self::CopyFullHash),
            "user_command" => Ok(Self::UserCommand),
            "toggle_directory" => Ok(Self::ToggleDirectory),
            "collapse_directory" => Ok(Self::CollapseDirectory),
            "scroll_diff_down" => Ok(Self::ScrollDiffDown),
            "scroll_diff_up" => Ok(Self::ScrollDiffUp),
            "scroll_diff_page_down" => Ok(Self::ScrollDiffPageDown),
            "scroll_diff_page_up" => Ok(Self::ScrollDiffPageUp),
            "scroll_diff_left" => Ok(Self::ScrollDiffLeft),
            "scroll_diff_right" => Ok(Self::ScrollDiffRight),
            "scroll_diff_fast_left" => Ok(Self::ScrollDiffFastLeft),
            "scroll_diff_fast_right" => Ok(Self::ScrollDiffFastRight),
            "next_commit" => Ok(Self::NextCommit),
            "prev_commit" => Ok(Self::PrevCommit),
            other => Err(format!("Unknown action: {other:?}")),
        }
    }
}

// ---------------------------------------------------------------------------
// UserEventWithCount
// ---------------------------------------------------------------------------

/// A user event with an optional numeric repeat prefix (e.g. `5j` = move down 5).
#[derive(Debug, Clone, Copy)]
pub struct UserEventWithCount {
    pub event: UserEvent,
    /// Repeat count from numeric prefix; `None` means "no prefix" (treat as 1).
    pub count: Option<usize>,
}

impl UserEventWithCount {
    pub fn new(event: UserEvent, count: Option<usize>) -> Self {
        Self { event, count }
    }

    /// Effective repeat count (defaults to 1 if no prefix given).
    pub fn effective_count(self) -> usize {
        self.count.unwrap_or(1)
    }
}

// ---------------------------------------------------------------------------
// Key parsing
// ---------------------------------------------------------------------------

/// Parse a key string into a crossterm [`KeyEvent`].
///
/// Supports modifier prefixes `ctrl-`, `alt-`, `shift-` (combinable in any order)
/// followed by:
/// - Named keys: `esc`, `enter`, `space`, `tab`, `backspace`, `delete`, `insert`,
///   `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1`–`f12`
/// - Single characters: `a`–`z`, `A`–`Z`, `0`–`9`, symbols (`/`, `?`, `;`, etc.)
///   Uppercase letters implicitly add `SHIFT`.
pub fn parse_key(s: &str) -> Result<KeyEvent, String> {
    let s = s.trim();
    let mut modifiers = KeyModifiers::NONE;
    let mut remaining = s;

    // Consume modifier prefixes in any order
    loop {
        if let Some(rest) = remaining.strip_prefix("ctrl-") {
            modifiers |= KeyModifiers::CONTROL;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("alt-") {
            modifiers |= KeyModifiers::ALT;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix("shift-") {
            modifiers |= KeyModifiers::SHIFT;
            remaining = rest;
        } else {
            break;
        }
    }

    let code = key_code_from_str(remaining, &mut modifiers)
        .ok_or_else(|| format!("Unknown key: {remaining:?}"))?;

    Ok(KeyEvent::new(code, modifiers))
}

fn key_code_from_str(s: &str, modifiers: &mut KeyModifiers) -> Option<KeyCode> {
    let lower = s.to_lowercase();
    match lower.as_str() {
        "esc" | "escape" => Some(KeyCode::Esc),
        "enter" | "return" => Some(KeyCode::Enter),
        "space" => Some(KeyCode::Char(' ')),
        "tab" => Some(KeyCode::Tab),
        "backspace" => Some(KeyCode::Backspace),
        "delete" | "del" => Some(KeyCode::Delete),
        "insert" | "ins" => Some(KeyCode::Insert),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "pageup" | "pgup" => Some(KeyCode::PageUp),
        "pagedown" | "pgdn" => Some(KeyCode::PageDown),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        _ => {
            // F1–F12
            if lower.len() > 1 && lower.starts_with('f') {
                if let Ok(n) = lower[1..].parse::<u8>() {
                    if (1..=12).contains(&n) {
                        return Some(KeyCode::F(n));
                    }
                }
            }
            // Single character — uppercase implies SHIFT
            if s.chars().count() == 1 {
                let ch = s.chars().next()?;
                if ch.is_ascii_uppercase() {
                    *modifiers |= KeyModifiers::SHIFT;
                }
                Some(KeyCode::Char(ch))
            } else {
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// KeyBind
// ---------------------------------------------------------------------------

/// Raw keybind section deserialized from TOML.
#[derive(Debug, Default, Deserialize)]
struct KeybindToml {
    #[serde(default)]
    keybind: std::collections::HashMap<String, Vec<String>>,
}

/// Keybinding configuration mapping [`KeyEvent`] → [`UserEvent`].
///
/// Loaded from the embedded `assets/default-keybind.toml` merged with
/// user overrides from the config file's `[keybind]` section.
#[derive(Debug, Clone)]
pub struct KeyBind {
    map: FxHashMap<KeyEvent, UserEvent>,
}

impl KeyBind {
    const DEFAULT_TOML: &'static str = include_str!("../assets/default-keybind.toml");

    /// Build a `KeyBind` from embedded defaults, optionally merged with
    /// user overrides from the config file.
    ///
    /// `overrides` should be the `[keybind]` table from the user's config
    /// as a `toml::Value`. Pass `None` to use defaults only.
    pub fn new(overrides: Option<&toml::Value>) -> Result<Self, String> {
        let defaults: KeybindToml = toml::from_str(Self::DEFAULT_TOML)
            .map_err(|e| format!("Failed to parse default keybinds: {e}"))?;

        let mut action_map: std::collections::HashMap<String, Vec<String>> = defaults.keybind;

        // Merge user overrides: replace entire action entry if present
        if let Some(ov) = overrides {
            if let Some(table) = ov.as_table() {
                for (action, keys_val) in table {
                    if let Some(arr) = keys_val.as_array() {
                        let keys: Vec<String> = arr
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect();
                        action_map.insert(action.clone(), keys);
                    }
                }
            }
        }

        // Collect the set of overridden action names so we can process them last.
        // This ensures override assignments win any key conflicts with non-overridden defaults.
        let overridden_actions: std::collections::HashSet<&str> = overrides
            .and_then(|v| v.as_table())
            .map(|t| t.keys().map(String::as_str).collect())
            .unwrap_or_default();

        // Build KeyEvent → UserEvent map.
        // Two passes: defaults first, overrides second (overrides win key conflicts).
        let mut map: FxHashMap<KeyEvent, UserEvent> = FxHashMap::default();

        let build_entries = |map: &mut FxHashMap<KeyEvent, UserEvent>,
                              action_str: &str,
                              keys: &[String]| {
            let event = match action_str.parse::<UserEvent>() {
                Ok(ev) => ev,
                Err(_) => return, // Unknown action — skip silently
            };
            map.retain(|_, v| *v != event);
            for key_str in keys {
                match parse_key(key_str) {
                    Ok(key_event) => {
                        map.insert(key_event, event);
                    }
                    Err(e) => {
                        eprintln!("gitpeek: keybind warning — {e}");
                    }
                }
            }
        };

        // Pass 1: non-overridden default actions
        for (action_str, keys) in &action_map {
            if overridden_actions.contains(action_str.as_str()) {
                continue;
            }
            build_entries(&mut map, action_str, keys);
        }

        // Pass 2: overridden actions (applied last, win any conflicts)
        for (action_str, keys) in &action_map {
            if !overridden_actions.contains(action_str.as_str()) {
                continue;
            }
            build_entries(&mut map, action_str, keys);
        }


        Ok(Self { map })
    }

    /// Look up the action bound to a key event.
    pub fn get(&self, key: &KeyEvent) -> Option<&UserEvent> {
        self.map.get(key)
    }

    /// Iterate over all (key, action) bindings (used for help view generation).
    pub fn iter(&self) -> impl Iterator<Item = (&KeyEvent, &UserEvent)> {
        self.map.iter()
    }

    /// Return all key strings bound to the given event, sorted for stable display.
    pub fn keys_for_event(&self, event: UserEvent) -> Vec<String> {
        let mut keys: Vec<String> = self
            .map
            .iter()
            .filter(|(_, v)| **v == event)
            .map(|(k, _)| format_key_event(k))
            .collect();
        keys.sort();
        keys
    }
}

/// Format a `KeyEvent` back into a human-readable key string (e.g. `"ctrl-d"`, `"?"`).
pub fn format_key_event(key: &KeyEvent) -> String {
    let mut s = String::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        s.push_str("ctrl-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        s.push_str("alt-");
    }
    // SHIFT prefix only for named keys — uppercase chars already carry shift visually
    let is_char = matches!(key.code, KeyCode::Char(_));
    if key.modifiers.contains(KeyModifiers::SHIFT) && !is_char {
        s.push_str("shift-");
    }
    match key.code {
        KeyCode::Char(c) => s.push(c),
        KeyCode::Enter => s.push_str("enter"),
        KeyCode::Esc => s.push_str("esc"),
        KeyCode::Tab => s.push_str("tab"),
        KeyCode::Backspace => s.push_str("backspace"),
        KeyCode::Delete => s.push_str("delete"),
        KeyCode::Insert => s.push_str("insert"),
        KeyCode::Home => s.push_str("home"),
        KeyCode::End => s.push_str("end"),
        KeyCode::PageUp => s.push_str("pageup"),
        KeyCode::PageDown => s.push_str("pagedown"),
        KeyCode::Up => s.push_str("up"),
        KeyCode::Down => s.push_str("down"),
        KeyCode::Left => s.push_str("left"),
        KeyCode::Right => s.push_str("right"),
        KeyCode::F(n) => s.push_str(&format!("f{n}")),
        _ => s.push('?'),
    }
    s
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn key(s: &str) -> KeyEvent {
        parse_key(s).unwrap_or_else(|e| panic!("parse_key({s:?}) failed: {e}"))
    }

    // ── parse_key ──────────────────────────────────────────────────────────

    #[test]
    fn parse_lowercase_char() {
        let ev = key("j");
        assert_eq!(ev.code, KeyCode::Char('j'));
        assert_eq!(ev.modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn parse_uppercase_char_implies_shift() {
        let ev = key("G");
        assert_eq!(ev.code, KeyCode::Char('G'));
        assert!(ev.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn parse_ctrl_modifier() {
        let ev = key("ctrl-c");
        assert_eq!(ev.code, KeyCode::Char('c'));
        assert_eq!(ev.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn parse_alt_modifier() {
        let ev = key("alt-j");
        assert_eq!(ev.code, KeyCode::Char('j'));
        assert_eq!(ev.modifiers, KeyModifiers::ALT);
    }

    #[test]
    fn parse_shift_prefix() {
        let ev = key("shift-G");
        assert_eq!(ev.code, KeyCode::Char('G'));
        assert!(ev.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn parse_special_keys() {
        assert_eq!(key("enter").code, KeyCode::Enter);
        assert_eq!(key("esc").code, KeyCode::Esc);
        assert_eq!(key("tab").code, KeyCode::Tab);
        assert_eq!(key("space").code, KeyCode::Char(' '));
        assert_eq!(key("backspace").code, KeyCode::Backspace);
        assert_eq!(key("up").code, KeyCode::Up);
        assert_eq!(key("down").code, KeyCode::Down);
        assert_eq!(key("left").code, KeyCode::Left);
        assert_eq!(key("right").code, KeyCode::Right);
        assert_eq!(key("pageup").code, KeyCode::PageUp);
        assert_eq!(key("pagedown").code, KeyCode::PageDown);
    }

    #[test]
    fn parse_function_keys() {
        assert_eq!(key("f1").code, KeyCode::F(1));
        assert_eq!(key("f12").code, KeyCode::F(12));
    }

    #[test]
    fn parse_symbol_keys() {
        assert_eq!(key("/").code, KeyCode::Char('/'));
        assert_eq!(key("?").code, KeyCode::Char('?'));
    }

    #[test]
    fn parse_ctrl_d() {
        let ev = key("ctrl-d");
        assert_eq!(ev.code, KeyCode::Char('d'));
        assert!(ev.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn parse_unknown_key_returns_err() {
        assert!(parse_key("banana").is_err());
    }

    // ── UserEvent::from_str ───────────────────────────────────────────────

    #[test]
    fn user_event_from_str_known() {
        assert_eq!("quit".parse::<UserEvent>().unwrap(), UserEvent::Quit);
        assert_eq!(
            "force_quit".parse::<UserEvent>().unwrap(),
            UserEvent::ForceQuit
        );
        assert_eq!(
            "navigate_down".parse::<UserEvent>().unwrap(),
            UserEvent::NavigateDown
        );
        assert_eq!(
            "copy_full_hash".parse::<UserEvent>().unwrap(),
            UserEvent::CopyFullHash
        );
    }

    #[test]
    fn user_event_from_str_unknown() {
        assert!("banana_action".parse::<UserEvent>().is_err());
    }

    // ── KeyBind ───────────────────────────────────────────────────────────

    #[test]
    fn keybind_loads_defaults() {
        let kb = KeyBind::new(None).expect("default keybinds should load");

        // ctrl-c → ForceQuit
        let ctrl_c = key("ctrl-c");
        assert_eq!(kb.get(&ctrl_c), Some(&UserEvent::ForceQuit));

        // j → NavigateDown
        let j = key("j");
        assert_eq!(kb.get(&j), Some(&UserEvent::NavigateDown));

        // down → NavigateDown (multiple keys per action)
        let down = key("down");
        assert_eq!(kb.get(&down), Some(&UserEvent::NavigateDown));

        // q → Quit
        let q = key("q");
        assert_eq!(kb.get(&q), Some(&UserEvent::Quit));

        // enter → Confirm
        let enter = key("enter");
        assert_eq!(kb.get(&enter), Some(&UserEvent::Confirm));
    }

    #[test]
    fn keybind_override_replaces_default() {
        let override_toml: toml::Value =
            toml::from_str(r#"navigate_down = ["n", "down"]"#).unwrap();

        let kb = KeyBind::new(Some(&override_toml)).expect("overrides should load");

        // "j" should no longer map to NavigateDown
        let j = key("j");
        assert_ne!(kb.get(&j), Some(&UserEvent::NavigateDown));

        // "n" should now map to NavigateDown
        let n = key("n");
        assert_eq!(kb.get(&n), Some(&UserEvent::NavigateDown));
    }

    #[test]
    fn keybind_get_unmapped_key_returns_none() {
        let kb = KeyBind::new(None).unwrap();
        let f11 = key("f11");
        assert_eq!(kb.get(&f11), None);
    }

    #[test]
    fn user_event_with_count_defaults_to_one() {
        let ewc = UserEventWithCount::new(UserEvent::NavigateDown, None);
        assert_eq!(ewc.effective_count(), 1);
    }

    #[test]
    fn user_event_with_count_uses_prefix() {
        let ewc = UserEventWithCount::new(UserEvent::NavigateDown, Some(5));
        assert_eq!(ewc.effective_count(), 5);
    }
}
