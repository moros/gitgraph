use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Debug, Formatter},
    io::Cursor,
};

use rayon::prelude::*;

use crate::{
    git::CommitHash,
    graph::{
        geometry::{bounding_box_u32, Point},
        Edge, EdgeType, Graph,
    },
    protocol::ImageProtocol,
};

// ─── GraphStyle ──────────────────────────────────────────────────────────────

/// Visual style for edge corners in the commit graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphStyle {
    /// Bezier-arc corners (smooth curves).
    Rounded,
    /// Sharp right-angle corners.
    Angular,
}

// ─── CellWidthType ───────────────────────────────────────────────────────────

/// Whether each graph column occupies one or two terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellWidthType {
    /// 2 terminal cells per column (50 px wide in the default params).
    Double,
    /// 1 terminal cell per column (25 px wide in the default params).
    Single,
}

// ─── ImageColors ─────────────────────────────────────────────────────────────

/// RGBA color palette used by the graph image renderer.
///
/// This is a rendering-layer type that holds pre-converted `image::Rgba<u8>` values.
/// Build it from your application's `GraphColorSet` (see `ImageColors::from_rgba_list`).
///
/// # Transparency
/// An alpha of 0 signals "transparent" — the terminal background shows through.
/// Pass `image::Rgba([0, 0, 0, 0])` for `edge` or `background` to get transparency.
#[derive(Debug, Clone)]
pub struct ImageColors {
    /// Branch-line colors, cycled by lane (x-position) index.
    pub branches: Vec<image::Rgba<u8>>,
    /// Ring color drawn around commit dots. Alpha=0 → transparent (no ring).
    pub edge: image::Rgba<u8>,
    /// Graph background fill. Alpha=0 → transparent (terminal background shows through).
    pub background: image::Rgba<u8>,
}

impl ImageColors {
    pub fn new(
        branches: Vec<image::Rgba<u8>>,
        edge: image::Rgba<u8>,
        background: image::Rgba<u8>,
    ) -> Self {
        Self { branches, edge, background }
    }
}

impl Default for ImageColors {
    /// One Dark palette — matches `GraphColorSet::default()`.
    fn default() -> Self {
        ImageColors {
            branches: vec![
                image::Rgba([0xE0, 0x6C, 0x75, 255]), // #E06C75 — red
                image::Rgba([0xE5, 0xC0, 0x7B, 255]), // #E5C07B — yellow
                image::Rgba([0x98, 0xC3, 0x79, 255]), // #98C379 — green
                image::Rgba([0x56, 0xB6, 0xC2, 255]), // #56B6C2 — teal
                image::Rgba([0x61, 0xAF, 0xEF, 255]), // #61AFEF — blue
                image::Rgba([0xC6, 0x78, 0xDD, 255]), // #C678DD — purple
            ],
            edge: image::Rgba([0, 0, 0, 0]),       // transparent
            background: image::Rgba([0, 0, 0, 0]), // transparent
        }
    }
}

// ─── GraphImageManager ───────────────────────────────────────────────────────

/// Manages pre-rendered, protocol-encoded graph images keyed by commit hash.
///
/// Images are generated lazily (via [`load_encoded_image`]) or eagerly
/// (pass `preload = true` to [`new`]).
#[derive(Debug)]
pub struct GraphImageManager<'a> {
    encoded_image_map: HashMap<CommitHash, String>,

    graph: &'a Graph,
    cell_width_type: CellWidthType,
    graph_style: GraphStyle,
    image_params: ImageParams,
    drawing_pixels: DrawingPixels,
    image_protocol: ImageProtocol,
}

impl<'a> GraphImageManager<'a> {
    pub fn new(
        graph: &'a Graph,
        image_colors: &ImageColors,
        cell_width_type: CellWidthType,
        graph_style: GraphStyle,
        image_protocol: ImageProtocol,
        preload: bool,
    ) -> Self {
        let image_params = ImageParams::new(image_colors, cell_width_type);
        let drawing_pixels = DrawingPixels::new(&image_params);

        let mut m = GraphImageManager {
            encoded_image_map: HashMap::default(),
            graph,
            cell_width_type,
            graph_style,
            image_params,
            drawing_pixels,
            image_protocol,
        };
        if preload {
            m.load_all_encoded_image();
        }
        m
    }

    /// Return the pre-encoded terminal escape string for `commit_hash`.
    ///
    /// # Panics
    /// Panics if the image has not been loaded yet (call `load_encoded_image` first).
    pub fn encoded_image(&self, commit_hash: &CommitHash) -> &str {
        self.encoded_image_map.get(commit_hash).unwrap()
    }

    /// Pre-render and encode images for every commit in the graph.
    pub fn load_all_encoded_image(&mut self) {
        let graph_image = build_graph_image(
            self.graph,
            &self.image_params,
            &self.drawing_pixels,
            self.graph_style,
        );
        let encoded: HashMap<CommitHash, String> = self
            .graph
            .commits
            .iter()
            .enumerate()
            .map(|(i, commit_hash)| {
                let edges = &self.graph.edges[i];
                let image =
                    graph_image.images[edges].encode(self.cell_width_type, self.image_protocol);
                (commit_hash.clone(), image)
            })
            .collect();
        self.encoded_image_map = encoded;
    }

    /// Preload encoded images for commits in `start..end` using a rayon thread pool.
    ///
    /// Already-cached commits are skipped. This is the primary API for keeping the
    /// visible window preloaded during fast J/K navigation.
    pub fn preload_range(&mut self, start: usize, end: usize) {
        let end = end.min(self.graph.commits.len());
        if start >= end {
            return;
        }

        let to_render: Vec<&CommitHash> = self.graph.commits[start..end]
            .iter()
            .filter(|h| !self.encoded_image_map.contains_key(*h))
            .collect();

        if to_render.is_empty() {
            return;
        }

        // Capture shared references so the parallel closure does not borrow `self`.
        let graph = self.graph;
        let image_params = &self.image_params;
        let drawing_pixels = &self.drawing_pixels;
        let graph_style = self.graph_style;
        let cell_width_type = self.cell_width_type;
        let image_protocol = self.image_protocol;

        let new_entries: Vec<(CommitHash, String)> = to_render
            .into_par_iter()
            .map(|commit_hash| {
                let row_image = build_single_graph_row_image(
                    graph,
                    image_params,
                    drawing_pixels,
                    graph_style,
                    commit_hash,
                );
                let encoded = row_image.encode(cell_width_type, image_protocol);
                (commit_hash.clone(), encoded)
            })
            .collect();

        for (hash, encoded) in new_entries {
            self.encoded_image_map.insert(hash, encoded);
        }
    }

    /// Lazy-load the encoded image for a single commit (no-op if already cached).
    pub fn load_encoded_image(&mut self, commit_hash: &CommitHash) {
        if self.encoded_image_map.contains_key(commit_hash) {
            return;
        }
        let graph_row_image = build_single_graph_row_image(
            self.graph,
            &self.image_params,
            &self.drawing_pixels,
            self.graph_style,
            commit_hash,
        );
        let image = graph_row_image.encode(self.cell_width_type, self.image_protocol);
        self.encoded_image_map.insert(commit_hash.clone(), image);
    }
}

// ─── GraphImage / GraphRowImage ──────────────────────────────────────────────

/// Collection of rendered row images, keyed by edge pattern.
#[derive(Debug, Default)]
pub struct GraphImage {
    pub images: HashMap<Vec<Edge>, GraphRowImage>,
}

/// A single rendered row: raw PNG bytes and the column count.
pub struct GraphRowImage {
    pub bytes: Vec<u8>,
    pub cell_count: usize,
}

impl Debug for GraphRowImage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GraphRowImage {{ bytes: [{} bytes], cell_count: {} }}",
            self.bytes.len(),
            self.cell_count
        )
    }
}

impl GraphRowImage {
    fn encode(&self, cell_width_type: CellWidthType, image_protocol: ImageProtocol) -> String {
        let image_cell_width = match cell_width_type {
            CellWidthType::Double => self.cell_count * 2,
            CellWidthType::Single => self.cell_count,
        };
        image_protocol.encode(&self.bytes, image_cell_width)
    }
}

// ─── ImageParams ─────────────────────────────────────────────────────────────

/// Pixel-level rendering parameters derived from the color palette and cell width.
#[derive(Debug)]
pub struct ImageParams {
    width: u16,
    height: u16,
    line_width: u16,
    circle_inner_radius: u16,
    circle_outer_radius: u16,
    edge_colors: Vec<image::Rgba<u8>>,
    circle_edge_color: image::Rgba<u8>,
    background_color: image::Rgba<u8>,
}

impl ImageParams {
    pub fn new(image_colors: &ImageColors, cell_width_type: CellWidthType) -> Self {
        let (width, height, line_width, circle_inner_radius, circle_outer_radius) =
            match cell_width_type {
                CellWidthType::Double => (50, 50, 5, 10, 13),
                CellWidthType::Single => (25, 50, 3, 7, 10),
            };
        Self {
            width,
            height,
            line_width,
            circle_inner_radius,
            circle_outer_radius,
            edge_colors: image_colors.branches.clone(),
            circle_edge_color: image_colors.edge,
            background_color: image_colors.background,
        }
    }

    fn edge_color(&self, index: usize) -> image::Rgba<u8> {
        self.edge_colors[index % self.edge_colors.len()]
    }

    fn corner_radius(&self) -> u16 {
        if self.width < self.height {
            self.width / 2
        } else {
            self.height / 2
        }
    }
}

// ─── Build functions ─────────────────────────────────────────────────────────

fn build_single_graph_row_image(
    graph: &Graph,
    image_params: &ImageParams,
    drawing_pixels: &DrawingPixels,
    graph_style: GraphStyle,
    commit_hash: &CommitHash,
) -> GraphRowImage {
    let pos = graph.commit_pos_map[commit_hash];
    let edges = &graph.edges[pos.y];
    let cell_count = graph.max_pos_x + 1;

    calc_graph_row_image(
        pos.x,
        cell_count,
        edges,
        image_params,
        drawing_pixels,
        graph_style,
    )
}

/// Pre-render all unique (commit_x, edges) combinations.
pub fn build_graph_image(
    graph: &Graph,
    image_params: &ImageParams,
    drawing_pixels: &DrawingPixels,
    graph_style: GraphStyle,
) -> GraphImage {
    // Deduplicate by (pos_x, edge_pattern) so shared graph rows are rendered once.
    let mut seen: HashSet<(usize, Vec<Edge>)> = HashSet::new();
    for commit_hash in &graph.commits {
        let pos = &graph.commit_pos_map[commit_hash];
        let edges = graph.edges[pos.y].clone();
        seen.insert((pos.x, edges));
    }

    let cell_count = graph.max_pos_x + 1;

    let images: HashMap<Vec<Edge>, GraphRowImage> = seen
        .into_par_iter()
        .map(|(pos_x, edges)| {
            let row_image = calc_graph_row_image(
                pos_x,
                cell_count,
                &edges,
                image_params,
                drawing_pixels,
                graph_style,
            );
            (edges, row_image)
        })
        .collect();

    GraphImage { images }
}

// ─── DrawingPixels ───────────────────────────────────────────────────────────

type Pixels = HashSet<(i32, i32)>;

/// Pre-computed pixel sets for every edge shape and the commit circle.
#[derive(Debug)]
pub struct DrawingPixels {
    circle: Pixels,
    circle_edge: Pixels,
    vertical_edge: Pixels,
    horizontal_edge: Pixels,
    up_edge: Pixels,
    down_edge: Pixels,
    left_edge: Pixels,
    right_edge: Pixels,
    right_top_edge: Pixels,
    left_top_edge: Pixels,
    right_bottom_edge: Pixels,
    left_bottom_edge: Pixels,
}

impl DrawingPixels {
    pub fn new(image_params: &ImageParams) -> Self {
        let circle = calc_commit_circle_drawing_pixels(image_params);
        let circle_edge = calc_circle_edge_drawing_pixels(image_params);
        let vertical_edge = calc_vertical_edge_drawing_pixels(image_params);
        let horizontal_edge = calc_horizontal_edge_drawing_pixels(image_params);
        let up_edge = calc_up_edge_drawing_pixels(image_params);
        let down_edge = calc_down_edge_drawing_pixels(image_params);
        let left_edge = calc_left_edge_drawing_pixels(image_params);
        let right_edge = calc_right_edge_drawing_pixels(image_params);
        let right_top_edge = calc_right_top_edge_drawing_pixels(image_params);
        let left_top_edge = calc_left_top_edge_drawing_pixels(image_params);
        let right_bottom_edge = calc_right_bottom_edge_drawing_pixels(image_params);
        let left_bottom_edge = calc_left_bottom_edge_drawing_pixels(image_params);

        Self {
            circle,
            circle_edge,
            vertical_edge,
            horizontal_edge,
            up_edge,
            down_edge,
            left_edge,
            right_edge,
            right_top_edge,
            left_top_edge,
            right_bottom_edge,
            left_bottom_edge,
        }
    }
}

// ─── Pixel calculation helpers ───────────────────────────────────────────────

fn calc_commit_circle_drawing_pixels(image_params: &ImageParams) -> Pixels {
    calc_circle_drawing_pixels(image_params, image_params.circle_inner_radius as i32)
}

fn calc_circle_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let inner = calc_circle_drawing_pixels(image_params, image_params.circle_inner_radius as i32);
    let outer = calc_circle_drawing_pixels(image_params, image_params.circle_outer_radius as i32);
    outer.difference(&inner).cloned().collect()
}

fn calc_circle_drawing_pixels(image_params: &ImageParams, radius: i32) -> Pixels {
    // Bresenham's circle algorithm
    let center_x = (image_params.width / 2) as i32;
    let center_y = (image_params.height / 2) as i32;

    let mut x = radius;
    let mut y = 0;
    let mut p = 1 - radius;

    let mut pixels = Pixels::default();

    while x >= y {
        for dx in -x..=x {
            pixels.insert((center_x + dx, center_y + y));
            pixels.insert((center_x + dx, center_y - y));
        }
        for dx in -y..=y {
            pixels.insert((center_x + dx, center_y + x));
            pixels.insert((center_x + dx, center_y - x));
        }

        y += 1;
        if p <= 0 {
            p += 2 * y + 1;
        } else {
            x -= 1;
            p += 2 * y - 2 * x + 1;
        }
    }

    pixels
}

fn calc_vertical_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let center_x = (image_params.width / 2) as i32;
    let line_width = image_params.line_width as i32;
    let x_start = center_x - line_width / 2;

    let mut pixels = Pixels::default();
    for y in 0..image_params.height as i32 {
        for x in x_start..(x_start + line_width) {
            pixels.insert((x, y));
        }
    }
    pixels
}

fn calc_horizontal_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let center_y = (image_params.height / 2) as i32;
    let line_width = image_params.line_width as i32;
    let y_start = center_y - line_width / 2;

    let mut pixels = Pixels::default();
    for y in y_start..(y_start + line_width) {
        for x in 0..image_params.width as i32 {
            pixels.insert((x, y));
        }
    }
    pixels
}

fn calc_up_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let center_x = (image_params.width / 2) as i32;
    let line_width = image_params.line_width as i32;
    let x_start = center_x - line_width / 2;
    let circle_center_y = (image_params.height / 2) as i32;
    let circle_outer_radius = image_params.circle_outer_radius as i32;

    let mut pixels = Pixels::default();
    for y in 0..(circle_center_y - circle_outer_radius) {
        for x in x_start..(x_start + line_width) {
            pixels.insert((x, y));
        }
    }
    pixels
}

fn calc_down_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let center_x = (image_params.width / 2) as i32;
    let line_width = image_params.line_width as i32;
    let x_start = center_x - line_width / 2;
    let circle_center_y = (image_params.height / 2) as i32;
    let circle_outer_radius = image_params.circle_outer_radius as i32;

    let mut pixels = Pixels::default();
    for y in (circle_center_y + circle_outer_radius + 1)..(image_params.height as i32) {
        for x in x_start..(x_start + line_width) {
            pixels.insert((x, y));
        }
    }
    pixels
}

fn calc_left_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let center_y = (image_params.height / 2) as i32;
    let line_width = image_params.line_width as i32;
    let y_start = center_y - line_width / 2;
    let circle_center_x = (image_params.width / 2) as i32;
    let circle_outer_radius = image_params.circle_outer_radius as i32;

    let mut pixels = Pixels::default();
    for y in y_start..(y_start + line_width) {
        for x in 0..(circle_center_x - circle_outer_radius) {
            pixels.insert((x, y));
        }
    }
    pixels
}

fn calc_right_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let center_y = (image_params.height / 2) as i32;
    let line_width = image_params.line_width as i32;
    let y_start = center_y - line_width / 2;
    let circle_center_x = (image_params.width / 2) as i32;
    let circle_outer_radius = image_params.circle_outer_radius as i32;

    let mut pixels = Pixels::default();
    for y in y_start..(y_start + line_width) {
        for x in (circle_center_x + circle_outer_radius + 1)..(image_params.width as i32) {
            pixels.insert((x, y));
        }
    }
    pixels
}

fn calc_right_top_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let (w, h, r) = (
        image_params.width as i32,
        image_params.height as i32,
        image_params.corner_radius() as i32,
    );
    let (x_offset, y_offset) = if w < h {
        (0, r - (h / 2))
    } else {
        ((w / 2) - r, 0)
    };
    calc_corner_edge_drawing_pixels(image_params, 0, h, x_offset, y_offset)
}

fn calc_left_top_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let (w, h, r) = (
        image_params.width as i32,
        image_params.height as i32,
        image_params.corner_radius() as i32,
    );
    let (x_offset, y_offset) = if w < h {
        (0, r - (h / 2))
    } else {
        (r - (w / 2), 0)
    };
    calc_corner_edge_drawing_pixels(image_params, w, h, x_offset, y_offset)
}

fn calc_right_bottom_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let (w, h, r) = (
        image_params.width as i32,
        image_params.height as i32,
        image_params.corner_radius() as i32,
    );
    let (x_offset, y_offset) = if w < h {
        (0, (h / 2) - r)
    } else {
        ((w / 2) - r, 0)
    };
    calc_corner_edge_drawing_pixels(image_params, 0, 0, x_offset, y_offset)
}

fn calc_left_bottom_edge_drawing_pixels(image_params: &ImageParams) -> Pixels {
    let (w, h, r) = (
        image_params.width as i32,
        image_params.height as i32,
        image_params.corner_radius() as i32,
    );
    let (x_offset, y_offset) = if w < h {
        (0, (h / 2) - r)
    } else {
        (r - (w / 2), 0)
    };
    calc_corner_edge_drawing_pixels(image_params, w, 0, x_offset, y_offset)
}

fn calc_corner_edge_drawing_pixels(
    image_params: &ImageParams,
    base_center_x: i32,
    base_center_y: i32,
    x_offset: i32,
    y_offset: i32,
) -> Pixels {
    // Bresenham's circle algorithm for a rounded corner arc
    let curve_center_x = base_center_x;
    let curve_center_y = base_center_y;
    let line_width = image_params.line_width as i32;
    let half_line_width = line_width / 2;
    let adjust = if image_params.line_width.is_multiple_of(2) { 0 } else { 1 };
    let radius_base_length = image_params.corner_radius() as i32;
    let inner_radius = radius_base_length - half_line_width - adjust;
    let outer_radius = radius_base_length + half_line_width;

    let mut x = inner_radius;
    let mut y = 0;
    let mut p = 1 - inner_radius;
    let mut inner_pixels = Pixels::default();

    while x >= y {
        for dx in -x..=x {
            inner_pixels.insert((curve_center_x + dx, curve_center_y + y));
            inner_pixels.insert((curve_center_x + dx, curve_center_y - y));
        }
        for dx in -y..=y {
            inner_pixels.insert((curve_center_x + dx, curve_center_y + x));
            inner_pixels.insert((curve_center_x + dx, curve_center_y - x));
        }
        y += 1;
        if p <= 0 {
            p += 2 * y + 1;
        } else {
            x -= 1;
            p += 2 * y - 2 * x + 1;
        }
    }

    let mut x = outer_radius;
    let mut y = 0;
    let mut p = 1 - outer_radius;
    let mut outer_pixels = Pixels::default();

    while x >= y {
        for dx in -x..=x {
            outer_pixels.insert((curve_center_x + dx, curve_center_y + y));
            outer_pixels.insert((curve_center_x + dx, curve_center_y - y));
        }
        for dx in -y..=y {
            outer_pixels.insert((curve_center_x + dx, curve_center_y + x));
            outer_pixels.insert((curve_center_x + dx, curve_center_y - x));
        }
        y += 1;
        if p <= 0 {
            p += 2 * y + 1;
        } else {
            x -= 1;
            p += 2 * y - 2 * x + 1;
        }
    }

    let mut pixels: Pixels = outer_pixels
        .difference(&inner_pixels)
        .filter(|p| {
            p.0 >= 0
                && p.0 < image_params.width as i32
                && p.1 >= 0
                && p.1 < image_params.height as i32
        })
        .map(|p| (p.0 + x_offset, p.1 + y_offset))
        .collect();

    if image_params.width < image_params.height {
        let (ys, ye) = if y_offset < 0 {
            (base_center_y + y_offset, base_center_y)
        } else {
            (base_center_y, base_center_y + y_offset)
        };
        let center_x = (image_params.width / 2) as i32;
        let x_start = center_x - line_width / 2;
        for x in x_start..(x_start + line_width) {
            for y in ys..ye {
                pixels.insert((x, y));
            }
        }
    }
    if image_params.width > image_params.height {
        let (xs, xe) = if x_offset < 0 {
            (base_center_x + x_offset, base_center_x)
        } else {
            (base_center_x, base_center_x + x_offset)
        };
        let center_y = (image_params.height / 2) as i32;
        let y_start = center_y - line_width / 2;
        for y in y_start..(y_start + line_width) {
            for x in xs..xe {
                pixels.insert((x, y));
            }
        }
    }

    pixels
}

// ─── Row image rendering ─────────────────────────────────────────────────────

fn calc_graph_row_image(
    commit_pos_x: usize,
    cell_count: usize,
    edges: &[Edge],
    image_params: &ImageParams,
    drawing_pixels: &DrawingPixels,
    graph_style: GraphStyle,
) -> GraphRowImage {
    let image_width = (image_params.width as usize * cell_count) as u32;
    let image_height = image_params.height as u32;

    let mut img_buf = image::ImageBuffer::new(image_width, image_height);

    draw_background(&mut img_buf, image_params);
    draw_commit_circle(&mut img_buf, commit_pos_x, image_params, drawing_pixels);

    match graph_style {
        GraphStyle::Rounded => {
            for edge in edges {
                draw_edge(&mut img_buf, edge, image_params, drawing_pixels);
            }
        }
        GraphStyle::Angular => {
            let (vertical_edges, horizontal_edges): (Vec<&Edge>, Vec<&Edge>) = edges
                .iter()
                .partition(|e| e.edge_type.is_vertically_related());
            for edge in vertical_edges {
                draw_edge(&mut img_buf, edge, image_params, drawing_pixels);
            }
            let mut horizontal_edges_map: HashMap<usize, Vec<&Edge>> = HashMap::new();
            for edge in horizontal_edges {
                horizontal_edges_map
                    .entry(edge.color_index)
                    .or_default()
                    .push(edge);
            }
            for edges in horizontal_edges_map.values() {
                draw_diagonal_connected_edge(&mut img_buf, edges, image_params);
            }
        }
    }

    let bytes = build_image(&img_buf, image_width, image_height);
    GraphRowImage { bytes, cell_count }
}

fn draw_background(
    img_buf: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    image_params: &ImageParams,
) {
    if image_params.background_color[3] == 0 {
        return; // transparent — leave pixels zeroed
    }
    for pixel in img_buf.pixels_mut() {
        *pixel = image_params.background_color;
    }
}

fn draw_commit_circle(
    img_buf: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    circle_pos_x: usize,
    image_params: &ImageParams,
    drawing_pixels: &DrawingPixels,
) {
    let x_offset = (circle_pos_x * image_params.width as usize) as i32;
    let color = image_params.edge_color(circle_pos_x);

    for (x, y) in &drawing_pixels.circle {
        let x = (*x + x_offset) as u32;
        let y = *y as u32;
        let pixel = img_buf.get_pixel_mut(x, y);
        *pixel = color;
    }

    if image_params.circle_edge_color[3] == 0 {
        return; // transparent edge ring
    }

    for (x, y) in &drawing_pixels.circle_edge {
        let x = (*x + x_offset) as u32;
        let y = *y as u32;
        let pixel = img_buf.get_pixel_mut(x, y);
        *pixel = image_params.circle_edge_color;
    }
}

fn draw_edge(
    img_buf: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    edge: &Edge,
    image_params: &ImageParams,
    drawing_pixels: &DrawingPixels,
) {
    let pixels = match edge.edge_type {
        EdgeType::Vertical => &drawing_pixels.vertical_edge,
        EdgeType::Horizontal => &drawing_pixels.horizontal_edge,
        EdgeType::Up => &drawing_pixels.up_edge,
        EdgeType::Down => &drawing_pixels.down_edge,
        EdgeType::Left => &drawing_pixels.left_edge,
        EdgeType::Right => &drawing_pixels.right_edge,
        EdgeType::RightTop => &drawing_pixels.right_top_edge,
        EdgeType::RightBottom => &drawing_pixels.right_bottom_edge,
        EdgeType::LeftTop => &drawing_pixels.left_top_edge,
        EdgeType::LeftBottom => &drawing_pixels.left_bottom_edge,
    };

    let x_offset = (edge.pos.x * image_params.width as usize) as i32;
    let color = image_params.edge_color(edge.color_index);

    for (x, y) in pixels {
        let x = (*x + x_offset) as u32;
        let y = *y as u32;
        let pixel = img_buf.get_pixel_mut(x, y);
        *pixel = color;
    }
}

fn draw_diagonal_connected_edge(
    img_buf: &mut image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
    edges: &[&Edge],
    image_params: &ImageParams,
) {
    let corner_edges = edges.iter().filter(|e| {
        matches!(
            e.edge_type,
            EdgeType::RightBottom | EdgeType::LeftBottom | EdgeType::RightTop | EdgeType::LeftTop
        )
    });

    for corner_edge in corner_edges {
        let expected_side_edge_type = match corner_edge.edge_type {
            EdgeType::RightBottom | EdgeType::RightTop => EdgeType::Right,
            EdgeType::LeftBottom | EdgeType::LeftTop => EdgeType::Left,
            _ => unreachable!("unexpected edge type for corner edge"),
        };
        let side_edge_opt = edges
            .iter()
            .find(|e| e.edge_type == expected_side_edge_type);

        if let Some(side_edge) = side_edge_opt {
            let line_width_f64 = image_params.line_width as f64;
            let line_width_i32 = image_params.line_width as i32;

            let y_offset = if image_params.width == image_params.height {
                image_params.height as f64 / 10.0
            } else {
                image_params.height as f64 / 2.0 - image_params.corner_radius() as f64
            };

            match corner_edge.edge_type {
                EdgeType::RightBottom | EdgeType::LeftBottom => {
                    let start_pos_center = Point::new(
                        (side_edge.pos.x * image_params.width as usize) as f64
                            + (image_params.width as f64 / 2.0),
                        image_params.height as f64 / 2.0,
                    );
                    let end_pos_center = Point::new(
                        (corner_edge.pos.x * image_params.width as usize) as f64
                            + (image_params.width as f64 / 2.0),
                        y_offset,
                    );

                    let line_vec = end_pos_center - start_pos_center;
                    let unit_vec = line_vec.normalize();
                    let normal_vec = unit_vec.perpendicular();

                    let line_start =
                        start_pos_center + unit_vec * (image_params.circle_outer_radius as f64);
                    let line_start_1 = line_start + normal_vec * (line_width_f64 / 2.0);
                    let line_start_2 = line_start - normal_vec * (line_width_f64 / 2.0);

                    let half_width = line_width_f64 / 2.0;
                    let slope = unit_vec.y / unit_vec.x;

                    let vertical_left_x = end_pos_center.x - half_width;
                    let vertical_right_x = end_pos_center.x + half_width;

                    let corner_1 = Point::new(
                        vertical_right_x,
                        line_start_1.y + slope * (vertical_right_x - line_start_1.x),
                    );
                    let corner_2 = Point::new(
                        vertical_left_x,
                        line_start_2.y + slope * (vertical_left_x - line_start_2.x),
                    );

                    let vertices = [line_start_1, corner_1, corner_2, line_start_2];

                    let (min_x, min_y, max_x, max_y) = bounding_box_u32(&vertices);
                    for y in min_y..max_y {
                        for x in min_x..max_x {
                            if x < img_buf.width() && y < img_buf.height() {
                                let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
                                if p.is_inside_polygon(&vertices) {
                                    let pixel = img_buf.get_pixel_mut(x, y);
                                    let color = image_params.edge_color(side_edge.color_index);
                                    *pixel = color;
                                }
                            }
                        }
                    }

                    let y_end = corner_1.y.max(corner_2.y) as u32;
                    let end_center_x_i32 = end_pos_center.x as i32;
                    let x_start = end_center_x_i32 - line_width_i32 / 2;
                    for y in 0..y_end {
                        for i in 0..line_width_i32 {
                            let x = (x_start + i) as u32;
                            if x < img_buf.width() && y < img_buf.height() {
                                let pixel = img_buf.get_pixel_mut(x, y);
                                let color = image_params.edge_color(side_edge.color_index);
                                *pixel = color;
                            }
                        }
                    }
                }
                EdgeType::RightTop | EdgeType::LeftTop => {
                    let start_pos_center = Point::new(
                        (side_edge.pos.x * image_params.width as usize) as f64
                            + (image_params.width as f64 / 2.0),
                        image_params.height as f64 / 2.0,
                    );
                    let end_pos_center = Point::new(
                        (corner_edge.pos.x * image_params.width as usize) as f64
                            + (image_params.width as f64 / 2.0),
                        image_params.height as f64 - y_offset,
                    );

                    let line_vec = end_pos_center - start_pos_center;
                    let unit_vec = line_vec.normalize();
                    let normal_vec = unit_vec.perpendicular();

                    let line_start =
                        start_pos_center + unit_vec * (image_params.circle_outer_radius as f64);
                    let line_start_1 = line_start + normal_vec * (line_width_f64 / 2.0);
                    let line_start_2 = line_start - normal_vec * (line_width_f64 / 2.0);

                    let half_width = line_width_f64 / 2.0;
                    let slope = unit_vec.y / unit_vec.x;

                    let vertical_left_x = end_pos_center.x - half_width;
                    let vertical_right_x = end_pos_center.x + half_width;

                    let corner_1 = Point::new(
                        vertical_left_x,
                        line_start_1.y + slope * (vertical_left_x - line_start_1.x),
                    );
                    let corner_2 = Point::new(
                        vertical_right_x,
                        line_start_2.y + slope * (vertical_right_x - line_start_2.x),
                    );

                    let vertices = [line_start_1, corner_1, corner_2, line_start_2];

                    let (min_x, min_y, max_x, max_y) = bounding_box_u32(&vertices);
                    for y in min_y..max_y {
                        for x in min_x..max_x {
                            if x < img_buf.width() && y < img_buf.height() {
                                let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
                                if p.is_inside_polygon(&vertices) {
                                    let pixel = img_buf.get_pixel_mut(x, y);
                                    let color = image_params.edge_color(side_edge.color_index);
                                    *pixel = color;
                                }
                            }
                        }
                    }

                    let y_start = corner_1.y.min(corner_2.y) as u32;
                    let end_center_x_i32 = end_pos_center.x as i32;
                    let x_start = end_center_x_i32 - line_width_i32 / 2;
                    for y in (y_start + 1)..image_params.height as u32 {
                        for i in 0..line_width_i32 {
                            let x = (x_start + i) as u32;
                            if x < img_buf.width() && y < img_buf.height() {
                                let pixel = img_buf.get_pixel_mut(x, y);
                                let color = image_params.edge_color(side_edge.color_index);
                                *pixel = color;
                            }
                        }
                    }
                }
                _ => unreachable!("unexpected edge type for corner edge"),
            }
        }
    }
}

fn build_image(img_buf: &[u8], image_width: u32, image_height: u32) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::write_buffer_with_format(
        &mut bytes,
        img_buf,
        image_width,
        image_height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
    bytes.into_inner()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_colors() -> ImageColors {
        ImageColors::default()
    }

    #[test]
    fn image_colors_default_has_six_branches() {
        let colors = default_colors();
        assert_eq!(colors.branches.len(), 6);
    }

    #[test]
    fn image_colors_default_transparent_edge_and_bg() {
        let colors = default_colors();
        assert_eq!(colors.edge[3], 0, "edge should be transparent");
        assert_eq!(colors.background[3], 0, "background should be transparent");
    }

    #[test]
    fn image_params_new_double() {
        let colors = default_colors();
        let params = ImageParams::new(&colors, CellWidthType::Double);
        assert_eq!(params.width, 50);
        assert_eq!(params.height, 50);
        assert_eq!(params.line_width, 5);
        assert_eq!(params.circle_inner_radius, 10);
        assert_eq!(params.circle_outer_radius, 13);
        assert_eq!(params.edge_colors.len(), colors.branches.len());
    }

    #[test]
    fn image_params_new_single() {
        let colors = default_colors();
        let params = ImageParams::new(&colors, CellWidthType::Single);
        assert_eq!(params.width, 25);
        assert_eq!(params.height, 50);
        assert_eq!(params.line_width, 3);
    }

    #[test]
    fn image_params_edge_color_cycles() {
        let colors = default_colors();
        let params = ImageParams::new(&colors, CellWidthType::Double);
        let n = colors.branches.len();
        assert_eq!(params.edge_color(0), params.edge_color(n));
    }

    #[test]
    fn drawing_pixels_non_empty() {
        let colors = default_colors();
        let params = ImageParams::new(&colors, CellWidthType::Double);
        let dp = DrawingPixels::new(&params);
        assert!(!dp.circle.is_empty());
        assert!(!dp.vertical_edge.is_empty());
        assert!(!dp.horizontal_edge.is_empty());
        assert!(!dp.up_edge.is_empty());
        assert!(!dp.down_edge.is_empty());
    }

    #[test]
    fn preload_range_populates_cache() {
        use crate::{
            git::{Commit, CommitHash, Repository},
            graph::calc_graph,
        };
        use std::collections::HashMap;

        // Build a tiny linear graph: A → B → C (newest first)
        let commit_data: Vec<(&str, Vec<&str>)> = vec![
            ("A", vec!["B"]),
            ("B", vec!["C"]),
            ("C", vec![]),
        ];

        let commit_list: Vec<Commit> = commit_data
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

        let repo = Repository::new(commit_hashes, commit_map, children_map);
        let graph = calc_graph(&repo);

        let colors = default_colors();
        let mut mgr = GraphImageManager::new(
            &graph,
            &colors,
            CellWidthType::Single,
            GraphStyle::Angular,
            crate::protocol::ImageProtocol::Iterm2,
            false, // don't preload at construction
        );

        // Nothing loaded yet.
        assert!(mgr.encoded_image_map.is_empty());

        // Preload the first two commits.
        mgr.preload_range(0, 2);

        let h_a = CommitHash::from("A");
        let h_b = CommitHash::from("B");
        let h_c = CommitHash::from("C");

        assert!(mgr.encoded_image_map.contains_key(&h_a));
        assert!(mgr.encoded_image_map.contains_key(&h_b));
        // Third commit not yet loaded.
        assert!(!mgr.encoded_image_map.contains_key(&h_c));

        // Extending the range loads the third commit without re-rendering the first two.
        mgr.preload_range(0, 3);
        assert!(mgr.encoded_image_map.contains_key(&h_c));
    }

    #[test]
    fn graph_row_image_encode_iterm2() {
        use crate::protocol::ImageProtocol;
        let row = GraphRowImage {
            bytes: b"fake-png".to_vec(),
            cell_count: 3,
        };
        let encoded = row.encode(CellWidthType::Double, ImageProtocol::Iterm2);
        assert!(encoded.contains("1337"));
        // cell_count * 2 = 6 cells for Double
        assert!(encoded.contains("width=6"));
    }

    #[test]
    fn graph_row_image_encode_kitty() {
        use crate::protocol::ImageProtocol;
        let row = GraphRowImage {
            bytes: b"fake-png".to_vec(),
            cell_count: 3,
        };
        let encoded = row.encode(CellWidthType::Single, ImageProtocol::Kitty);
        assert!(encoded.contains("a=T"));
        // cell_count * 1 = 3 cells for Single
        assert!(encoded.contains("c=3"));
    }
}
