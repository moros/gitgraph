//! Text-based graph rendering using Unicode box-drawing characters.
//!
//! Used as a fallback when the terminal does not support inline image protocols
//! (iTerm2 / Kitty). Each commit row is rendered as a string of box-drawing
//! glyphs (│ ─ ╮ ╯ ╭ ╰ …) with the commit node shown as `●`.

use crate::git::CommitHash;
use crate::graph::{EdgeType, Graph};

/// Render a single graph row for `commit_hash` as a Unicode text string.
///
/// Returns a string of length `graph.max_pos_x + 1`, with each character
/// representing the box-drawing glyph for that lane. The commit's own lane
/// is always `●`.
pub fn render_text_row(graph: &Graph, commit_hash: &CommitHash) -> String {
    let pos = &graph.commit_pos_map[commit_hash];
    let commit_x = pos.x;
    let cell_count = graph.max_pos_x + 1;
    let row_edges = &graph.edges[pos.y];

    let mut cells: Vec<char> = vec![' '; cell_count];

    for edge in row_edges {
        if edge.pos.x < cell_count {
            cells[edge.pos.x] = edge_type_to_char(edge.edge_type);
        }
    }

    // Commit node overrides any edge at the same position.
    cells[commit_x] = '●';

    cells.into_iter().collect()
}

fn edge_type_to_char(et: EdgeType) -> char {
    match et {
        EdgeType::Vertical => '│',
        EdgeType::Horizontal => '─',
        EdgeType::Up => '╵',
        EdgeType::Down => '╷',
        EdgeType::Left => '╴',
        EdgeType::Right => '╶',
        EdgeType::RightTop => '╮',
        EdgeType::RightBottom => '╯',
        EdgeType::LeftTop => '╭',
        EdgeType::LeftBottom => '╰',
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::CommitHash;
    use crate::graph::{Edge, Graph, Position};
    use std::collections::HashMap;

    fn make_hash(s: &str) -> CommitHash {
        CommitHash(s.to_string())
    }

    fn simple_graph() -> Graph {
        // Single commit at (0,0), one vertical edge
        let hash = make_hash("aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111");
        let mut commit_pos_map = HashMap::new();
        commit_pos_map.insert(hash.clone(), Position { x: 0, y: 0 });
        let edges = vec![vec![Edge {
            edge_type: EdgeType::Vertical,
            pos: Position { x: 0, y: 0 },
            color_index: 0,
        }]];
        Graph {
            commits: vec![hash],
            commit_pos_map,
            edges,
            max_pos_x: 0,
        }
    }

    #[test]
    fn commit_dot_placed_at_commit_column() {
        let graph = simple_graph();
        let hash = graph.commits[0].clone();
        let row = render_text_row(&graph, &hash);
        assert_eq!(row, "●");
    }

    #[test]
    fn edge_type_chars_cover_all_variants() {
        use EdgeType::*;
        let cases = [
            (Vertical, '│'),
            (Horizontal, '─'),
            (Up, '╵'),
            (Down, '╷'),
            (Left, '╴'),
            (Right, '╶'),
            (RightTop, '╮'),
            (RightBottom, '╯'),
            (LeftTop, '╭'),
            (LeftBottom, '╰'),
        ];
        for (et, expected) in cases {
            assert_eq!(edge_type_to_char(et), expected, "{et:?}");
        }
    }

    #[test]
    fn multi_lane_row_has_correct_width() {
        // Two lanes: commit at x=1, vertical at x=0
        let hash = make_hash("bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222");
        let mut commit_pos_map = HashMap::new();
        commit_pos_map.insert(hash.clone(), Position { x: 1, y: 0 });
        let edges = vec![vec![
            Edge {
                edge_type: EdgeType::Vertical,
                pos: Position { x: 0, y: 0 },
                color_index: 0,
            },
            Edge {
                edge_type: EdgeType::Vertical,
                pos: Position { x: 1, y: 0 },
                color_index: 1,
            },
        ]];
        let graph = Graph {
            commits: vec![hash.clone()],
            commit_pos_map,
            edges,
            max_pos_x: 1,
        };
        let row = render_text_row(&graph, &hash);
        assert_eq!(row.chars().count(), 2);
        let chars: Vec<char> = row.chars().collect();
        assert_eq!(chars[0], '│');
        assert_eq!(chars[1], '●'); // commit overrides vertical
    }
}
