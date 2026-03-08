// FileTreeBuilder and FileTreeItem for constructing hierarchical file trees from diffs.

pub mod icons;

use crate::git::diff::FileChange;

/// Extract the path string from any FileChange variant.
pub fn file_change_path(change: &FileChange) -> &str {
    match change {
        FileChange::Add(p) => p,
        FileChange::Modify(p) => p,
        FileChange::Delete(p) => p,
        FileChange::Move(_, to) => to,
    }
}

/// A node in the hierarchical file tree.
#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub full_path: String,
    pub is_dir: bool,
    pub change: Option<FileChange>,
    pub children: Vec<FileTreeNode>,
    pub expanded: bool,
}

impl FileTreeNode {
    /// Build a hierarchical tree from a flat list of file changes.
    /// Dirs first (alpha), then files (alpha). All dirs start expanded.
    pub fn build(changes: &[FileChange]) -> Vec<FileTreeNode> {
        let mut roots: Vec<FileTreeNode> = Vec::new();
        for change in changes {
            let path = file_change_path(change);
            let components: Vec<&str> = path.split('/').collect();
            insert_node(&mut roots, &components, 0, path, change);
        }
        sort_nodes(&mut roots);
        roots
    }

    /// Flatten the tree respecting expanded/collapsed state.
    /// Returns (node_ref, depth) pairs in display order.
    pub fn flatten(nodes: &[FileTreeNode]) -> Vec<(&FileTreeNode, usize)> {
        let mut result = Vec::new();
        flatten_recursive(nodes, 0, &mut result);
        result
    }
}

fn insert_node(
    nodes: &mut Vec<FileTreeNode>,
    components: &[&str],
    depth: usize,
    full_path: &str,
    change: &FileChange,
) {
    if components.len() == 1 {
        // Leaf file node
        nodes.push(FileTreeNode {
            name: components[0].to_string(),
            full_path: full_path.to_string(),
            is_dir: false,
            change: Some(change.clone()),
            children: Vec::new(),
            expanded: false,
        });
    } else {
        // Directory component
        let dir_name = components[0];
        let dir_path = if depth == 0 {
            dir_name.to_string()
        } else {
            // Build partial path for the dir
            let path_components: Vec<&str> = full_path.split('/').collect();
            path_components[..depth + 1].join("/")
        };

        if let Some(existing) = nodes.iter_mut().find(|n| n.is_dir && n.name == dir_name) {
            insert_node(
                &mut existing.children,
                &components[1..],
                depth + 1,
                full_path,
                change,
            );
        } else {
            let mut dir_node = FileTreeNode {
                name: dir_name.to_string(),
                full_path: dir_path,
                is_dir: true,
                change: None,
                children: Vec::new(),
                expanded: true,
            };
            insert_node(
                &mut dir_node.children,
                &components[1..],
                depth + 1,
                full_path,
                change,
            );
            nodes.push(dir_node);
        }
    }
}

fn sort_nodes(nodes: &mut [FileTreeNode]) {
    nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    for node in nodes.iter_mut() {
        if node.is_dir {
            sort_nodes(&mut node.children);
        }
    }
}

fn flatten_recursive<'a>(
    nodes: &'a [FileTreeNode],
    depth: usize,
    result: &mut Vec<(&'a FileTreeNode, usize)>,
) {
    for node in nodes {
        result.push((node, depth));
        if node.is_dir && node.expanded {
            flatten_recursive(&node.children, depth + 1, result);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_changes(paths: &[&str]) -> Vec<FileChange> {
        paths
            .iter()
            .map(|p| FileChange::Modify(p.to_string()))
            .collect()
    }

    #[test]
    fn build_flat_files() {
        let changes = make_changes(&["foo.rs", "bar.rs"]);
        let nodes = FileTreeNode::build(&changes);
        assert_eq!(nodes.len(), 2);
        // sorted alpha: bar.rs, foo.rs
        assert_eq!(nodes[0].name, "bar.rs");
        assert_eq!(nodes[1].name, "foo.rs");
        assert!(!nodes[0].is_dir);
    }

    #[test]
    fn build_creates_dir_nodes() {
        let changes = make_changes(&["src/main.rs", "src/lib.rs", "README.md"]);
        let nodes = FileTreeNode::build(&changes);
        // dirs first: src/, then README.md
        assert_eq!(nodes.len(), 2);
        assert!(nodes[0].is_dir);
        assert_eq!(nodes[0].name, "src");
        assert_eq!(nodes[1].name, "README.md");
        assert_eq!(nodes[0].children.len(), 2);
        // files sorted: lib.rs, main.rs
        assert_eq!(nodes[0].children[0].name, "lib.rs");
        assert_eq!(nodes[0].children[1].name, "main.rs");
    }

    #[test]
    fn build_dirs_start_expanded() {
        let changes = make_changes(&["src/foo.rs"]);
        let nodes = FileTreeNode::build(&changes);
        assert!(nodes[0].expanded);
    }

    #[test]
    fn flatten_expanded() {
        let changes = make_changes(&["src/main.rs", "README.md"]);
        let nodes = FileTreeNode::build(&changes);
        let flat = FileTreeNode::flatten(&nodes);
        // src/ (depth 0), src/main.rs (depth 1), README.md (depth 0)
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].0.name, "src");
        assert_eq!(flat[0].1, 0);
        assert_eq!(flat[1].0.name, "main.rs");
        assert_eq!(flat[1].1, 1);
        assert_eq!(flat[2].0.name, "README.md");
        assert_eq!(flat[2].1, 0);
    }

    #[test]
    fn flatten_collapsed_dir_hides_children() {
        let changes = make_changes(&["src/main.rs", "README.md"]);
        let mut nodes = FileTreeNode::build(&changes);
        nodes[0].expanded = false; // collapse src/
        let flat = FileTreeNode::flatten(&nodes);
        assert_eq!(flat.len(), 2); // src/ and README.md only
    }

    #[test]
    fn file_change_path_variants() {
        assert_eq!(file_change_path(&FileChange::Add("a.rs".into())), "a.rs");
        assert_eq!(file_change_path(&FileChange::Modify("b.rs".into())), "b.rs");
        assert_eq!(file_change_path(&FileChange::Delete("c.rs".into())), "c.rs");
        assert_eq!(
            file_change_path(&FileChange::Move("old.rs".into(), "new.rs".into())),
            "new.rs"
        );
    }

    #[test]
    fn build_with_add_delete_move_variants() {
        let changes = vec![
            FileChange::Add("new_file.rs".into()),
            FileChange::Delete("old_file.rs".into()),
            FileChange::Move("src/before.rs".into(), "src/after.rs".into()),
            FileChange::Modify("src/lib.rs".into()),
        ];
        let nodes = FileTreeNode::build(&changes);

        // Dirs first: src/, then files (alpha: new_file.rs, old_file.rs)
        assert!(nodes[0].is_dir, "first node should be the src/ directory");
        assert_eq!(nodes[0].name, "src");

        // src/ should contain after.rs and lib.rs (sorted alpha)
        let src_children = &nodes[0].children;
        assert_eq!(src_children.len(), 2);
        assert_eq!(src_children[0].name, "after.rs");
        assert_eq!(src_children[1].name, "lib.rs");

        // Root file nodes sorted alpha
        assert_eq!(nodes[1].name, "new_file.rs");
        assert_eq!(nodes[2].name, "old_file.rs");
    }

    #[test]
    fn build_deeply_nested_path() {
        let changes = vec![FileChange::Modify("a/b/c/deep.rs".into())];
        let nodes = FileTreeNode::build(&changes);

        // a/ -> b/ -> c/ -> deep.rs
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].is_dir);
        assert_eq!(nodes[0].name, "a");

        let b = &nodes[0].children[0];
        assert!(b.is_dir);
        assert_eq!(b.name, "b");

        let c = &b.children[0];
        assert!(c.is_dir);
        assert_eq!(c.name, "c");

        let leaf = &c.children[0];
        assert!(!leaf.is_dir);
        assert_eq!(leaf.name, "deep.rs");
        assert_eq!(leaf.full_path, "a/b/c/deep.rs");
    }

    #[test]
    fn build_multiple_files_same_dir() {
        let changes = vec![
            FileChange::Add("pkg/a.rs".into()),
            FileChange::Add("pkg/b.rs".into()),
            FileChange::Add("pkg/c.rs".into()),
        ];
        let nodes = FileTreeNode::build(&changes);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "pkg");
        assert_eq!(nodes[0].children.len(), 3);
        // sorted alpha
        assert_eq!(nodes[0].children[0].name, "a.rs");
        assert_eq!(nodes[0].children[1].name, "b.rs");
        assert_eq!(nodes[0].children[2].name, "c.rs");
    }

    #[test]
    fn flatten_all_file_change_types_visible() {
        let changes = vec![
            FileChange::Add("add.rs".into()),
            FileChange::Delete("del.rs".into()),
            FileChange::Move("from.rs".into(), "to.rs".into()),
        ];
        let nodes = FileTreeNode::build(&changes);
        let flat = FileTreeNode::flatten(&nodes);
        assert_eq!(
            flat.len(),
            3,
            "all three file-change leaf nodes should be visible"
        );
        let names: Vec<&str> = flat.iter().map(|(n, _)| n.name.as_str()).collect();
        assert!(names.contains(&"add.rs"));
        assert!(names.contains(&"del.rs"));
        assert!(names.contains(&"to.rs"));
    }
}
