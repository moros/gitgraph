//! RefList widget — tree-view of git refs (branches, remotes, tags, stashes).
//!
//! Ported from serie's widget/ref_list.rs with adaptations for gitgraph:
//! - Uses `Rc<Config>` instead of `Rc<AppContext>`
//! - Semver comparison uses a simple inline parser (no `semver` crate dependency)

use std::rc::Rc;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Padding, StatefulWidget},
};
use tui_tree_widget::{Tree, TreeItem, TreeState};

use crate::{config::Config, git::Ref};

const TREE_BRANCH_ROOT_IDENT: &str = "__branches__";
const TREE_REMOTE_ROOT_IDENT: &str = "__remotes__";
const TREE_TAG_ROOT_IDENT: &str = "__tags__";
const TREE_STASH_ROOT_IDENT: &str = "__stashes__";

const TREE_BRANCH_ROOT_TEXT: &str = "Branches";
const TREE_REMOTE_ROOT_TEXT: &str = "Remotes";
const TREE_TAG_ROOT_TEXT: &str = "Tags";
const TREE_STASH_ROOT_TEXT: &str = "Stashes";

// ---------------------------------------------------------------------------
// RefListState
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct RefListState {
    tree_state: TreeState<String>,
}

impl RefListState {
    pub fn new() -> Self {
        let mut tree_state = TreeState::default();
        tree_state.select(vec![TREE_BRANCH_ROOT_IDENT.into()]);
        tree_state.open(vec![TREE_BRANCH_ROOT_IDENT.into()]);
        Self { tree_state }
    }

    pub fn select_next(&mut self) {
        self.tree_state.key_down();
    }

    pub fn select_prev(&mut self) {
        self.tree_state.key_up();
    }

    pub fn select_first(&mut self) {
        self.tree_state.select_first();
    }

    pub fn select_last(&mut self) {
        self.tree_state.select_last();
    }

    pub fn open_node(&mut self) {
        self.tree_state.key_right();
    }

    pub fn close_node(&mut self) {
        self.tree_state.key_left();
    }

    /// Returns the identifier of the deepest selected node (the actual ref name).
    pub fn selected_ref_name(&self) -> Option<String> {
        self.tree_state.selected().last().cloned()
    }

    /// Returns the selected ref name if it is a local or remote branch.
    pub fn selected_branch(&self) -> Option<String> {
        let selected = self.tree_state.selected();
        if selected.len() > 1
            && (selected[0] == TREE_BRANCH_ROOT_IDENT
                || selected[0] == TREE_REMOTE_ROOT_IDENT)
        {
            selected.last().cloned()
        } else {
            None
        }
    }

    /// Returns the selected ref name if it is a tag.
    pub fn selected_tag(&self) -> Option<String> {
        let selected = self.tree_state.selected();
        if selected.len() > 1 && selected[0] == TREE_TAG_ROOT_IDENT {
            selected.last().cloned()
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// RefList widget
// ---------------------------------------------------------------------------

pub struct RefList {
    items: Vec<TreeItem<'static, String>>,
    config: Rc<Config>,
}

impl RefList {
    pub fn new(refs: &[Ref], config: Rc<Config>) -> Self {
        let items = build_ref_tree_items(refs);
        Self { items, config }
    }
}

impl StatefulWidget for RefList {
    type State = RefListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let tree = Tree::new(&self.items)
            .unwrap()
            .node_closed_symbol("\u{25b8} ") // ▸
            .node_open_symbol("\u{25be} ")   // ▾
            .node_no_children_symbol("  ")
            .highlight_style(
                Style::default().bg(self.config.color.refs_selected_bg.0),
            )
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .style(Style::default().fg(self.config.color.border.0))
                    .padding(Padding::horizontal(1)),
            );
        tree.render(area, buf, &mut state.tree_state);
    }
}

// ---------------------------------------------------------------------------
// Tree construction
// ---------------------------------------------------------------------------

fn build_ref_tree_items(refs: &[Ref]) -> Vec<TreeItem<'static, String>> {
    let mut branch_refs: Vec<String> = Vec::new();
    let mut remote_refs: Vec<String> = Vec::new();
    let mut tag_refs: Vec<String> = Vec::new();
    let mut stash_refs: Vec<(String, String)> = Vec::new();

    for r in refs {
        match r {
            Ref::Tag { name, .. } => tag_refs.push(name.clone()),
            Ref::Branch { name, .. } => branch_refs.push(name.clone()),
            Ref::RemoteBranch { name, .. } => remote_refs.push(name.clone()),
            Ref::Stash { name, message, .. } => {
                stash_refs.push((name.clone(), message.clone()))
            }
        }
    }

    let mut branch_nodes = refs_to_ref_tree_nodes(branch_refs);
    let mut remote_nodes = refs_to_ref_tree_nodes(remote_refs);
    let mut tag_nodes = refs_to_ref_tree_nodes(tag_refs);
    let mut stash_nodes = refs_to_stash_ref_tree_nodes(stash_refs);

    sort_branch_tree_nodes(&mut branch_nodes);
    sort_branch_tree_nodes(&mut remote_nodes);
    sort_tag_tree_nodes(&mut tag_nodes);
    sort_stash_tree_nodes(&mut stash_nodes);

    let branch_items = ref_tree_nodes_to_tree_items(branch_nodes);
    let remote_items = ref_tree_nodes_to_tree_items(remote_nodes);
    let tag_items = ref_tree_nodes_to_tree_items(tag_nodes);
    let stash_items = ref_tree_nodes_to_tree_items(stash_nodes);

    vec![
        TreeItem::new(
            TREE_BRANCH_ROOT_IDENT.to_string(),
            TREE_BRANCH_ROOT_TEXT,
            branch_items,
        )
        .unwrap(),
        TreeItem::new(
            TREE_REMOTE_ROOT_IDENT.to_string(),
            TREE_REMOTE_ROOT_TEXT,
            remote_items,
        )
        .unwrap(),
        TreeItem::new(
            TREE_TAG_ROOT_IDENT.to_string(),
            TREE_TAG_ROOT_TEXT,
            tag_items,
        )
        .unwrap(),
        TreeItem::new(
            TREE_STASH_ROOT_IDENT.to_string(),
            TREE_STASH_ROOT_TEXT,
            stash_items,
        )
        .unwrap(),
    ]
}

// ---------------------------------------------------------------------------
// RefTreeNode — internal tree representation before converting to TreeItems
// ---------------------------------------------------------------------------

struct RefTreeNode {
    identifier: String,
    name: String,
    children: Vec<RefTreeNode>,
}

fn refs_to_stash_ref_tree_nodes(ref_name_messages: Vec<(String, String)>) -> Vec<RefTreeNode> {
    ref_name_messages
        .into_iter()
        .map(|(name, message)| RefTreeNode {
            identifier: name,
            name: message,
            children: Vec::new(),
        })
        .collect()
}

fn refs_to_ref_tree_nodes(ref_names: Vec<String>) -> Vec<RefTreeNode> {
    let mut nodes: Vec<RefTreeNode> = Vec::new();

    for ref_name in ref_names {
        let mut parts = ref_name.split('/').collect::<Vec<_>>();
        let mut current_nodes = &mut nodes;
        let mut parent_identifier = String::new();

        while !parts.is_empty() {
            let part = parts.remove(0);
            if let Some(index) = current_nodes.iter().position(|n| n.name == part) {
                let node = &mut current_nodes[index];
                current_nodes = &mut node.children;
                parent_identifier.clone_from(&node.identifier);
            } else {
                let identifier = if parent_identifier.is_empty() {
                    part.to_string()
                } else {
                    format!("{parent_identifier}/{part}")
                };
                let node = RefTreeNode {
                    identifier: identifier.clone(),
                    name: part.to_string(),
                    children: Vec::new(),
                };
                current_nodes.push(node);
                current_nodes = current_nodes.last_mut().unwrap().children.as_mut();
                parent_identifier = identifier;
            }
        }
    }

    nodes
}

fn ref_tree_nodes_to_tree_items(nodes: Vec<RefTreeNode>) -> Vec<TreeItem<'static, String>> {
    nodes
        .into_iter()
        .map(|node| {
            if node.children.is_empty() {
                TreeItem::new(node.identifier, node.name, vec![]).unwrap()
            } else {
                let children = ref_tree_nodes_to_tree_items(node.children);
                TreeItem::new(node.identifier, node.name, children).unwrap()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Sorting
// ---------------------------------------------------------------------------

fn sort_branch_tree_nodes(nodes: &mut [RefTreeNode]) {
    nodes.sort_by(|a, b| {
        b.children
            .len()
            .cmp(&a.children.len())
            .then(a.name.cmp(&b.name))
    });
    for node in nodes {
        sort_branch_tree_nodes(&mut node.children);
    }
}

fn sort_tag_tree_nodes(nodes: &mut [RefTreeNode]) {
    nodes.sort_by(|a, b| {
        let a_v = parse_semver(&a.name);
        let b_v = parse_semver(&b.name);
        match (a_v, b_v) {
            (None, None) => a.name.cmp(&b.name),
            // semver tags sort descending; non-semver tags go after
            (a_ver, b_ver) => b_ver.cmp(&a_ver),
        }
    });
}

fn sort_stash_tree_nodes(nodes: &mut [RefTreeNode]) {
    nodes.sort_by(|a, b| a.identifier.cmp(&b.identifier));
}

/// Parse a simple semver tag (optionally prefixed with 'v') into `(major, minor, patch)`.
/// Returns `None` for anything that doesn't parse as `MAJOR.MINOR.PATCH`.
fn parse_semver(tag: &str) -> Option<(u64, u64, u64)> {
    let tag = tag.trim_start_matches('v');
    let parts: Vec<&str> = tag.splitn(4, '.').collect();
    if parts.len() < 3 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    // Patch may have a pre-release suffix like "0-alpha"; take only the numeric part.
    let patch = parts[2].split('-').next()?.parse::<u64>().ok()?;
    Some((major, minor, patch))
}
