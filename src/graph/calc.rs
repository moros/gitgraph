use std::collections::HashMap;

use crate::git::{Commit, CommitHash, Repository};
use crate::graph::{Edge, EdgeType, Graph, Position};

type CommitPosMap = HashMap<CommitHash, Position>;

/// Compute the full graph layout for `repository`.
///
/// Commits are rendered newest-first (row 0 = index 0 in `repository.all_commits()`).
/// Lane assignment follows the same greedy algorithm as serie:
/// - A commit with no "first-parent children" in scope starts a new branch in the
///   first vacant lane.
/// - A commit whose children occupy known lanes takes the leftmost of those lanes
///   (the others are freed).
///
/// Edge drawing is a two-pass process:
/// - Pass 1: commit (vertical same-lane) edges and branch (first-parent, lane-change) edges.
/// - Pass 2: merge edges, with a detour to avoid overlapping existing verticals.
pub fn calc_graph(repository: &Repository) -> Graph {
    let commits = repository.all_commits();
    let commit_pos_map = calc_commit_positions(&commits, repository);
    let (edges, max_pos_x) = calc_edges(&commit_pos_map, &commits, repository);

    Graph {
        commits: commits.iter().map(|c| c.hash.clone()).collect(),
        commit_pos_map,
        edges,
        max_pos_x,
    }
}

// ---------------------------------------------------------------------------
// Position calculation
// ---------------------------------------------------------------------------

fn calc_commit_positions(commits: &[&Commit], repository: &Repository) -> CommitPosMap {
    let mut pos_map: CommitPosMap = HashMap::new();
    // Each slot holds the hash of the commit currently "owning" that lane.
    let mut lane_state: Vec<Option<CommitHash>> = Vec::new();

    for (pos_y, commit) in commits.iter().enumerate() {
        let first_parent_children = first_parent_children(commit, repository);
        let pos_x = if first_parent_children.is_empty() {
            // New branch — grab the first vacant lane.
            let x = first_vacant_lane(&lane_state);
            set_lane(&mut lane_state, x, Some(commit.hash.clone()));
            x
        } else {
            // Release all child lanes; take the leftmost one.
            update_lane(&mut lane_state, &commit.hash, &first_parent_children)
        };

        pos_map.insert(commit.hash.clone(), Position { x: pos_x, y: pos_y });
    }

    pos_map
}

/// Children of `commit` for which `commit` is the **first** parent.
/// These are the commits that continue the same lane downward.
fn first_parent_children<'a>(commit: &Commit, repository: &'a Repository) -> Vec<&'a CommitHash> {
    repository
        .children_hash(&commit.hash)
        .into_iter()
        .filter(|child_hash| {
            let child_parents = repository.parents_hash(child_hash);
            !child_parents.is_empty() && *child_parents[0] == commit.hash
        })
        .collect()
}

fn first_vacant_lane(lane_state: &[Option<CommitHash>]) -> usize {
    lane_state
        .iter()
        .position(|s| s.is_none())
        .unwrap_or(lane_state.len())
}

fn set_lane(lane_state: &mut Vec<Option<CommitHash>>, x: usize, value: Option<CommitHash>) {
    if lane_state.len() <= x {
        lane_state.push(value);
    } else {
        lane_state[x] = value;
    }
}

/// Clear all lanes held by `child_hashes`, then place `commit_hash` in the
/// leftmost freed lane. Returns that lane index.
fn update_lane(
    lane_state: &mut [Option<CommitHash>],
    commit_hash: &CommitHash,
    child_hashes: &[&CommitHash],
) -> usize {
    if lane_state.is_empty() {
        return 0;
    }
    let mut min_x = lane_state.len().saturating_sub(1);
    for target in child_hashes {
        for (x, slot) in lane_state.iter_mut().enumerate() {
            if slot.as_ref().map(|h| h == *target).unwrap_or(false) {
                *slot = None;
                if x < min_x {
                    min_x = x;
                }
                break;
            }
        }
    }
    lane_state[min_x] = Some(commit_hash.clone());
    min_x
}

// ---------------------------------------------------------------------------
// Edge calculation
// ---------------------------------------------------------------------------

/// Internal edge representation that also tracks the owning commit hash,
/// used for overlap detection in the merge pass.
#[derive(Clone)]
struct PendingEdge {
    edge_type: EdgeType,
    pos_x: usize,
    /// The lane x that determines this edge's color (= associated_line_pos_x in serie).
    color_x: usize,
    owner: CommitHash,
}

impl PendingEdge {
    fn new(edge_type: EdgeType, pos_x: usize, color_x: usize, owner: CommitHash) -> Self {
        Self {
            edge_type,
            pos_x,
            color_x,
            owner,
        }
    }
}

fn calc_edges(
    pos_map: &CommitPosMap,
    commits: &[&Commit],
    repository: &Repository,
) -> (Vec<Vec<Edge>>, usize) {
    let n = commits.len();
    let mut max_pos_x: usize = 0;
    let mut pending: Vec<Vec<PendingEdge>> = vec![vec![]; n];

    // ------------------------------------------------------------------
    // Pass 1: commit (same-lane vertical) and branch (first-parent lane
    // change) edges.
    // ------------------------------------------------------------------
    for commit in commits.iter() {
        let pos = pos_map[&commit.hash];

        for child_hash in repository.children_hash(&commit.hash) {
            let child_pos = pos_map[child_hash];

            if pos.x == child_pos.x {
                // Straight vertical — same lane.
                push_edge(
                    &mut pending,
                    pos.y,
                    EdgeType::Up,
                    pos.x,
                    pos.x,
                    &commit.hash,
                );
                for y in (child_pos.y + 1)..pos.y {
                    push_edge(
                        &mut pending,
                        y,
                        EdgeType::Vertical,
                        pos.x,
                        pos.x,
                        &commit.hash,
                    );
                }
                push_edge(
                    &mut pending,
                    child_pos.y,
                    EdgeType::Down,
                    pos.x,
                    pos.x,
                    &commit.hash,
                );
            } else {
                // Different lanes — decide based on parent relationship.
                let child_first_parent = &commits[child_pos.y].parent_hashes[0];
                if *child_first_parent == commit.hash {
                    // Branch: this commit is the first parent of the child, but
                    // they ended up in different lanes. Draw horizontal + corner.
                    draw_branch_edges(&mut pending, pos, child_pos, &commit.hash);
                }
                // Merge edges (not first parent) are handled in pass 2.
            }
        }

        if pos.x > max_pos_x {
            max_pos_x = pos.x;
        }

        // If this commit has parents that are not in the graph (truncated log),
        // draw a dangling vertical so the lane appears to continue.
        if !commit.parent_hashes.is_empty() && repository.commit(&commit.parent_hashes[0]).is_none()
        {
            push_edge(
                &mut pending,
                pos.y,
                EdgeType::Down,
                pos.x,
                pos.x,
                &commit.hash,
            );
            for y in (pos.y + 1)..n {
                push_edge(
                    &mut pending,
                    y,
                    EdgeType::Vertical,
                    pos.x,
                    pos.x,
                    &commit.hash,
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // Pass 2: merge edges, with optional detour to avoid vertical overlap.
    // ------------------------------------------------------------------
    for commit in commits.iter() {
        let pos = pos_map[&commit.hash];

        for child_hash in repository.children_hash(&commit.hash) {
            let child_pos = pos_map[child_hash];

            if pos.x == child_pos.x {
                continue; // commit edge, already handled
            }

            let child_first_parent = &commits[child_pos.y].parent_hashes[0];
            if *child_first_parent == commit.hash {
                continue; // branch edge, already handled
            }

            // Merge edge: this commit is a non-first parent of child_hash.
            let (overlap, new_pos_x) =
                find_merge_overlap(&pending, pos_map, commits, pos, child_pos, &commit.hash);

            if overlap {
                draw_merge_detour_edges(&mut pending, pos, child_pos, new_pos_x, &commit.hash);
                if new_pos_x > max_pos_x {
                    max_pos_x = new_pos_x;
                }
            } else {
                draw_merge_direct_edges(&mut pending, pos, child_pos, &commit.hash);
            }
        }

        if pos.x > max_pos_x {
            max_pos_x = pos.x;
        }
    }

    // Flatten PendingEdge rows into final Edge rows, sort and dedup.
    let edges = pending
        .into_iter()
        .enumerate()
        .map(|(y, row)| {
            let mut row: Vec<Edge> = row
                .into_iter()
                .map(|pe| Edge {
                    edge_type: pe.edge_type,
                    pos: Position { x: pe.pos_x, y },
                    color_index: pe.color_x,
                })
                .collect();
            row.sort_by_key(|e| (e.color_index, e.pos.x, e.edge_type));
            row.dedup();
            row
        })
        .collect();

    (edges, max_pos_x)
}

// ---------------------------------------------------------------------------
// Edge drawing helpers
// ---------------------------------------------------------------------------

fn push_edge(
    pending: &mut [Vec<PendingEdge>],
    row: usize,
    edge_type: EdgeType,
    pos_x: usize,
    color_x: usize,
    owner: &CommitHash,
) {
    pending[row].push(PendingEdge::new(edge_type, pos_x, color_x, owner.clone()));
}

/// Draw horizontal + corner edges for a branch that changes lanes.
/// `pos` = parent position, `child_pos` = child (branch start) position.
fn draw_branch_edges(
    pending: &mut [Vec<PendingEdge>],
    pos: Position,
    child_pos: Position,
    owner: &CommitHash,
) {
    if pos.x < child_pos.x {
        // Parent is to the left of child — curve right then down.
        push_edge(pending, pos.y, EdgeType::Right, pos.x, child_pos.x, owner);
        for x in (pos.x + 1)..child_pos.x {
            push_edge(pending, pos.y, EdgeType::Horizontal, x, child_pos.x, owner);
        }
        push_edge(
            pending,
            pos.y,
            EdgeType::RightBottom,
            child_pos.x,
            child_pos.x,
            owner,
        );
    } else {
        // Parent is to the right of child — curve left then down.
        push_edge(pending, pos.y, EdgeType::Left, pos.x, child_pos.x, owner);
        for x in (child_pos.x + 1)..pos.x {
            push_edge(pending, pos.y, EdgeType::Horizontal, x, child_pos.x, owner);
        }
        push_edge(
            pending,
            pos.y,
            EdgeType::LeftBottom,
            child_pos.x,
            child_pos.x,
            owner,
        );
    }
    // Vertical stretch from child row down to parent row.
    for y in (child_pos.y + 1)..pos.y {
        push_edge(
            pending,
            y,
            EdgeType::Vertical,
            child_pos.x,
            child_pos.x,
            owner,
        );
    }
    push_edge(
        pending,
        child_pos.y,
        EdgeType::Down,
        child_pos.x,
        child_pos.x,
        owner,
    );
}

/// Determine whether drawing a merge edge from `pos` to `child_pos` would
/// overlap any existing vertical edge owned by a different commit. If so,
/// also compute the x position to detour to (one past the rightmost obstacle).
fn find_merge_overlap(
    pending: &[Vec<PendingEdge>],
    pos_map: &CommitPosMap,
    commits: &[&Commit],
    pos: Position,
    child_pos: Position,
    owner: &CommitHash,
) -> (bool, usize) {
    let mut new_pos_x = pos.x;

    // Quick skip: if no intervening commit sits at pos.x and no foreign vertical
    // edges cross pos.x, we can draw straight without detour.
    let mut must_check = false;
    for y in (child_pos.y + 1)..pos.y {
        let row_commit_x = pos_map[&commits[y].hash].x;
        if row_commit_x == new_pos_x {
            must_check = true;
            break;
        }
        if pending[y]
            .iter()
            .any(|e| e.pos_x == pos.x && e.edge_type == EdgeType::Vertical && e.owner != *owner)
        {
            must_check = true;
            break;
        }
    }

    if !must_check {
        return (false, new_pos_x);
    }

    let mut overlap = false;
    for y in (child_pos.y + 1)..pos.y {
        let row_commit_x = pos_map[&commits[y].hash].x;
        if row_commit_x == new_pos_x {
            overlap = true;
            if new_pos_x < row_commit_x + 1 {
                new_pos_x = row_commit_x + 1;
            }
        }
        for edge in &pending[y] {
            if edge.pos_x >= new_pos_x
                && edge.owner != *owner
                && edge.edge_type == EdgeType::Vertical
            {
                overlap = true;
                if new_pos_x < edge.pos_x + 1 {
                    new_pos_x = edge.pos_x + 1;
                }
            }
        }
    }

    (overlap, new_pos_x)
}

/// Draw a direct (no-detour) merge edge from `pos` up to `child_pos`.
fn draw_merge_direct_edges(
    pending: &mut [Vec<PendingEdge>],
    pos: Position,
    child_pos: Position,
    owner: &CommitHash,
) {
    push_edge(pending, pos.y, EdgeType::Up, pos.x, pos.x, owner);
    for y in (child_pos.y + 1)..pos.y {
        push_edge(pending, y, EdgeType::Vertical, pos.x, pos.x, owner);
    }
    if pos.x < child_pos.x {
        push_edge(pending, child_pos.y, EdgeType::LeftTop, pos.x, pos.x, owner);
        for x in (pos.x + 1)..child_pos.x {
            push_edge(pending, child_pos.y, EdgeType::Horizontal, x, pos.x, owner);
        }
        push_edge(
            pending,
            child_pos.y,
            EdgeType::Left,
            child_pos.x,
            pos.x,
            owner,
        );
    } else {
        push_edge(
            pending,
            child_pos.y,
            EdgeType::RightTop,
            pos.x,
            pos.x,
            owner,
        );
        for x in (child_pos.x + 1)..pos.x {
            push_edge(pending, child_pos.y, EdgeType::Horizontal, x, pos.x, owner);
        }
        push_edge(
            pending,
            child_pos.y,
            EdgeType::Right,
            child_pos.x,
            pos.x,
            owner,
        );
    }
}

/// Draw a detoured merge edge that steps right to `new_pos_x` to avoid overlap.
fn draw_merge_detour_edges(
    pending: &mut [Vec<PendingEdge>],
    pos: Position,
    child_pos: Position,
    new_pos_x: usize,
    owner: &CommitHash,
) {
    // At parent row: go right to detour lane.
    push_edge(pending, pos.y, EdgeType::Right, pos.x, pos.x, owner);
    for x in (pos.x + 1)..new_pos_x {
        push_edge(pending, pos.y, EdgeType::Horizontal, x, pos.x, owner);
    }
    push_edge(
        pending,
        pos.y,
        EdgeType::RightBottom,
        new_pos_x,
        pos.x,
        owner,
    );

    // Vertical in detour lane from parent row up to child row.
    for y in (child_pos.y + 1)..pos.y {
        push_edge(pending, y, EdgeType::Vertical, new_pos_x, pos.x, owner);
    }

    // At child row: come back left to child lane.
    push_edge(
        pending,
        child_pos.y,
        EdgeType::RightTop,
        new_pos_x,
        pos.x,
        owner,
    );
    for x in (child_pos.x + 1)..new_pos_x {
        push_edge(pending, child_pos.y, EdgeType::Horizontal, x, pos.x, owner);
    }
    push_edge(
        pending,
        child_pos.y,
        EdgeType::Right,
        child_pos.x,
        pos.x,
        owner,
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{Commit, CommitHash, Repository};
    use std::collections::HashMap;

    fn make_repo(commits: Vec<(&str, Vec<&str>)>) -> Repository {
        // `commits` is ordered newest-first. Each entry is (hash, [parent_hashes...]).
        let commit_list: Vec<Commit> = commits
            .iter()
            .map(|(hash, parents)| Commit {
                hash: CommitHash::from(*hash),
                parent_hashes: parents.iter().map(|p| CommitHash::from(*p)).collect(),
                ..Default::default()
            })
            .collect();

        let commit_hashes: Vec<CommitHash> = commit_list.iter().map(|c| c.hash.clone()).collect();
        let mut commit_map: HashMap<CommitHash, Commit> = HashMap::new();
        let mut children_map: HashMap<CommitHash, Vec<CommitHash>> = HashMap::new();

        for commit in &commit_list {
            for parent in &commit.parent_hashes {
                children_map
                    .entry(parent.clone())
                    .or_default()
                    .push(commit.hash.clone());
            }
            commit_map.insert(commit.hash.clone(), commit.clone());
        }

        Repository::new(commit_hashes, commit_map, children_map)
    }

    #[test]
    fn test_linear_graph() {
        // A → B → C (linear, single lane)
        let repo = make_repo(vec![("A", vec!["B"]), ("B", vec!["C"]), ("C", vec![])]);
        let graph = calc_graph(&repo);

        assert_eq!(graph.commits.len(), 3);
        assert_eq!(graph.max_pos_x, 0);

        // All commits should be in lane 0.
        for hash in &graph.commits {
            assert_eq!(graph.commit_pos_map[hash].x, 0);
        }
    }

    #[test]
    fn test_branch_and_merge() {
        // Merge commit M has two parents: B (first) and C.
        // B branches off A; C is an independent line that merges into M.
        //   M   (row 0)  parents: [B, C]
        //   B   (row 1)  parent: [A]
        //   C   (row 2)  parent: [A]
        //   A   (row 3)  parent: []
        let repo = make_repo(vec![
            ("M", vec!["B", "C"]),
            ("B", vec!["A"]),
            ("C", vec!["A"]),
            ("A", vec![]),
        ]);
        let graph = calc_graph(&repo);
        assert_eq!(graph.commits.len(), 4);

        // M is at x=0 (first parent B is at x=0).
        let m_pos = graph.commit_pos_map[&CommitHash::from("M")];
        let b_pos = graph.commit_pos_map[&CommitHash::from("B")];
        assert_eq!(m_pos.x, b_pos.x);
    }

    #[test]
    fn test_positions_are_unique_per_row() {
        let repo = make_repo(vec![
            ("M", vec!["B", "C"]),
            ("B", vec!["A"]),
            ("C", vec!["A"]),
            ("A", vec![]),
        ]);
        let graph = calc_graph(&repo);

        // No two commits in the same row should share an x position.
        let mut by_row: HashMap<usize, Vec<usize>> = HashMap::new();
        for hash in &graph.commits {
            let pos = graph.commit_pos_map[hash];
            by_row.entry(pos.y).or_default().push(pos.x);
        }
        for xs in by_row.values() {
            let mut sorted = xs.clone();
            sorted.dedup();
            assert_eq!(sorted.len(), xs.len());
        }
    }

    #[test]
    fn test_commit_ordering_newest_first() {
        // Graph commits list must preserve the input order (newest-first).
        let repo = make_repo(vec![("C3", vec!["C2"]), ("C2", vec!["C1"]), ("C1", vec![])]);
        let graph = calc_graph(&repo);

        assert_eq!(graph.commits[0], CommitHash::from("C3"));
        assert_eq!(graph.commits[1], CommitHash::from("C2"));
        assert_eq!(graph.commits[2], CommitHash::from("C1"));

        // Positions must reflect order: row 0 = newest.
        assert_eq!(graph.commit_pos_map[&CommitHash::from("C3")].y, 0);
        assert_eq!(graph.commit_pos_map[&CommitHash::from("C1")].y, 2);
    }

    #[test]
    fn test_single_commit() {
        let repo = make_repo(vec![("ROOT", vec![])]);
        let graph = calc_graph(&repo);
        assert_eq!(graph.commits.len(), 1);
        assert_eq!(graph.max_pos_x, 0);
        let pos = graph.commit_pos_map[&CommitHash::from("ROOT")];
        assert_eq!(pos.x, 0);
        assert_eq!(pos.y, 0);
    }

    #[test]
    fn test_edges_non_empty_for_connected_graph() {
        // A two-commit linear graph must produce edges connecting them.
        let repo = make_repo(vec![("HEAD", vec!["BASE"]), ("BASE", vec![])]);
        let graph = calc_graph(&repo);
        let total_edges: usize = graph.edges.iter().map(|row| row.len()).sum();
        assert!(total_edges > 0, "connected graph must have edges");
    }

    #[test]
    fn test_parallel_branches_occupy_different_lanes() {
        // Two independent branch tips that share no parent until ROOT.
        //   A   (row 0, parent ROOT)
        //   B   (row 1, parent ROOT)
        //  ROOT (row 2, no parents)
        let repo = make_repo(vec![
            ("A", vec!["ROOT"]),
            ("B", vec!["ROOT"]),
            ("ROOT", vec![]),
        ]);
        let graph = calc_graph(&repo);

        let a_x = graph.commit_pos_map[&CommitHash::from("A")].x;
        let b_x = graph.commit_pos_map[&CommitHash::from("B")].x;
        // A and B must be in different lanes.
        assert_ne!(a_x, b_x);
    }

    #[test]
    fn test_double_merge() {
        // Two merges in sequence, all positions must be unique per row.
        //   M2  (row 0, parents [M1, D])
        //   M1  (row 1, parents [B, C])
        //   B   (row 2, parent [A])
        //   C   (row 3, parent [A])
        //   D   (row 4, parent [A])
        //   A   (row 5, no parents)
        let repo = make_repo(vec![
            ("M2", vec!["M1", "D"]),
            ("M1", vec!["B", "C"]),
            ("B", vec!["A"]),
            ("C", vec!["A"]),
            ("D", vec!["A"]),
            ("A", vec![]),
        ]);
        let graph = calc_graph(&repo);
        assert_eq!(graph.commits.len(), 6);

        // No row may have duplicate x positions.
        let mut by_row: HashMap<usize, Vec<usize>> = HashMap::new();
        for hash in &graph.commits {
            let pos = graph.commit_pos_map[hash];
            by_row.entry(pos.y).or_default().push(pos.x);
        }
        for (_, xs) in &by_row {
            let mut deduped = xs.clone();
            deduped.dedup();
            assert_eq!(deduped.len(), xs.len());
        }
    }
}
