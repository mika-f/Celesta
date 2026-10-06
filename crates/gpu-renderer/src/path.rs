use crate::draw::encode_paint;
use crate::transform::LayerState;
use celesta_composition::{BlendMode, LineCap, LineJoin, Paint, PathCommand, Stroke};
use celesta_renderer::{FlattenedPath, LineSegment, PathShape};

/// A path layer whose outline `prepare_layer` left for `outline_paths`,
/// holding what is needed to outline it and to replace `item`.
pub(crate) struct PendingPath {
    /// Index of its `PreparedItem::PendingPath` in the frame's items.
    pub(crate) item: usize,
    pub(crate) commands: Vec<PathCommand>,
    pub(crate) fill: Option<Paint>,
    pub(crate) stroke: Option<Stroke>,
    pub(crate) line_cap: LineCap,
    pub(crate) line_join: LineJoin,
    pub(crate) miter_limit: f64,
    pub(crate) state: LayerState,
    pub(crate) blend_mode: BlendMode,
}

impl PendingPath {
    pub(crate) fn shape(&self) -> PathShape<'_> {
        PathShape {
            commands: &self.commands,
            fill: self.fill.as_ref(),
            stroke: self.stroke.as_ref(),
            line_cap: self.line_cap,
            line_join: self.line_join,
            miter_limit: self.miter_limit,
        }
    }
}

/// The pixels of a path's region that share one list of edges in `paths`.
/// Mirrors `PATH_TILE_COLUMNS` and `PATH_TILE_ROWS` in `layer.wgsl`.
pub(crate) const PATH_TILE_COLUMNS: u32 = 8;
pub(crate) const PATH_TILE_ROWS: u32 = 8;

/// A path drawn by `layer.wgsl`'s `path_texel`, which shades the coverage of
/// its outlines' edges (`celesta_renderer::flatten_path`) into the pixels
/// `celesta_renderer::rasterize_path` would have produced: texel for texel,
/// as a quad per `PATH_TILE_COLUMNS`x`PATH_TILE_ROWS` tile of its region
/// that either outline covers any of.
pub(crate) struct ShadedPath {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The index of its first entry in `paths`:
    ///
    /// - the inverse transform's a, b, c, d (region pixels to layer
    ///   coordinates, for gradients)
    /// - the inverse transform's tx, ty; the number of tile columns; how many
    ///   entries each tile drawn has, one per outline
    /// - which of a tile's entries is the fill's, then the stroke's (-1
    ///   without one); the index of the first tile's; unused
    ///
    /// then the tiles drawn, each outline's entry for each: the tile's index
    /// in the region (row by row), the index and count of the edges the
    /// outline has there, and its backdrop (see `bin_tiles`). The edges, x0,
    /// y0, x1, y1 in region pixels, follow. Indices are whole numbers in f32,
    /// exact below 2^24 entries, which the storage buffer binding limit keeps
    /// them under.
    pub(crate) base: f32,
    /// How many tiles it draws.
    pub(crate) tiles: u32,
    /// See `encode_paint`.
    pub(crate) fill: [f32; 4],
    pub(crate) stroke: [f32; 4],
}

// ponytail: cap the quadratic scanline loop; denser tiles use the existing
// CPU rasterizer. Subdivide tiles if measurements justify a denser GPU path.
pub(crate) const MAX_PATH_TILE_EDGES: usize = 64;

/// Keep all shader indices exact in f32 even with larger device limits.
pub(crate) fn path_entry_limit(limits: &wgpu::Limits) -> usize {
    (limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size)
        / 16)
        .min(1 << 24) as usize
}

impl ShadedPath {
    /// Returns None before modifying either buffer when this layer needs
    /// CPU rasterization to bound shader work or fit the frame's buffer.
    #[cfg(test)]
    pub(crate) fn new(
        path: &FlattenedPath,
        paints: &mut Vec<[f32; 4]>,
        entries: &mut Vec<[f32; 4]>,
        entry_limit: usize,
    ) -> Option<Self> {
        let built = PathEntries::build(path, entry_limit)?;
        let base = entries.len();
        if !built.fits(base, entry_limit) {
            return None;
        }
        entries.resize(base + built.len(), [0.0; 4]);
        built.place(base, &mut entries[base..]);
        Some(built.shaded(path, base, paints))
    }
}

/// A path's entries in `paths` (see `ShadedPath::base`), built on any
/// thread with its indices counted from its own first entry, then placed
/// in the frame's buffer once the paths before it are.
pub(crate) struct PathEntries {
    width: u32,
    height: u32,
    tiles: u32,
    stride: usize,
    entries: Vec<[f32; 4]>,
}

impl PathEntries {
    /// None when a tile has more edges than the shader takes, or the path
    /// needs more than `entry_limit` entries (the most the frame's buffer
    /// can hold), so it needs CPU rasterization.
    pub(crate) fn build(path: &FlattenedPath, entry_limit: usize) -> Option<Self> {
        let inverse = path.inverse;
        let columns = path.width.div_ceil(PATH_TILE_COLUMNS);
        let outlines: Vec<_> = [&path.fill, &path.stroke]
            .into_iter()
            .map(|outline| {
                outline
                    .as_ref()
                    .map(|(_, edges)| bin_tiles(edges, path.width, path.height))
            })
            .collect();
        if outlines.iter().flatten().any(|bins| {
            bins.tiles
                .iter()
                .any(|tile| tile.edges.len() > MAX_PATH_TILE_EDGES)
        }) {
            return None;
        }
        // Each tile's entries: the fill's, then the stroke's, if it has them.
        let mut stride = 0;
        let mut slots = [-1.0; 2];
        for (slot, outline) in slots.iter_mut().zip(&outlines) {
            if outline.is_some() {
                *slot = stride as f32;
                stride += 1;
            }
        }
        // Every tile either outline covers, in order.
        let mut tiles: Vec<u32> = outlines
            .iter()
            .flatten()
            .flat_map(|bins| bins.tiles.iter().map(|tile| tile.index))
            .collect();
        tiles.sort_unstable();
        tiles.dedup();
        let edge_entries: usize = outlines.iter().flatten().map(|bins| bins.edges.len()).sum();
        let first_tile = 3;
        let required = first_tile + tiles.len() * stride + edge_entries;
        if required > entry_limit {
            return None;
        }
        let mut entries = Vec::with_capacity(required);
        entries.push([inverse.a, inverse.b, inverse.c, inverse.d].map(|value| value as f32));
        entries.push([
            inverse.tx as f32,
            inverse.ty as f32,
            columns as f32,
            stride as f32,
        ]);
        entries.push([slots[0], slots[1], first_tile as f32, 0.0]);
        entries.resize(first_tile + tiles.len() * stride, [0.0; 4]);
        for (slot, bins) in outlines.iter().flatten().enumerate() {
            let edges = entries.len();
            let mut own = bins.tiles.iter().peekable();
            for (position, &index) in tiles.iter().enumerate() {
                let entry = &mut entries[first_tile + position * stride + slot];
                *entry = [index as f32, 0.0, 0.0, 0.0];
                if let Some(tile) = own.next_if(|tile| tile.index == index) {
                    *entry = [
                        index as f32,
                        (edges + tile.edges.start) as f32,
                        tile.edges.len() as f32,
                        tile.backdrop as f32,
                    ];
                }
            }
            entries.extend_from_slice(&bins.edges);
        }
        Some(Self {
            width: path.width,
            height: path.height,
            tiles: tiles.len() as u32,
            stride,
            entries,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// How many tiles it draws.
    pub(crate) const fn tiles(&self) -> u32 {
        self.tiles
    }

    /// Whether it fits a buffer of `entry_limit` entries from `base` on.
    pub(crate) fn fits(&self, base: usize, entry_limit: usize) -> bool {
        self.len() <= entry_limit.saturating_sub(base)
    }

    /// Writes its entries into `out`, the `len()` entries of the frame's
    /// buffer from `base` on, with their indices counted from the buffer's
    /// start.
    pub(crate) fn place(&self, base: usize, out: &mut [[f32; 4]]) {
        out.copy_from_slice(&self.entries);
        let shift = |index: f32| (index as usize + base) as f32;
        out[2][2] = shift(out[2][2]);
        let tiles = &mut out[3..3 + self.tiles as usize * self.stride];
        // A tile an outline covers has edges or a backdrop; only those
        // point at edges.
        for entry in tiles {
            if entry[2] != 0.0 || entry[3] != 0.0 {
                entry[1] = shift(entry[1]);
            }
        }
    }

    /// The draw of `path` once its entries are placed at `base`.
    pub(crate) fn shaded(
        &self,
        path: &FlattenedPath,
        base: usize,
        paints: &mut Vec<[f32; 4]>,
    ) -> ShadedPath {
        ShadedPath {
            width: self.width,
            height: self.height,
            base: base as f32,
            tiles: self.tiles,
            fill: encode_paint(path.fill.as_ref().map(|(paint, _)| paint), paints),
            stroke: encode_paint(path.stroke.as_ref().map(|(paint, _)| paint), paints),
        }
    }
}

/// Up to how many pieces `bin_tiles` sorts with `sort_pieces_in_place`.
const SMALL_PIECE_COUNT: usize = 64;

/// Sorts a region's pieces by tile: first into rows, keeping their order,
/// then each row's few pieces by column. A region has few rows, and a row
/// few pieces, so this costs a fraction of sorting them all by comparison.
/// The pieces are counted into their rows.
pub(crate) fn count_pieces_into_rows(
    pieces: Vec<(usize, [f32; 4])>,
    rows: usize,
    columns: usize,
) -> Vec<(usize, [f32; 4])> {
    let mut row_starts = vec![0_usize; rows + 1];
    for &(tile, _) in &pieces {
        row_starts[tile / columns + 1] += 1;
    }
    for row in 0..rows {
        row_starts[row + 1] += row_starts[row];
    }
    let mut sorted = vec![(0, [0.0; 4]); pieces.len()];
    let mut next = row_starts.clone();
    for piece in pieces {
        let row = piece.0 / columns;
        sorted[next[row]] = piece;
        next[row] += 1;
    }
    for row in row_starts.windows(2) {
        sorted[row[0]..row[1]].sort_unstable_by_key(|(tile, _)| *tile);
    }
    sorted
}

/// Sorts like `count_pieces_into_rows`, into the same order, but in place:
/// for a small path's few pieces, the standard library's stable sort into
/// rows needs no heap memory, and the counts would cost more than it.
pub(crate) fn sort_pieces_in_place(
    mut pieces: Vec<(usize, [f32; 4])>,
    columns: usize,
) -> Vec<(usize, [f32; 4])> {
    pieces.sort_by_key(|(tile, _)| tile / columns);
    for row in pieces.chunk_by_mut(|a, b| a.0 / columns == b.0 / columns) {
        row.sort_unstable_by_key(|(tile, _)| *tile);
    }
    pieces
}

/// An outline's edges sorted into the tiles of its region that it covers
/// any of: those its edges reach, and those inside it.
pub(crate) struct TileBins {
    /// In order of their index.
    pub(crate) tiles: Vec<CoveredTile>,
    pub(crate) edges: Vec<[f32; 4]>,
}

pub(crate) struct CoveredTile {
    /// Row by row.
    pub(crate) index: u32,
    /// The winding just left of the tile's top-left corner.
    pub(crate) backdrop: i32,
    /// What it needs of `TileBins::edges`.
    pub(crate) edges: std::ops::Range<usize>,
}

/// Sorts `edges` into the `PATH_TILE_COLUMNS`x`PATH_TILE_ROWS` tiles of a
/// `width`x`height` region.
///
/// A tile gets only the parts of `edges` at or right of its left side, cut
/// to its rows. What lies left of it is summed up by its backdrop, plus a
/// vertical edge down the tile's left side from wherever an edge crosses
/// it, since the winding along that side only changes there. A tile no edge
/// reaches is uniformly covered or not, as its backdrop says, and costs the
/// shader no edges at all.
pub(crate) fn bin_tiles(edges: &[LineSegment], width: u32, height: u32) -> TileBins {
    let columns = width.div_ceil(PATH_TILE_COLUMNS) as usize;
    let rows = height.div_ceil(PATH_TILE_ROWS) as usize;
    let tile_width = PATH_TILE_COLUMNS as f32;
    let tile_height = PATH_TILE_ROWS as f32;
    let column_of = |x: f32| ((x / tile_width).floor().max(0.0) as usize).min(columns - 1);
    let mut pieces: Vec<(usize, [f32; 4])> = Vec::with_capacity(edges.len() * 2);
    // Where along a tile row the backdrop changes, and by how much.
    let mut backdrops: Vec<(usize, i32)> = Vec::new();
    for edge in edges {
        let LineSegment { x0, y0, x1, y1 } = *edge;
        if y0 == y1 {
            // Its crossings of tile sides, where it does not sit on a tile
            // row's top (whose backdrop already counts what it connects).
            let row = (y0 / tile_height).floor() as usize;
            if row >= rows || y0 == row as f32 * tile_height {
                continue;
            }
            let bottom = (row + 1) as f32 * tile_height;
            let direction = if x0 > x1 { [y0, bottom] } else { [bottom, y0] };
            let (low, high) = (x0.min(x1), x0.max(x1));
            for column in column_of(low) + 1..=column_of(high) {
                let side = column as f32 * tile_width;
                if low < side && side <= high {
                    pieces.push((
                        row * columns + column,
                        [side, direction[0], side, direction[1]],
                    ));
                }
            }
            continue;
        }
        let first_row = ((y0.min(y1) / tile_height).floor() as usize).min(rows - 1);
        let last_row = ((y0.max(y1) / tile_height).ceil() as usize).clamp(first_row + 1, rows);
        for row in first_row..last_row {
            let (top, bottom) = (row as f32 * tile_height, (row + 1) as f32 * tile_height);
            // The edge within the row, in its own direction.
            let at = |y: f32| x0 + (x1 - x0) * ((y - y0) / (y1 - y0));
            let clip = |x: f32, y: f32| {
                if y < top {
                    (at(top), top)
                } else if y > bottom {
                    (at(bottom), bottom)
                } else {
                    (x, y)
                }
            };
            let (ax, ay) = clip(x0, y0);
            let (bx, by) = clip(x1, y1);
            if ay == by {
                continue;
            }
            let (top_x, low, high) = if ay < by { (ax, ax, bx) } else { (bx, bx, ax) };
            let (low, high) = (low.min(high), low.max(high));
            let direction = if by > ay { 1 } else { -1 };
            let tiles = row * columns..(row + 1) * columns;
            // Where the edge starts on the row's top, the tiles whose left
            // side is right of it count it in their backdrop.
            if ay.min(by) == top {
                let first = (top_x / tile_width).floor() as usize + 1;
                if first < columns {
                    backdrops.push((tiles.start + first, direction));
                }
            }
            for column in column_of(low)..=column_of(high) {
                let side = column as f32 * tile_width;
                let tile = tiles.start + column;
                if low >= side {
                    pieces.push((tile, [ax, ay, bx, by]));
                    continue;
                }
                // Cut where it crosses the tile's left side: the part left
                // of it is the backdrop's business, plus a vertical edge
                // from the crossing down the side.
                let t = (side - ax) / (bx - ax);
                let cross = if t <= 0.0 {
                    ay
                } else if t >= 1.0 {
                    by
                } else {
                    ay + (by - ay) * t
                };
                let piece = if ax < side {
                    [side, cross, bx, by]
                } else {
                    [ax, ay, side, cross]
                };
                if piece[1] != piece[3] {
                    pieces.push((tile, piece));
                }
                if cross < bottom {
                    // Leaving the left side downwards (left part above) takes
                    // the edge's winding away below the crossing; entering
                    // it adds it.
                    let leaves = (ax < side) == (ay < by);
                    let sign = if leaves { -direction } else { direction };
                    let side_edge = if sign > 0 {
                        [side, cross, side, bottom]
                    } else {
                        [side, bottom, side, cross]
                    };
                    pieces.push((tile, side_edge));
                }
            }
        }
    }
    let pieces = if pieces.len() <= SMALL_PIECE_COUNT {
        sort_pieces_in_place(pieces, columns)
    } else {
        count_pieces_into_rows(pieces, rows, columns)
    };
    backdrops.sort_unstable_by_key(|(tile, _)| *tile);
    let mut bins = TileBins {
        tiles: Vec::new(),
        edges: Vec::with_capacity(pieces.len()),
    };
    let mut pieces = pieces.into_iter().peekable();
    let mut backdrops = backdrops.into_iter().peekable();
    for row in 0..rows {
        let end = (row + 1) * columns;
        let mut backdrop = 0;
        let mut tile = row * columns;
        while tile < end {
            while let Some((_, change)) = backdrops.next_if(|(index, _)| *index == tile) {
                backdrop += change;
            }
            let start = bins.edges.len();
            while let Some((_, piece)) = pieces.next_if(|(index, _)| *index == tile) {
                bins.edges.push(piece);
            }
            let edges = start..bins.edges.len();
            if backdrop != 0 || !edges.is_empty() {
                bins.tiles.push(CoveredTile {
                    index: tile as u32,
                    backdrop,
                    edges,
                });
            }
            tile += 1;
            if backdrop == 0 {
                // Nothing to draw up to the next tile with edges or a change.
                let next_piece = pieces.peek().map_or(end, |(index, _)| *index);
                let next_change = backdrops.peek().map_or(end, |(index, _)| *index);
                tile = tile.max(next_piece.min(next_change).min(end));
            }
        }
    }
    bins
}
