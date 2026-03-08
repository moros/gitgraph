// Lane-based graph layout algorithm and 2D geometry for the gitpeek commit graph.
pub mod calc;
pub mod geometry;
pub mod image;

use std::collections::HashMap;

use crate::git::CommitHash;

pub use calc::calc_graph;

/// A 2D grid position in the commit graph (lane x, row y).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub x: usize,
    pub y: usize,
}

/// A single drawn element within one graph row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Edge {
    pub edge_type: EdgeType,
    pub pos: Position,
    /// Lane index used to cycle through the color palette.
    pub color_index: usize,
}

/// The shape of an edge segment, matching standard box-drawing glyphs.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum EdgeType {
    Vertical,    // │
    Horizontal,  // ─
    Up,          // ╵
    Down,        // ╷
    Left,        // ╴
    Right,       // ╶
    RightTop,    // ╮
    RightBottom, // ╯
    LeftTop,     // ╭
    LeftBottom,  // ╰
}

impl EdgeType {
    pub fn is_vertically_related(&self) -> bool {
        matches!(self, EdgeType::Vertical | EdgeType::Up | EdgeType::Down)
    }
}

/// The computed graph layout for a repository.
#[derive(Debug)]
pub struct Graph {
    /// Commit hashes in display order (row 0 = newest).
    pub commits: Vec<CommitHash>,
    /// Maps each commit hash to its grid position.
    pub commit_pos_map: HashMap<CommitHash, Position>,
    /// Per-row edge lists. `edges[y]` contains all drawn segments for row y.
    pub edges: Vec<Vec<Edge>>,
    /// Maximum x position used (determines graph render width).
    pub max_pos_x: usize,
}
