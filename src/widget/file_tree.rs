//! FileTree widget — scrollable file tree for the diff explorer left panel.

use crate::config::Config;
use crate::git::diff::FileChange;
use crate::tree::FileTreeNode;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{StatefulWidget, Widget},
};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// FileTreeState
// ---------------------------------------------------------------------------

pub struct FileTreeState {
    tree: Vec<FileTreeNode>,
    selected: usize,
    offset: usize,
    height: usize,
}

impl FileTreeState {
    pub fn new(file_changes: Vec<FileChange>) -> Self {
        Self {
            tree: FileTreeNode::build(&file_changes),
            selected: 0,
            offset: 0,
            height: 1,
        }
    }

    fn flat_len(&self) -> usize {
        FileTreeNode::flatten(&self.tree).len()
    }

    pub fn select_next(&mut self) {
        let len = self.flat_len();
        if len == 0 {
            return;
        }
        if self.selected + 1 < len {
            self.selected += 1;
            self.scroll_to_selected();
        }
    }

    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.scroll_to_selected();
        }
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }

    pub fn select_last(&mut self) {
        let len = self.flat_len();
        if len > 0 {
            self.selected = len - 1;
            self.scroll_to_selected();
        }
    }

    /// Toggle dir expand/collapse; no-op for files.
    pub fn toggle_selected(&mut self) {
        self.mutate_selected(&mut |node| {
            if node.is_dir {
                node.expanded = !node.expanded;
            }
        });
    }

    /// Collapse dir if selected is a dir; if file, move to parent dir.
    pub fn collapse_selected(&mut self) {
        let flat = FileTreeNode::flatten(&self.tree);
        if flat.is_empty() || self.selected >= flat.len() {
            return;
        }
        let (node, depth) = &flat[self.selected];
        if node.is_dir {
            // collapse in place via full-path lookup
            let full_path = node.full_path.clone();
            collapse_node_by_path(&mut self.tree, &full_path);
        } else if *depth > 0 {
            // find parent: scan backwards for a dir at depth-1
            let target_depth = depth - 1;
            for i in (0..self.selected).rev() {
                if flat[i].1 == target_depth && flat[i].0.is_dir {
                    self.selected = i;
                    self.scroll_to_selected();
                    break;
                }
            }
        }
    }

    /// Returns full_path of selected file (not dir), or None.
    pub fn selected_path(&self) -> Option<String> {
        let flat = FileTreeNode::flatten(&self.tree);
        flat.get(self.selected).and_then(|(node, _)| {
            if node.is_dir {
                None
            } else {
                Some(node.full_path.clone())
            }
        })
    }

    fn scroll_to_selected(&mut self) {
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + self.height {
            self.offset = self.selected + 1 - self.height;
        }
    }

    fn mutate_selected(&mut self, f: &mut dyn FnMut(&mut FileTreeNode)) {
        let flat = FileTreeNode::flatten(&self.tree);
        if self.selected >= flat.len() {
            return;
        }
        let full_path = flat[self.selected].0.full_path.clone();
        mutate_node_by_path(&mut self.tree, &full_path, f);
    }
}

fn mutate_node_by_path(
    nodes: &mut [FileTreeNode],
    full_path: &str,
    f: &mut dyn FnMut(&mut FileTreeNode),
) -> bool {
    for node in nodes.iter_mut() {
        if node.full_path == full_path {
            f(node);
            return true;
        }
        if node.is_dir && mutate_node_by_path(&mut node.children, full_path, f) {
            return true;
        }
    }
    false
}

fn collapse_node_by_path(nodes: &mut [FileTreeNode], full_path: &str) -> bool {
    for node in nodes.iter_mut() {
        if node.full_path == full_path && node.is_dir {
            node.expanded = false;
            return true;
        }
        if node.is_dir && collapse_node_by_path(&mut node.children, full_path) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// FileTree widget
// ---------------------------------------------------------------------------

pub struct FileTree {
    config: Rc<Config>,
}

impl FileTree {
    pub fn new(config: Rc<Config>) -> Self {
        Self { config }
    }
}

impl StatefulWidget for FileTree {
    type State = FileTreeState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        state.height = area.height as usize;

        let theme = &self.config.color;
        let connector_style = Style::default().fg(theme.tree_connector.0);
        let dir_style = Style::default().fg(theme.tree_directory.0).add_modifier(Modifier::BOLD);
        let file_style = Style::default().fg(theme.tree_file.0);
        let selected_bg = theme.tree_selected_bg.0;

        let flat = FileTreeNode::flatten(&state.tree);
        let visible = flat.iter().skip(state.offset).take(area.height as usize);

        for (row_idx, (node, depth)) in visible.enumerate() {
            let y = area.y + row_idx as u16;
            let is_selected = state.offset + row_idx == state.selected;

            let row_area = Rect { y, height: 1, ..area };

            // Indentation + connector
            let indent = "  ".repeat(*depth);
            let connector = if node.is_dir {
                if node.expanded { "▼ " } else { "▶ " }
            } else {
                "  "
            };

            // Change indicator
            let (indicator, ind_color) = match &node.change {
                Some(FileChange::Add(_)) => ("[A]", theme.diff_stats_added.0),
                Some(FileChange::Modify(_)) => ("[M]", ratatui::style::Color::Yellow),
                Some(FileChange::Delete(_)) => ("[D]", theme.diff_stats_removed.0),
                Some(FileChange::Move(_, _)) => ("[R]", ratatui::style::Color::Cyan),
                None => ("", ratatui::style::Color::Reset),
            };

            let name_style = if node.is_dir { dir_style } else { file_style };

            let mut spans: Vec<Span> = vec![
                Span::styled(indent, connector_style),
                Span::styled(connector, connector_style),
                Span::styled(node.name.as_str(), name_style),
            ];

            if !indicator.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(indicator, Style::default().fg(ind_color)));
            }

            let mut line = Line::from(spans);
            if is_selected {
                line = line.bg(selected_bg);
            }

            line.render(row_area, buf);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::FileChange;

    fn changes(paths: &[&str]) -> Vec<FileChange> {
        paths.iter().map(|p| FileChange::Modify(p.to_string())).collect()
    }

    #[test]
    fn select_next_wraps_at_end() {
        let mut state = FileTreeState::new(changes(&["a.rs", "b.rs"]));
        state.select_next();
        assert_eq!(state.selected, 1);
        state.select_next(); // already at end
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn select_prev_stops_at_zero() {
        let mut state = FileTreeState::new(changes(&["a.rs"]));
        state.select_prev();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn toggle_expands_and_collapses_dir() {
        let mut state = FileTreeState::new(changes(&["src/a.rs", "src/b.rs"]));
        // src dir is first (selected=0)
        assert_eq!(state.selected, 0);
        let expanded_before = FileTreeNode::flatten(&state.tree)[0].0.expanded;
        assert!(expanded_before);

        state.toggle_selected();
        let expanded_after = FileTreeNode::flatten(&state.tree)[0].0.expanded;
        assert!(!expanded_after);

        state.toggle_selected();
        let expanded_back = FileTreeNode::flatten(&state.tree)[0].0.expanded;
        assert!(expanded_back);
    }

    #[test]
    fn selected_path_returns_file_path() {
        let mut state = FileTreeState::new(changes(&["src/main.rs"]));
        // flat: src/(0), src/main.rs(1)
        state.select_next();
        assert_eq!(state.selected_path(), Some("src/main.rs".to_string()));
    }

    #[test]
    fn selected_path_none_for_dir() {
        let mut state = FileTreeState::new(changes(&["src/main.rs"]));
        assert_eq!(state.selected, 0); // on dir
        assert_eq!(state.selected_path(), None);
    }
}
