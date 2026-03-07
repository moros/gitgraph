//! Color theme for all gitpeek UI elements.
//!
//! [`ColorTheme`] holds a `ThemeColor` field for every UI element in gitpeek.
//! All fields carry `#[serde(default)]` so a partial TOML `[color]` section
//! only overrides the values the user specifies; everything else falls back to
//! the built-in dark theme.
//!
//! [`GraphColorSet`] holds the branch-line color palette and graph background
//! used by the graph image renderer.

use crate::theme::ThemeColor;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

// ─── GraphColorSet ────────────────────────────────────────────────────────────

/// Color palette for the commit-graph image renderer.
///
/// Branch colors cycle through `branches` by x-position (lane index).
/// Defaults use the One Dark palette from serie.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphColorSet {
    /// Branch line colors, cycled by lane (x-position).
    pub branches: Vec<ThemeColor>,
    /// Edge/connector color. `transparent` means the terminal background shows through.
    pub edge: ThemeColor,
    /// Graph image background. `transparent` means the terminal background shows through.
    pub background: ThemeColor,
}

impl Default for GraphColorSet {
    fn default() -> Self {
        GraphColorSet {
            branches: vec![
                ThemeColor(Color::Rgb(0xE0, 0x6C, 0x75)), // #E06C75 — red
                ThemeColor(Color::Rgb(0xE5, 0xC0, 0x7B)), // #E5C07B — yellow
                ThemeColor(Color::Rgb(0x98, 0xC3, 0x79)), // #98C379 — green
                ThemeColor(Color::Rgb(0x56, 0xB6, 0xC2)), // #56B6C2 — teal
                ThemeColor(Color::Rgb(0x61, 0xAF, 0xEF)), // #61AFEF — blue
                ThemeColor(Color::Rgb(0xC6, 0x78, 0xDD)), // #C678DD — purple
            ],
            edge: ThemeColor(Color::Reset),
            background: ThemeColor(Color::Reset),
        }
    }
}

// ─── ColorTheme ──────────────────────────────────────────────────────────────

/// Complete color theme for all gitpeek UI elements.
///
/// Deserialized from the `[color]` section of the gitpeek config file.
/// Every field has a default so a partial config merges cleanly with the
/// built-in dark theme.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ColorTheme {
    // ── List View ─────────────────────────────────────────────────────────
    /// Commit subject text (unselected row).
    pub list_subject: ThemeColor,
    /// Date column text.
    pub list_date: ThemeColor,
    /// Hash column text.
    pub list_hash: ThemeColor,
    /// Author name column text.
    pub list_author: ThemeColor,

    /// HEAD commit indicator foreground.
    pub list_head: ThemeColor,
    /// HEAD commit indicator background (usually Reset).
    pub list_head_bg: ThemeColor,

    /// Tag label foreground.
    pub list_tag: ThemeColor,
    /// Tag label background.
    pub list_tag_bg: ThemeColor,

    /// Local branch label foreground.
    pub list_branch_local: ThemeColor,
    /// Local branch label background.
    pub list_branch_local_bg: ThemeColor,

    /// Remote branch label foreground.
    pub list_branch_remote: ThemeColor,
    /// Remote branch label background.
    pub list_branch_remote_bg: ThemeColor,

    /// Stash label foreground.
    pub list_stash: ThemeColor,
    /// Stash label background.
    pub list_stash_bg: ThemeColor,

    /// Background of the selected commit row.
    pub list_selected_bg: ThemeColor,

    /// HEAD marker character (●) color.
    pub list_marker: ThemeColor,

    // ── Detail View ───────────────────────────────────────────────────────
    /// Metadata label foreground (e.g. "Author:", "Date:").
    pub detail_label: ThemeColor,
    /// Metadata value foreground.
    pub detail_value: ThemeColor,
    /// Commit hash in the detail header.
    pub detail_hash: ThemeColor,
    /// Commit message body text.
    pub detail_message: ThemeColor,
    /// Separator line between sections.
    pub detail_separator: ThemeColor,
    /// Background of the selected row in the detail commit list.
    pub detail_selected_bg: ThemeColor,

    // ── Refs View ─────────────────────────────────────────────────────────
    /// HEAD indicator in the refs tree.
    pub refs_head: ThemeColor,
    /// Local branch in the refs tree.
    pub refs_branch: ThemeColor,
    /// Remote branch in the refs tree.
    pub refs_remote: ThemeColor,
    /// Tag in the refs tree.
    pub refs_tag: ThemeColor,
    /// Stash in the refs tree.
    pub refs_stash: ThemeColor,
    /// Background of the selected ref.
    pub refs_selected_bg: ThemeColor,

    // ── Help View ─────────────────────────────────────────────────────────
    /// Key binding display (e.g. "j", "ctrl-d").
    pub help_key: ThemeColor,
    /// Action description next to the key.
    pub help_description: ThemeColor,
    /// Section header (e.g. "List", "Detail").
    pub help_section: ThemeColor,
    /// Separator between key and description.
    pub help_separator: ThemeColor,

    // ── Status Bar ────────────────────────────────────────────────────────
    /// Normal status bar foreground.
    pub status_fg: ThemeColor,
    /// Status bar background.
    pub status_bg: ThemeColor,
    /// Search mode status bar foreground.
    pub status_search_fg: ThemeColor,
    /// Search mode status bar background.
    pub status_search_bg: ThemeColor,
    /// Error message foreground in the status bar.
    pub status_error_fg: ThemeColor,
    /// Error message background in the status bar.
    pub status_error_bg: ThemeColor,

    // ── Common ────────────────────────────────────────────────────────────
    /// Search match highlight foreground.
    pub search_match: ThemeColor,
    /// Search match highlight background.
    pub search_match_bg: ThemeColor,
    /// Virtual cursor foreground (when cursor_type = virtual).
    pub cursor: ThemeColor,
    /// Widget border foreground.
    pub border: ThemeColor,

    // ── File Tree (from ftdv) ─────────────────────────────────────────────
    /// Directory name foreground.
    pub tree_directory: ThemeColor,
    /// File name foreground.
    pub tree_file: ThemeColor,
    /// Background of the selected tree row.
    pub tree_selected_bg: ThemeColor,
    /// Tree connector characters (│, ├──, ╰──).
    pub tree_connector: ThemeColor,
    /// Nerd Font file/dir icon color.
    pub tree_icon: ThemeColor,

    // ── Diff Viewer (from ftdv) ───────────────────────────────────────────
    /// Added line foreground (`+` lines).
    pub diff_added: ThemeColor,
    /// Removed line foreground (`-` lines).
    pub diff_removed: ThemeColor,
    /// Hunk header foreground (`@@ … @@`).
    pub diff_hunk_header: ThemeColor,
    /// Context line foreground (unchanged lines).
    pub diff_context: ThemeColor,
    /// File header foreground (`diff --git`, `---`, `+++`).
    pub diff_file_header: ThemeColor,
    /// Added-lines stat foreground (`+N` in tree and status line).
    pub diff_stats_added: ThemeColor,
    /// Removed-lines stat foreground (`-M` in tree and status line).
    pub diff_stats_removed: ThemeColor,
}

impl Default for ColorTheme {
    fn default() -> Self {
        ColorTheme {
            // List View
            list_subject: ThemeColor(Color::White),
            list_date: ThemeColor(Color::Indexed(243)),
            list_hash: ThemeColor(Color::Indexed(243)),
            list_author: ThemeColor(Color::Cyan),
            list_head: ThemeColor(Color::Red),
            list_head_bg: ThemeColor(Color::Reset),
            list_tag: ThemeColor(Color::Yellow),
            list_tag_bg: ThemeColor(Color::Reset),
            list_branch_local: ThemeColor(Color::Green),
            list_branch_local_bg: ThemeColor(Color::Reset),
            list_branch_remote: ThemeColor(Color::Red),
            list_branch_remote_bg: ThemeColor(Color::Reset),
            list_stash: ThemeColor(Color::Magenta),
            list_stash_bg: ThemeColor(Color::Reset),
            list_selected_bg: ThemeColor(Color::Rgb(0x3E, 0x44, 0x52)),
            list_marker: ThemeColor(Color::Yellow),

            // Detail View
            detail_label: ThemeColor(Color::Indexed(243)),
            detail_value: ThemeColor(Color::White),
            detail_hash: ThemeColor(Color::Yellow),
            detail_message: ThemeColor(Color::White),
            detail_separator: ThemeColor(Color::Indexed(238)),
            detail_selected_bg: ThemeColor(Color::Rgb(0x3E, 0x44, 0x52)),

            // Refs View
            refs_head: ThemeColor(Color::Red),
            refs_branch: ThemeColor(Color::Green),
            refs_remote: ThemeColor(Color::Red),
            refs_tag: ThemeColor(Color::Yellow),
            refs_stash: ThemeColor(Color::Magenta),
            refs_selected_bg: ThemeColor(Color::Rgb(0x3E, 0x44, 0x52)),

            // Help View
            help_key: ThemeColor(Color::Yellow),
            help_description: ThemeColor(Color::White),
            help_section: ThemeColor(Color::Cyan),
            help_separator: ThemeColor(Color::Indexed(243)),

            // Status Bar
            status_fg: ThemeColor(Color::White),
            status_bg: ThemeColor(Color::Indexed(236)),
            status_search_fg: ThemeColor(Color::Black),
            status_search_bg: ThemeColor(Color::Yellow),
            status_error_fg: ThemeColor(Color::White),
            status_error_bg: ThemeColor(Color::Red),

            // Common
            search_match: ThemeColor(Color::Yellow),
            search_match_bg: ThemeColor(Color::Reset),
            cursor: ThemeColor(Color::White),
            border: ThemeColor(Color::Indexed(243)),

            // File Tree
            tree_directory: ThemeColor(Color::Blue),
            tree_file: ThemeColor(Color::White),
            tree_selected_bg: ThemeColor(Color::Rgb(0x32, 0x32, 0x48)),
            tree_connector: ThemeColor(Color::Indexed(243)),
            tree_icon: ThemeColor(Color::Indexed(243)),

            // Diff Viewer
            diff_added: ThemeColor(Color::Green),
            diff_removed: ThemeColor(Color::Red),
            diff_hunk_header: ThemeColor(Color::Cyan),
            diff_context: ThemeColor(Color::White),
            diff_file_header: ThemeColor(Color::Indexed(243)),
            diff_stats_added: ThemeColor(Color::Green),
            diff_stats_removed: ThemeColor(Color::Red),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_color_set_default_has_six_branch_colors() {
        let g = GraphColorSet::default();
        assert_eq!(g.branches.len(), 6);
    }

    #[test]
    fn graph_color_set_default_edge_and_bg_are_reset() {
        let g = GraphColorSet::default();
        assert_eq!(g.edge, ThemeColor(Color::Reset));
        assert_eq!(g.background, ThemeColor(Color::Reset));
    }

    #[test]
    fn graph_color_set_branch_colors_match_one_dark() {
        let g = GraphColorSet::default();
        assert_eq!(g.branches[0], ThemeColor(Color::Rgb(0xE0, 0x6C, 0x75)));
        assert_eq!(g.branches[1], ThemeColor(Color::Rgb(0xE5, 0xC0, 0x7B)));
        assert_eq!(g.branches[2], ThemeColor(Color::Rgb(0x98, 0xC3, 0x79)));
        assert_eq!(g.branches[3], ThemeColor(Color::Rgb(0x56, 0xB6, 0xC2)));
        assert_eq!(g.branches[4], ThemeColor(Color::Rgb(0x61, 0xAF, 0xEF)));
        assert_eq!(g.branches[5], ThemeColor(Color::Rgb(0xC6, 0x78, 0xDD)));
    }

    #[test]
    fn color_theme_default_smoke() {
        // Ensure Default does not panic and returns sensible values.
        let t = ColorTheme::default();
        assert_eq!(t.list_subject, ThemeColor(Color::White));
        assert_eq!(t.diff_added, ThemeColor(Color::Green));
        assert_eq!(t.diff_removed, ThemeColor(Color::Red));
        assert_eq!(t.tree_directory, ThemeColor(Color::Blue));
        assert_eq!(t.tree_file, ThemeColor(Color::White));
        assert_eq!(t.list_selected_bg, ThemeColor(Color::Rgb(0x3E, 0x44, 0x52)));
        assert_eq!(t.tree_selected_bg, ThemeColor(Color::Rgb(0x32, 0x32, 0x48)));
    }

    #[test]
    fn color_theme_serde_partial_override() {
        // A partial [color] section should keep defaults for unspecified fields.
        let toml = r#"
            diff_added = "lightgreen"
            diff_removed = "#ff0000"
        "#;
        let t: ColorTheme = toml::from_str(toml).expect("partial TOML parse failed");

        assert_eq!(t.diff_added, ThemeColor(Color::LightGreen));
        assert_eq!(t.diff_removed, ThemeColor(Color::Rgb(0xff, 0x00, 0x00)));
        // Unspecified field should still have the default
        assert_eq!(t.list_subject, ColorTheme::default().list_subject);
        assert_eq!(t.tree_directory, ColorTheme::default().tree_directory);
    }

    #[test]
    fn color_theme_serde_roundtrip() {
        let original = ColorTheme::default();
        let serialized = toml::to_string(&original).expect("serialization failed");
        let parsed: ColorTheme = toml::from_str(&serialized).expect("deserialization failed");

        // Spot-check a few fields
        assert_eq!(parsed.list_subject, original.list_subject);
        assert_eq!(parsed.diff_hunk_header, original.diff_hunk_header);
        assert_eq!(parsed.tree_selected_bg, original.tree_selected_bg);
        assert_eq!(parsed.status_bg, original.status_bg);
    }

    #[test]
    fn graph_color_set_serde_roundtrip() {
        let original = GraphColorSet::default();
        let serialized = toml::to_string(&original).expect("serialization failed");
        let parsed: GraphColorSet = toml::from_str(&serialized).expect("deserialization failed");

        assert_eq!(parsed.branches.len(), original.branches.len());
        for (a, b) in parsed.branches.iter().zip(original.branches.iter()) {
            assert_eq!(a, b);
        }
        assert_eq!(parsed.edge, original.edge);
        assert_eq!(parsed.background, original.background);
    }
}
