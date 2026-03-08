// FileTreeBuilder and FileTreeItem for constructing hierarchical file trees from diffs.
use std::collections::HashSet;

use crate::git::diff::FileChange;

pub mod icons;

#[derive(Clone, Debug)]
pub struct FileTreeItem {
    pub name: String,
    pub full_path: String,
    pub is_directory: bool,
    pub depth: usize,
    pub file_change: Option<FileChange>,
    pub is_last_child: bool,
    pub parent_is_last: Vec<bool>,
    pub is_expanded: bool,
    pub dir_file_count: usize,
    pub dir_added_lines: usize,
    pub dir_removed_lines: usize,
}

#[derive(Clone, Debug)]
pub struct FileTreeState {
    pub selected_index: usize,
    pub collapsed_directories: HashSet<String>,
    pub offset: usize,
}

impl FileTreeState {
    pub fn new() -> Self {
        FileTreeState {
            selected_index: 0,
            collapsed_directories: HashSet::new(),
            offset: 0,
        }
    }

    pub fn move_up(&mut self, items: &[FileTreeItem]) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
        self.clamp(items);
    }

    pub fn move_down(&mut self, items: &[FileTreeItem]) {
        if self.selected_index + 1 < items.len() {
            self.selected_index += 1;
        }
        self.clamp(items);
    }

    pub fn go_to_top(&mut self, items: &[FileTreeItem]) {
        self.selected_index = 0;
        self.clamp(items);
    }

    pub fn go_to_bottom(&mut self, items: &[FileTreeItem]) {
        if !items.is_empty() {
            self.selected_index = items.len() - 1;
        }
        self.clamp(items);
    }

    /// Toggle expand/collapse on a directory item.
    pub fn toggle_expand(&mut self, items: &[FileTreeItem]) {
        if let Some(item) = items.get(self.selected_index) {
            if item.is_directory {
                if self.collapsed_directories.contains(&item.full_path) {
                    self.collapsed_directories.remove(&item.full_path);
                } else {
                    self.collapsed_directories.insert(item.full_path.clone());
                }
            }
        }
    }

    /// Collapse the selected directory (or collapse the parent if a file is selected).
    pub fn collapse(&mut self, items: &[FileTreeItem]) {
        if let Some(item) = items.get(self.selected_index) {
            if item.is_directory && !self.collapsed_directories.contains(&item.full_path) {
                self.collapsed_directories.insert(item.full_path.clone());
            }
        }
    }

    fn clamp(&mut self, items: &[FileTreeItem]) {
        if items.is_empty() {
            self.selected_index = 0;
        } else if self.selected_index >= items.len() {
            self.selected_index = items.len() - 1;
        }
    }
}

impl Default for FileTreeState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct TreeNode {
    name: String,
    full_path: String,
    is_directory: bool,
    file_change: Option<FileChange>,
    children: Vec<TreeNode>,
    file_count: usize,
    added_lines: usize,
    removed_lines: usize,
}

pub struct FileTreeBuilder;

impl FileTreeBuilder {
    /// Build a flat display list from a set of FileChanges (all directories expanded).
    pub fn build(changes: &[FileChange]) -> Vec<FileTreeItem> {
        Self::rebuild(changes, &HashSet::new())
    }

    /// Re-flatten the tree respecting a set of collapsed directory paths.
    pub fn rebuild(
        changes: &[FileChange],
        collapsed_dirs: &HashSet<String>,
    ) -> Vec<FileTreeItem> {
        let root = Self::build_tree_structure(changes);
        let mut result = Vec::new();
        Self::flatten_tree(&root, 0, &mut Vec::new(), &mut result, collapsed_dirs);
        result
    }

    fn file_change_path(fc: &FileChange) -> &str {
        match fc {
            FileChange::Add(p) | FileChange::Modify(p) | FileChange::Delete(p) => p.as_str(),
            FileChange::Move(_, new_path) => new_path.as_str(),
        }
    }

    fn build_tree_structure(changes: &[FileChange]) -> TreeNode {
        let mut root = TreeNode {
            name: String::new(),
            full_path: String::new(),
            is_directory: true,
            file_change: None,
            children: Vec::new(),
            file_count: 0,
            added_lines: 0,
            removed_lines: 0,
        };

        let mut sorted: Vec<&FileChange> = changes.iter().collect();
        sorted.sort_by(|a, b| {
            Self::file_change_path(a)
                .to_lowercase()
                .cmp(&Self::file_change_path(b).to_lowercase())
        });

        for fc in sorted {
            let path = Self::file_change_path(fc).to_string();
            Self::add_to_tree(&mut root, &path, Some(fc.clone()));
        }

        Self::sort_children(&mut root);
        Self::calc_stats(&mut root);

        root
    }

    fn add_to_tree(root: &mut TreeNode, path: &str, file_change: Option<FileChange>) {
        let parts: Vec<&str> = path.split('/').collect();
        let mut current = root;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                current.children.push(TreeNode {
                    name: part.to_string(),
                    full_path: path.to_string(),
                    is_directory: false,
                    file_change,
                    children: Vec::new(),
                    file_count: 1,
                    added_lines: 0,
                    removed_lines: 0,
                });
                return;
            }

            let dir_path = parts[..=i].join("/");
            let pos = current
                .children
                .iter()
                .position(|c| c.name == *part && c.is_directory);

            if let Some(idx) = pos {
                current = &mut current.children[idx];
            } else {
                current.children.push(TreeNode {
                    name: part.to_string(),
                    full_path: dir_path,
                    is_directory: true,
                    file_change: None,
                    children: Vec::new(),
                    file_count: 0,
                    added_lines: 0,
                    removed_lines: 0,
                });
                let last = current.children.len() - 1;
                current = &mut current.children[last];
            }
        }
    }

    fn sort_children(node: &mut TreeNode) {
        node.children
            .sort_by(|a, b| match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
        for child in &mut node.children {
            Self::sort_children(child);
        }
    }

    fn calc_stats(node: &mut TreeNode) -> (usize, usize, usize) {
        if !node.is_directory {
            return (node.file_count, node.added_lines, node.removed_lines);
        }

        let mut total_files = 0;
        let mut total_added = 0;
        let mut total_removed = 0;

        for child in &mut node.children {
            let (f, a, r) = Self::calc_stats(child);
            total_files += f;
            total_added += a;
            total_removed += r;
        }

        node.file_count = total_files;
        node.added_lines = total_added;
        node.removed_lines = total_removed;

        (total_files, total_added, total_removed)
    }

    fn flatten_tree(
        node: &TreeNode,
        depth: usize,
        parent_is_last: &mut Vec<bool>,
        result: &mut Vec<FileTreeItem>,
        collapsed_dirs: &HashSet<String>,
    ) {
        if depth > 0 {
            let is_last_child = parent_is_last.get(depth - 1).copied().unwrap_or(true);
            let is_expanded = !collapsed_dirs.contains(&node.full_path);

            result.push(FileTreeItem {
                name: node.name.clone(),
                full_path: node.full_path.clone(),
                is_directory: node.is_directory,
                depth: depth - 1,
                file_change: node.file_change.clone(),
                is_last_child,
                parent_is_last: parent_is_last[..depth.saturating_sub(1)].to_vec(),
                is_expanded,
                dir_file_count: node.file_count,
                dir_added_lines: node.added_lines,
                dir_removed_lines: node.removed_lines,
            });
        }

        let show_children = depth == 0 || !collapsed_dirs.contains(&node.full_path);
        if !show_children {
            return;
        }

        for (i, child) in node.children.iter().enumerate() {
            let is_last = i == node.children.len() - 1;

            if depth > 0 {
                if parent_is_last.len() <= depth {
                    parent_is_last.push(is_last);
                } else {
                    parent_is_last[depth] = is_last;
                }
            }

            Self::flatten_tree(child, depth + 1, parent_is_last, result, collapsed_dirs);
        }

        if depth > 0 {
            parent_is_last.truncate(depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changes() -> Vec<FileChange> {
        vec![
            FileChange::Add("src/main.rs".into()),
            FileChange::Modify("src/lib.rs".into()),
            FileChange::Add("README.md".into()),
            FileChange::Delete("src/old.rs".into()),
            FileChange::Move("src/a.rs".into(), "src/b.rs".into()),
        ]
    }

    #[test]
    fn build_produces_items() {
        let items = FileTreeBuilder::build(&changes());
        assert!(!items.is_empty());
    }

    #[test]
    fn directories_before_files() {
        let items = FileTreeBuilder::build(&changes());
        // "src" directory should appear before "README.md"
        let src_pos = items.iter().position(|i| i.name == "src").unwrap();
        let readme_pos = items.iter().position(|i| i.name == "README.md").unwrap();
        assert!(src_pos < readme_pos);
    }

    #[test]
    fn collapse_hides_children() {
        let items_full = FileTreeBuilder::build(&changes());
        let full_count = items_full.len();

        let mut collapsed = HashSet::new();
        collapsed.insert("src".to_string());
        let items_collapsed = FileTreeBuilder::rebuild(&changes(), &collapsed);
        assert!(items_collapsed.len() < full_count);
    }

    #[test]
    fn state_navigation() {
        let items = FileTreeBuilder::build(&changes());
        let mut state = FileTreeState::new();
        assert_eq!(state.selected_index, 0);

        state.move_down(&items);
        assert_eq!(state.selected_index, 1);

        state.move_up(&items);
        assert_eq!(state.selected_index, 0);

        state.go_to_bottom(&items);
        assert_eq!(state.selected_index, items.len() - 1);

        state.go_to_top(&items);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn move_shows_new_path() {
        let items = FileTreeBuilder::build(&changes());
        // The Move(src/a.rs, src/b.rs) should show as "b.rs" (new path)
        let b = items.iter().find(|i| i.name == "b.rs");
        assert!(b.is_some());
        let a = items.iter().find(|i| i.name == "a.rs");
        assert!(a.is_none());
    }
}
