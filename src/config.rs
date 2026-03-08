//! Configuration loading for gitpeek.
//!
//! Config file is searched in order:
//! 1. `$GITPEEK_CONFIG_FILE` env var
//! 2. `$XDG_CONFIG_HOME/gitpeek/config.toml`  (dirs::config_dir)
//! 3. `~/.config/gitpeek/config.toml`
//!
//! All fields use `#[serde(default)]` so any or all sections may be omitted.
//! Missing file → use defaults.

use crate::color::{ColorTheme, GraphColorSet};
use crate::keybind::KeyBind;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Fully-resolved gitpeek configuration (after merging file + defaults).
#[derive(Debug, Clone)]
pub struct Config {
    pub core: CoreConfig,
    pub ui: UiConfig,
    pub graph: GraphConfig,
    /// Color theme (48+ color fields).  Resolved from `[color]` section.
    pub color: ColorTheme,
    /// Keybinding map, built from `[keybind]` section merged with defaults.
    pub keybind: KeyBind,
}

/// Raw TOML representation for deserialization.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
struct RawConfig {
    pub core: CoreConfig,
    pub ui: UiConfig,
    pub graph: GraphConfig,
    pub color: ColorTheme,
    // keybind is extracted separately as a raw toml::Value before deserialization
}

impl Config {
    /// Load configuration from file (if present) or use defaults throughout.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();

        let (raw, keybind_value) = match path {
            Some(ref p) if p.exists() => {
                let content = std::fs::read_to_string(p)
                    .with_context(|| format!("Failed to read config file: {}", p.display()))?;

                // Parse once as generic toml::Value to extract the keybind table
                let table: toml::Value = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse TOML: {}", p.display()))?;

                let keybind_value = table.get("keybind").cloned();

                // Deserialize the full config (keybind section is ignored by RawConfig)
                let raw: RawConfig = toml::from_str(&content)
                    .with_context(|| format!("Failed to deserialize config: {}", p.display()))?;

                (raw, keybind_value)
            }
            _ => (RawConfig::default(), None),
        };

        let keybind = KeyBind::new(keybind_value.as_ref())
            .map_err(|e| anyhow::anyhow!("Keybind configuration error: {e}"))?;

        Ok(Config {
            core: raw.core,
            ui: raw.ui,
            graph: raw.graph,
            color: raw.color,
            keybind,
        })
    }

    /// Resolve the config file path using the documented lookup order.
    pub fn config_path() -> Option<PathBuf> {
        // 1. Explicit env var
        if let Ok(p) = std::env::var("GITPEEK_CONFIG_FILE") {
            return Some(PathBuf::from(p));
        }
        // 2. XDG config dir (covers both $XDG_CONFIG_HOME and the OS default)
        if let Some(dir) = dirs::config_dir() {
            return Some(dir.join("gitpeek").join("config.toml"));
        }
        // 3. Explicit ~/.config fallback (in case dirs returns None)
        if let Some(home) = dirs::home_dir() {
            return Some(home.join(".config").join("gitpeek").join("config.toml"));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// CoreConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct CoreConfig {
    /// Git log ordering strategy.
    pub order: OrderType,
    /// Graph cell width (auto-detects from terminal width).
    pub graph_width: GraphWidth,
    /// Graph line rendering style.
    pub graph_style: GraphStyle,
    /// Which commit to select on startup.
    pub initial_selection: InitialSelection,
    /// Terminal image protocol for graph rendering.
    pub protocol: Protocol,
    pub search: SearchConfig,
    pub external: ExternalConfig,
    pub diff: DiffConfig,
    pub user_command: UserCommandConfig,
}

// ── Enums ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    #[default]
    Chrono,
    Topo,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GraphWidth {
    #[default]
    Auto,
    Double,
    Single,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GraphStyle {
    #[default]
    Rounded,
    Angular,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InitialSelection {
    #[default]
    Latest,
    Head,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Auto,
    Iterm,
    Kitty,
}

// ── Search ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct SearchConfig {
    pub ignore_case: bool,
    pub fuzzy: bool,
}

// ── External ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ExternalConfig {
    /// Clipboard backend: `"auto"` or a custom command list.
    pub clipboard: String,
}

impl Default for ExternalConfig {
    fn default() -> Self {
        Self {
            clipboard: "auto".to_string(),
        }
    }
}

// ── Diff ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DiffConfig {
    /// External pager command (e.g. `"delta --side-by-side"`).  Empty = none.
    pub pager: String,
    /// External diff command (e.g. `"difft --color=always"`).  Empty = none.
    pub external_command: String,
    /// `--color` argument passed to git diff when using external tools.
    pub color_arg: String,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            pager: String::new(),
            external_command: String::new(),
            color_arg: "always".to_string(),
        }
    }
}

// ── User commands ──────────────────────────────────────────────────────────

/// The rendering/execution mode for a user command.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserCommandKind {
    /// Capture stdout and display inline in the TUI.
    #[default]
    Inline,
    /// Run in background, don't capture output.
    Silent,
}

/// A single user-defined command (configured as `commands_N` in TOML).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserCommandDef {
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: UserCommandKind,
    /// Command + args.  Template markers: `first_parent_hash`, `target_hash`.
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct UserCommandConfig {
    /// Tab stop width for command output display.
    pub tab_width: usize,
    pub commands_1: Option<UserCommandDef>,
    pub commands_2: Option<UserCommandDef>,
    pub commands_3: Option<UserCommandDef>,
    pub commands_4: Option<UserCommandDef>,
    pub commands_5: Option<UserCommandDef>,
    pub commands_6: Option<UserCommandDef>,
    pub commands_7: Option<UserCommandDef>,
    pub commands_8: Option<UserCommandDef>,
    pub commands_9: Option<UserCommandDef>,
}

impl UserCommandConfig {
    /// Return all configured commands in slot order (1-indexed, skipping None).
    pub fn commands(&self) -> Vec<(u8, &UserCommandDef)> {
        [
            (1, &self.commands_1),
            (2, &self.commands_2),
            (3, &self.commands_3),
            (4, &self.commands_4),
            (5, &self.commands_5),
            (6, &self.commands_6),
            (7, &self.commands_7),
            (8, &self.commands_8),
            (9, &self.commands_9),
        ]
        .iter()
        .filter_map(|(n, opt)| opt.as_ref().map(|def| (*n, def)))
        .collect()
    }
}

impl Default for UserCommandConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            commands_1: Some(UserCommandDef {
                name: "Diff".to_string(),
                kind: UserCommandKind::Inline,
                commands: vec![
                    "git".to_string(),
                    "--no-pager".to_string(),
                    "diff".to_string(),
                    "--color=always".to_string(),
                    "{{first_parent_hash}}".to_string(),
                    "{{target_hash}}".to_string(),
                ],
            }),
            commands_2: None,
            commands_3: None,
            commands_4: None,
            commands_5: None,
            commands_6: None,
            commands_7: None,
            commands_8: None,
            commands_9: None,
        }
    }
}

// ---------------------------------------------------------------------------
// UiConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct UiConfig {
    pub list: ListUiConfig,
    pub detail: DetailUiConfig,
    pub refs: RefsUiConfig,
    pub common: CommonUiConfig,
}

// ── List view ──────────────────────────────────────────────────────────────

/// Valid column names for `ui.list.columns`.
pub const VALID_COLUMNS: &[&str] = &["Graph", "Marker", "Subject", "Name", "Hash", "Date"];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ListUiConfig {
    /// Ordered list of columns to display.
    pub columns: Vec<String>,
    /// Minimum width (chars) of the Subject column.
    pub subject_min_width: usize,
    /// `chrono` date format string for the Date column.
    pub date_format: String,
    /// Fixed width (chars) of the Date column.
    pub date_width: usize,
    /// Display dates in local timezone.
    pub date_local: bool,
    /// Fixed width (chars) of the Name column.
    pub name_width: usize,
}

impl Default for ListUiConfig {
    fn default() -> Self {
        Self {
            columns: VALID_COLUMNS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            subject_min_width: 20,
            date_format: "%Y-%m-%d".to_string(),
            date_width: 10,
            date_local: true,
            name_width: 20,
        }
    }
}

// ── Detail view ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DetailUiConfig {
    /// Number of terminal rows used for the commit metadata section.
    pub metadata_height: usize,
    /// `chrono` date format for the metadata panel.
    pub date_format: String,
    /// Display dates in local timezone.
    pub date_local: bool,
    /// Width of the file-tree panel as a percentage of the diff-explorer area.
    pub tree_width_percent: usize,
}

impl Default for DetailUiConfig {
    fn default() -> Self {
        Self {
            metadata_height: 8,
            date_format: "%Y-%m-%d %H:%M:%S".to_string(),
            date_local: true,
            tree_width_percent: 20,
        }
    }
}

// ── Refs view ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct RefsUiConfig {
    /// Width (chars) of the refs side panel.
    pub width: usize,
}

impl Default for RefsUiConfig {
    fn default() -> Self {
        Self { width: 26 }
    }
}

// ── Common ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CommonUiConfig {
    /// `"native"` uses the terminal cursor; `"virtual"` renders a `>` character.
    pub cursor_type: String,
}

impl Default for CommonUiConfig {
    fn default() -> Self {
        Self {
            cursor_type: "native".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// GraphConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct GraphConfig {
    pub color: GraphColorSet,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_core_config() {
        let c = CoreConfig::default();
        assert_eq!(c.order, OrderType::Chrono);
        assert_eq!(c.graph_width, GraphWidth::Auto);
        assert_eq!(c.graph_style, GraphStyle::Rounded);
        assert_eq!(c.initial_selection, InitialSelection::Latest);
        assert_eq!(c.protocol, Protocol::Auto);
        assert!(!c.search.ignore_case);
        assert!(!c.search.fuzzy);
        assert_eq!(c.external.clipboard, "auto");
        assert_eq!(c.diff.color_arg, "always");
        assert_eq!(c.user_command.tab_width, 4);
    }

    #[test]
    fn default_ui_list_config() {
        let c = ListUiConfig::default();
        assert_eq!(
            c.columns,
            ["Graph", "Marker", "Subject", "Name", "Hash", "Date"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(c.subject_min_width, 20);
        assert_eq!(c.date_format, "%Y-%m-%d");
        assert_eq!(c.date_width, 10);
        assert!(c.date_local);
        assert_eq!(c.name_width, 20);
    }

    #[test]
    fn default_ui_detail_config() {
        let c = DetailUiConfig::default();
        assert_eq!(c.metadata_height, 8);
        assert_eq!(c.date_format, "%Y-%m-%d %H:%M:%S");
        assert!(c.date_local);
        assert_eq!(c.tree_width_percent, 20);
    }

    #[test]
    fn default_refs_config() {
        assert_eq!(RefsUiConfig::default().width, 26);
    }

    #[test]
    fn default_user_command_has_diff() {
        let c = UserCommandConfig::default();
        assert_eq!(c.tab_width, 4);
        let cmds = c.commands();
        assert_eq!(cmds.len(), 1);
        let (slot, def) = cmds[0];
        assert_eq!(slot, 1);
        assert_eq!(def.name, "Diff");
        assert_eq!(def.kind, UserCommandKind::Inline);
        assert!(def.commands.contains(&"git".to_string()));
    }

    #[test]
    fn config_path_respects_env_var() {
        std::env::set_var("GITPEEK_CONFIG_FILE", "/tmp/test-gitpeek.toml");
        let path = Config::config_path();
        std::env::remove_var("GITPEEK_CONFIG_FILE");
        assert_eq!(path, Some(PathBuf::from("/tmp/test-gitpeek.toml")));
    }

    #[test]
    fn parse_partial_toml_uses_defaults() {
        // Only override one field; everything else should fall back to defaults
        let partial = r#"
[core]
order = "topo"
"#;
        let raw: RawConfig = toml::from_str(partial).expect("partial TOML should deserialize");
        assert_eq!(raw.core.order, OrderType::Topo);
        // Other fields should be defaults
        assert_eq!(raw.core.graph_width, GraphWidth::Auto);
        assert_eq!(raw.ui.list.subject_min_width, 20);
        assert_eq!(raw.ui.detail.tree_width_percent, 20);
    }

    #[test]
    fn parse_full_core_section() {
        let toml_str = r#"
[core]
order = "topo"
graph_width = "double"
graph_style = "angular"
initial_selection = "head"
protocol = "kitty"

[core.search]
ignore_case = true
fuzzy = true

[core.diff]
pager = "delta"
external_command = "difft"
color_arg = "never"

[core.user_command]
tab_width = 2
"#;
        let raw: RawConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(raw.core.order, OrderType::Topo);
        assert_eq!(raw.core.graph_width, GraphWidth::Double);
        assert_eq!(raw.core.graph_style, GraphStyle::Angular);
        assert_eq!(raw.core.initial_selection, InitialSelection::Head);
        assert_eq!(raw.core.protocol, Protocol::Kitty);
        assert!(raw.core.search.ignore_case);
        assert!(raw.core.search.fuzzy);
        assert_eq!(raw.core.diff.pager, "delta");
        assert_eq!(raw.core.diff.external_command, "difft");
        assert_eq!(raw.core.diff.color_arg, "never");
        assert_eq!(raw.core.user_command.tab_width, 2);
    }

    #[test]
    fn parse_ui_sections() {
        let toml_str = r#"
[ui.list]
columns = ["Graph", "Subject", "Date"]
subject_min_width = 30
date_format = "%d/%m/%Y"
date_width = 12
date_local = false
name_width = 25

[ui.detail]
metadata_height = 10
date_format = "%Y-%m-%d"
date_local = false
tree_width_percent = 30

[ui.refs]
width = 32

[ui.common]
cursor_type = "virtual"
"#;
        let raw: RawConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(raw.ui.list.columns, ["Graph", "Subject", "Date"]);
        assert_eq!(raw.ui.list.subject_min_width, 30);
        assert_eq!(raw.ui.list.date_format, "%d/%m/%Y");
        assert!(!raw.ui.list.date_local);
        assert_eq!(raw.ui.detail.metadata_height, 10);
        assert_eq!(raw.ui.detail.tree_width_percent, 30);
        assert_eq!(raw.ui.refs.width, 32);
        assert_eq!(raw.ui.common.cursor_type, "virtual");
    }
}
