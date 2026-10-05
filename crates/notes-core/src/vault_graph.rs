//! The vault graph for display: the filter and the layout are **in one
//! place** for everyone who draws it:
//!
//! - the graph page and the home page of the client (`POST /api/graph/layout`);
//! - a note, `#vault-graph(...)` from `baluk/graph.typ`: the library reads
//!   the virtual file `/_vault/graph/<filter>.json` (served by the provider
//!   [`GraphData`] from the registry [`crate::vault_data`]), CeTZ draws the
//!   graph in PDF and HTML, and the client animates it by the same
//!   coordinates.
//!
//! The layout is a deterministic force model: repulsion within a radius via
//! a grid of neighbours, edges are springs, a weak pull to the center; then
//! labelled nodes are pushed apart until the labels stop overlapping. The
//! same graph gives the same picture.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::graph::{ChapterOf, Edge, Graph, Node, Snapshot, SourceIndex};
use crate::vault::{NoteKind, Vault};
use crate::vault_data::DataProvider;

/// The group of notes in the vault root (interface text, also a value of the
/// `folders` filter in `baluk/graph.typ`).
pub const ROOT_GROUP: &str = "в корне";

/// Label font size (in layout units), the same as when drawing.
pub const LABEL_SIZE: f64 = 11.0;
/// Gap between the circle and its label (in layout units).
pub const LABEL_GAP: f64 = 2.0;
/// Gap between node rectangles after pushing apart.
const PAD: f64 = 4.0;
/// The desired edge length.
const EDGE: f64 = 70.0;

/// The group of a node: its top-level folder.
pub fn group_of(id: &str) -> &str {
    id.split_once('/').map_or(ROOT_GROUP, |(g, _)| g)
}

/// The note of a node: for a chapter, its book.
fn node_note(n: &Node) -> &str {
    n.chapter.as_ref().map_or(&n.id, |c| &c.book)
}

/// The group of a node: for a chapter, the group of its book.
fn node_group(n: &Node) -> &str {
    group_of(node_note(n))
}

/// What to show. Empty `folders` - all folders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct GraphFilter {
    /// Only these groups (top-level folders; the root is [`ROOT_GROUP`]).
    pub folders: Vec<String>,
    /// Only notes in this folder and its subfolders (path from the root: `Мат/Анализ`).
    pub folder: Option<String>,
    /// Hidden groups.
    pub hidden: Vec<String>,
    /// Only notes with this tag.
    pub tag: Option<String>,
    /// Show missing notes (linked to but not written).
    pub missing: bool,
    /// Show notes without links.
    pub orphans: bool,
    /// Only the neighbours of this note within `depth` steps (both ways);
    /// the note itself stays even if another filter would hide it.
    pub around: Option<String>,
    /// Steps from `around`.
    pub depth: u32,
    /// Books as a root with chapters around it, not one node.
    pub chapters: bool,
    /// Layout forces (view settings on the graph page); a graph in a note
    /// uses the defaults.
    pub forces: Forces,
}

impl Default for GraphFilter {
    fn default() -> Self {
        Self {
            folders: vec![],
            folder: None,
            hidden: vec![],
            tag: None,
            missing: true,
            orphans: true,
            around: None,
            depth: 1,
            chapters: false,
            forces: Forces::default(),
        }
    }
}

/// Layout forces in percent of the normal ones; by default folders hold
/// together a little (`clusters` 5 %, user's decision), the rest is normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct Forces {
    /// How pronounced folders are: the nodes of a folder hold together,
    /// other folders repel more (a gap between folders); 0 - folders hold
    /// together only by their starting places and links.
    pub clusters: u32,
    /// Pull to the center: less - a roomier graph.
    pub center: u32,
    /// Node repulsion.
    pub repel: u32,
    /// Pull of linked nodes (edge stiffness).
    pub links: u32,
}

impl Default for Forces {
    fn default() -> Self {
        Self { clusters: 5, center: 100, repel: 100, links: 100 }
    }
}

impl Forces {
    fn scale(percent: u32) -> f64 {
        f64::from(percent) / 100.0
    }
}

/// A node in its place.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlacedNode {
    /// Node id ([`Node::id`]).
    pub id: String,
    /// `None`: the note does not exist.
    pub kind: Option<NoteKind>,
    /// A book chapter: the book opens at this chapter.
    pub chapter: Option<ChapterOf>,
    /// Label: the note title.
    pub name: String,
    /// Group ([`group_of`]).
    pub group: String,
    /// Center x.
    pub x: f64,
    /// Center y.
    pub y: f64,
    /// Circle radius: a book is larger, links add to it.
    pub r: f64,
    /// Links in the shown graph.
    pub degree: usize,
}

/// A graph ready to draw.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GraphLayout {
    /// Placed nodes.
    pub nodes: Vec<PlacedNode>,
    /// Edges between them.
    pub edges: Vec<Edge>,
    /// All groups of the vault in order, for colors: a filter does not recolor nodes.
    pub groups: Vec<String>,
    /// Bounds of the drawing with labels: `[x0, y0, x1, y1]`.
    pub bounds: [f64; 4],
    /// The note in the center ("neighbours of a note").
    pub center: Option<String>,
}

/// Notes at most `depth` edges from `start` (direction does not matter).
pub fn neighbourhood(graph: &Graph, start: &str, depth: u32) -> HashSet<String> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &graph.edges {
        adj.entry(&e.from).or_default().push(&e.to);
        adj.entry(&e.to).or_default().push(&e.from);
    }
    let mut seen = HashSet::from([start.to_owned()]);
    let mut layer = vec![start];
    for _ in 0..depth {
        let mut next = Vec::new();
        for id in layer {
            for m in adj.get(id).into_iter().flatten() {
                if seen.insert((*m).to_owned()) {
                    next.push(*m);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        layer = next;
    }
    seen
}

/// The subgraph by the filter. `has_tag(node, tag)`: whether the note has
/// the tag (a book - or any of its chapters; a chapter - it or its book).
pub fn filter(graph: &Graph, has_tag: impl Fn(&Node, &str) -> bool, f: &GraphFilter) -> Graph {
    let center = f.around.as_deref();
    let near = center.map(|c| neighbourhood(graph, c, f.depth));
    let keep = |n: &Node| {
        let group = node_group(n);
        Some(n.id.as_str()) == center
            || (near.as_ref().is_none_or(|near| near.contains(&n.id))
                && (f.folders.is_empty() || f.folders.iter().any(|g| g == group))
                && f.folder
                    .as_ref()
                    .is_none_or(|p| node_note(n).strip_prefix(p.as_str()).is_some_and(|r| r.starts_with('/')))
                && !f.hidden.iter().any(|g| g == group)
                && (f.missing || n.kind.is_some())
                && f.tag.as_ref().is_none_or(|t| has_tag(n, t)))
    };
    let mut nodes: Vec<_> = graph.nodes.iter().filter(|n| keep(n)).cloned().collect();
    let ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let edges: Vec<_> =
        graph.edges.iter().filter(|e| ids.contains(e.from.as_str()) && ids.contains(e.to.as_str())).cloned().collect();
    if !f.orphans {
        let linked: HashSet<&str> = edges.iter().flat_map(|e| [e.from.as_str(), e.to.as_str()]).collect();
        nodes.retain(|n| linked.contains(n.id.as_str()) || Some(n.id.as_str()) == center);
    }
    Graph { nodes, edges }
}

/// A node's place: the circle radius and the width of the label under it.
#[derive(Debug, Clone, Copy)]
pub struct NodeBox {
    /// Circle radius.
    pub r: f64,
    /// Label width.
    pub label: f64,
}

/// Label width by the number of characters (no font measuring: the layout is the same everywhere).
#[expect(clippy::cast_precision_loss, reason = "a label has tens of characters")]
pub fn label_width(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * size * 0.58
}

/// The rectangle of a node with its label: `[left, top, right, bottom]`.
pub fn box_rect((x, y): (f64, f64), b: NodeBox) -> [f64; 4] {
    let half = b.r.max(b.label / 2.0);
    [x - half, y - b.r, x + half, y + b.r + LABEL_GAP + LABEL_SIZE * 1.2]
}

/// Layout: node coordinates in the order of `n` nodes, edges are pairs of
/// indices.
///
/// Forces: repulsion of all pairs `EDGE²/d` (Barnes-Hut: a far quadtree
/// cell is one body at its center of mass), edge springs towards the length
/// [`EDGE`], a pull to the center. The pull is tuned so that a node gets
/// about [`AREA`] of area at any number of nodes: a big graph neither
/// collapses into a mush of labels nor spreads out. The start is
/// [`initial_positions`]. No randomness and the same order of sums - the
/// same input gives the same coordinates bit for bit.
///
/// `groups` is the group (folder) number of each node, for
/// [`Forces::clusters`]; empty - no groups.
pub fn layout(
    n: usize,
    links: &[(usize, usize)],
    groups: &[usize],
    boxes: Option<&[NodeBox]>,
    forces: Forces,
) -> Vec<(f64, f64)> {
    layout_with(n, links, &[], groups, boxes, forces)
}

/// [`layout`] with extra springs `tight` (book - chapter): shorter
/// ([`TIGHT_EDGE`]) and stiffer ([`TIGHT_SPRING`]) than the normal ones.
fn layout_with(
    n: usize,
    links: &[(usize, usize)],
    tight: &[(usize, usize)],
    groups: &[usize],
    boxes: Option<&[NodeBox]>,
    forces: Forces,
) -> Vec<(f64, f64)> {
    let mut pos = layout_forces(n, links, tight, groups, forces);
    if let Some(boxes) = boxes {
        separate(&mut pos, boxes);
    }
    pos
}

/// Layout area per node: a label is ~130 x 35 on average.
const AREA: f64 = 12_000.0;
/// Pull to the center `CENTER_PULL * r`: in balance with the repulsion
/// `EDGE²/d` it gives a density disc of `π EDGE² / CENTER_PULL` per node.
const CENTER_PULL: f64 = std::f64::consts::PI * EDGE * EDGE / AREA;
/// Barnes-Hut: a cell of size `s` at distance `d` is one body if `s/d` is
/// less than this.
const THETA: f64 = 0.9;
/// Force iterations.
const ITERATIONS: usize = 60;
/// The largest step of a node: at first, then it decreases to ~2.
const START_TEMP: f64 = 40.0;
/// Step factor per iteration.
const COOLING: f64 = 0.95;
/// Pull to the folder at `clusters` = 100 %, in parts of [`CENTER_PULL`].
const CLUSTER_PULL: f64 = 4.0 * CENTER_PULL;
/// Extra repulsion of another folder at `clusters` = 100 %, in parts of the
/// normal one (a folder is one body at its middle).
const GROUP_REPEL: f64 = 1.0;

/// Length of the spring "book - chapter" (the link itself is a graph edge
/// too, the springs add up).
const TIGHT_EDGE: f64 = 0.4 * EDGE;
/// Stiffness of the spring "book - chapter", in parts of the normal one.
const TIGHT_SPRING: f64 = 10.0;

fn layout_forces(
    n: usize,
    links: &[(usize, usize)],
    tight: &[(usize, usize)],
    groups: &[usize],
    forces: Forces,
) -> Vec<(f64, f64)> {
    let mut pos = initial_positions(n);
    let mut temp = START_TEMP;
    let mut tree = QuadTree::default();
    let mut stack = Vec::new();
    let mut force = vec![(0.0, 0.0); n];
    let repel = Forces::scale(forces.repel);
    let spring = Forces::scale(forces.links) / 2.0;
    let center = CENTER_PULL * Forces::scale(forces.center);
    let cluster = CLUSTER_PULL * Forces::scale(forces.clusters);
    let group_repel = Forces::scale(forces.clusters) * GROUP_REPEL;
    let clustered = cluster > 0.0 && groups.len() == n;
    let mut middles = Vec::new();
    for _ in 0..ITERATIONS {
        tree.build(&pos);
        for (i, f) in force.iter_mut().enumerate() {
            let (x, y) = tree.repulsion(i, pos[i], &mut stack);
            *f = (x * repel, y * repel);
        }
        for &(a, b) in links {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = dx.hypot(dy).max(1.0);
            let f = (d - EDGE) / d * spring;
            force[a].0 -= dx * f;
            force[a].1 -= dy * f;
            force[b].0 += dx * f;
            force[b].1 += dy * f;
        }
        for &(a, b) in tight {
            let (dx, dy) = (pos[a].0 - pos[b].0, pos[a].1 - pos[b].1);
            let d = dx.hypot(dy).max(1.0);
            let f = (d - TIGHT_EDGE) / d * spring * TIGHT_SPRING;
            force[a].0 -= dx * f;
            force[a].1 -= dy * f;
            force[b].0 += dx * f;
            force[b].1 += dy * f;
        }
        if clustered {
            group_middles(&pos, groups, &mut middles);
            // Only because of the edge of the folder's area circle: stragglers
            // are pulled in, but the folder does not get denser than [`AREA`]
            // per node (otherwise labels overlap).
            // Other folders repel more: a gap between folders.
            for ((f, p), &g) in force.iter_mut().zip(&pos).zip(groups) {
                for (h, &(mx, my, radius, mass)) in middles.iter().enumerate() {
                    let (dx, dy) = (mx - p.0, my - p.1);
                    let d = dx.hypot(dy).max(1.0);
                    let k = if h == g {
                        (d - radius).max(0.0) / d * cluster
                    } else {
                        -mass * EDGE * EDGE / (d * d) * group_repel
                    };
                    f.0 += dx * k;
                    f.1 += dy * k;
                }
            }
        }
        for (p, f) in pos.iter_mut().zip(&mut force) {
            f.0 -= p.0 * center;
            f.1 -= p.1 * center;
            let len = f.0.hypot(f.1);
            if len > 0.0 {
                let m = len.min(temp) / len;
                p.0 += f.0 * m;
                p.1 += f.1 * m;
            }
        }
        temp *= COOLING;
    }
    pos
}

/// Group middles: `middles[g]` is the center of mass of the nodes of group
/// `g`, the radius of a circle with [`AREA`] per its node, and the node count.
#[expect(clippy::cast_precision_loss, reason = "thousands of nodes")]
fn group_middles(pos: &[(f64, f64)], groups: &[usize], middles: &mut Vec<(f64, f64, f64, f64)>) {
    let count = groups.iter().max().map_or(0, |g| g + 1);
    let mut sum = vec![(0.0, 0.0, 0usize); count];
    for (p, &g) in pos.iter().zip(groups) {
        sum[g].0 += p.0;
        sum[g].1 += p.1;
        sum[g].2 += 1;
    }
    middles.clear();
    middles.extend(sum.iter().map(|&(x, y, k)| {
        let k = k.max(1) as f64;
        (x / k, y / k, (k * AREA / std::f64::consts::PI).sqrt(), k)
    }));
}

/// Starting places: nodes in order along a Hilbert curve, a square of
/// [`AREA`] per node. Nodes go by path (`Graph` is ordered by name), and a
/// segment of the curve is a compact spot: folders start in clusters, not
/// mixed, and few steps are needed.
#[expect(clippy::cast_precision_loss, reason = "thousands of nodes")]
fn initial_positions(n: usize) -> Vec<(f64, f64)> {
    let mut side = 1;
    while side * side < n {
        side *= 2;
    }
    let step = (n as f64 * AREA).sqrt() / side as f64;
    let half = side as f64 * step / 2.0;
    (0..n)
        .map(|k| {
            let (x, y) = hilbert(side, k * side * side / n);
            ((x as f64 + 0.5) * step - half, (y as f64 + 0.5) * step - half)
        })
        .collect()
}

/// Point `d` of the Hilbert curve on a `side` x `side` grid (`side` is a power of two).
fn hilbert(side: usize, mut d: usize) -> (usize, usize) {
    let (mut x, mut y) = (0, 0);
    let mut s = 1;
    while s < side {
        let rx = 1 & (d / 2);
        let ry = 1 & (d ^ rx);
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += s * rx;
        y += s * ry;
        d /= 4;
        s *= 2;
    }
    (x, y)
}

/// A quadtree for repulsion (Barnes-Hut). Cells are stored flat; a leaf has
/// a segment of `order` with its nodes (more than one only for nodes at one
/// point).
#[derive(Debug, Default)]
struct QuadTree {
    cells: Vec<Cell>,
    order: Vec<usize>,
    pos: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Copy)]
struct Cell {
    /// Center of mass and mass (number of nodes).
    x: f64,
    y: f64,
    mass: f64,
    size: f64,
    /// The first of four consecutive child cells; 0 - a leaf.
    kids: usize,
    /// Leaf nodes: `order[from..to]`.
    from: usize,
    to: usize,
}

/// Reorders `items` so that those matching `pred` go first; returns their count.
fn partition(items: &mut [usize], pred: impl Fn(usize) -> bool) -> usize {
    let mut k = 0;
    for i in 0..items.len() {
        if pred(items[i]) {
            items.swap(i, k);
            k += 1;
        }
    }
    k
}

/// Deeper, the nodes are almost at one point: a leaf with several nodes.
const MAX_DEPTH: usize = 40;

impl QuadTree {
    fn build(&mut self, pos: &[(f64, f64)]) {
        self.cells.clear();
        self.order.clear();
        self.order.extend(0..pos.len());
        self.pos.clear();
        self.pos.extend_from_slice(pos);
        if pos.is_empty() {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in pos {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let size = (x1 - x0).max(y1 - y0).max(1.0);
        self.cells.push(Cell { x: 0.0, y: 0.0, mass: 0.0, size, kids: 0, from: 0, to: pos.len() });
        self.split(0, x0, y0, 0);
    }

    /// Splits the cell `c` with the top left corner `(x0, y0)` and computes
    /// its center of mass.
    #[expect(clippy::cast_precision_loss, reason = "thousands of nodes")]
    fn split(&mut self, c: usize, x0: f64, y0: f64, depth: usize) {
        let Cell { size, from, to, .. } = self.cells[c];
        if to - from > 1 && depth < MAX_DEPTH {
            let half = size / 2.0;
            let (mx, my) = (x0 + half, y0 + half);
            // The top ones (`y < my`) first, in each half the left ones first.
            let pos = &self.pos;
            let items = &mut self.order[from..to];
            let top = partition(items, |i| pos[i].1 < my);
            let (upper, lower) = items.split_at_mut(top);
            let bounds = [
                from,
                from + partition(upper, |i| pos[i].0 < mx),
                from + top,
                from + top + partition(lower, |i| pos[i].0 < mx),
                to,
            ];
            let kids = self.cells.len();
            for q in 0..4 {
                self.cells.push(Cell {
                    x: 0.0,
                    y: 0.0,
                    mass: 0.0,
                    size: half,
                    kids: 0,
                    from: bounds[q],
                    to: bounds[q + 1],
                });
            }
            self.cells[c].kids = kids;
            let (mut sx, mut sy, mut m) = (0.0, 0.0, 0.0);
            for q in 0..4 {
                let k = kids + q;
                if self.cells[k].to > self.cells[k].from {
                    let (qx, qy) = (x0 + half * (q % 2) as f64, y0 + half * (q / 2) as f64);
                    self.split(k, qx, qy, depth + 1);
                    let kid = self.cells[k];
                    sx += kid.x * kid.mass;
                    sy += kid.y * kid.mass;
                    m += kid.mass;
                }
            }
            let cell = &mut self.cells[c];
            (cell.x, cell.y, cell.mass) = (sx / m, sy / m, m);
        } else {
            let (mut sx, mut sy) = (0.0, 0.0);
            for &i in &self.order[from..to] {
                sx += self.pos[i].0;
                sy += self.pos[i].1;
            }
            let m = (to - from) as f64;
            let cell = &mut self.cells[c];
            (cell.x, cell.y, cell.mass) = (sx / m, sy / m, m);
        }
    }

    /// The sum of repulsion `EDGE²/d` from all nodes except `i` on the point
    /// `p`; `stack` is space for the traversal.
    fn repulsion(&self, i: usize, p: (f64, f64), stack: &mut Vec<usize>) -> (f64, f64) {
        let mut f = (0.0, 0.0);
        let mut push = |dx: f64, dy: f64, mass: f64| {
            let d2 = (dx * dx + dy * dy).max(1.0);
            let k = mass * EDGE * EDGE / d2;
            f.0 += dx * k;
            f.1 += dy * k;
        };
        stack.clear();
        stack.push(0);
        while let Some(c) = stack.pop() {
            let cell = self.cells[c];
            if cell.mass == 0.0 {
                continue;
            }
            let (dx, dy) = (p.0 - cell.x, p.1 - cell.y);
            if cell.kids == 0 {
                for &j in &self.order[cell.from..cell.to] {
                    if j != i {
                        // Nodes at one point: a deterministic shift by index pushes them apart.
                        let (dx, dy) = (p.0 - self.pos[j].0, p.1 - self.pos[j].1);
                        let (dx, dy) =
                            if dx == 0.0 && dy == 0.0 { (if i < j { -1.0 } else { 1.0 }, 0.0) } else { (dx, dy) };
                        push(dx, dy, 1.0);
                    }
                }
            } else if cell.size * cell.size < THETA * THETA * (dx * dx + dy * dy) {
                push(dx, dy, cell.mass);
            } else {
                stack.extend((cell.kids..cell.kids + 4).rev());
            }
        }
        f
    }
}

/// A grid to find neighbours: cell -> node indices in ascending order.
struct Grid {
    w: f64,
    h: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    fn new(pos: &[(f64, f64)], w: f64, h: f64) -> Self {
        let mut grid = Self { w, h, cells: HashMap::new() };
        for (i, &p) in pos.iter().enumerate() {
            grid.cells.entry(grid.cell(p)).or_default().push(i);
        }
        grid
    }

    #[expect(clippy::cast_possible_truncation, reason = "layout coordinates are thousands of units")]
    fn cell(&self, (x, y): (f64, f64)) -> (i64, i64) {
        ((x / self.w).floor() as i64, (y / self.h).floor() as i64)
    }

    /// Nodes with an index greater than `i` (`usize::MAX` - all) in the cell
    /// `(cx, cy)` and the eight neighbouring ones, in ascending order.
    fn after(&self, i: usize, (cx, cy): (i64, i64), out: &mut Vec<usize>) {
        out.clear();
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                if let Some(v) = self.cells.get(&(x, y)) {
                    // Cells are ascending runs: a merge, not a sort.
                    let from = if i == usize::MAX { 0 } else { v.partition_point(|&j| j <= i) };
                    out.extend_from_slice(&v[from..]);
                }
            }
        }
        out.sort_unstable();
    }

    /// A node moved.
    fn moved(&mut self, node: usize, from: (f64, f64), to: (f64, f64)) {
        let (old, new) = (self.cell(from), self.cell(to));
        if old == new {
            return;
        }
        // The order of indices in a cell is kept.
        if let Some(list) = self.cells.get_mut(&old)
            && let Ok(at) = list.binary_search(&node)
        {
            list.remove(at);
        }
        let list = self.cells.entry(new).or_default();
        let at = list.binary_search(&node).unwrap_or_else(|at| at);
        list.insert(at, node);
    }
}

/// Pushes apart overlapping node rectangles: each pair equally along the
/// axis with the smaller overlap. A few dozen passes are enough for the
/// labels to stop overlapping; links barely change the look.
///
/// Pairs go in the same order as when trying all (`i < j` ascending) and
/// with the same shifts, but the candidates `j` come from a grid by the
/// current places: a cell is wider than the two widest rectangles, so
/// overlapping nodes are always in neighbouring cells. When `i` moves, the
/// candidates are looked up anew.
fn separate(pos: &mut [(f64, f64)], boxes: &[NodeBox]) {
    let n = pos.len();
    let half = boxes.iter().map(|b| b.r.max(b.label / 2.0)).fold(0.0, f64::max);
    let radius = boxes.iter().map(|b| b.r).fold(0.0, f64::max);
    let width = 2.0 * half + PAD + 1.0;
    let height = 2.0 * radius + LABEL_GAP + LABEL_SIZE * 1.2 + PAD + 1.0;
    let mut grid = Grid::new(pos, width, height);
    let mut near = Vec::new();
    for _ in 0..80 {
        let mut moved = false;
        for i in 0..n {
            grid.after(i, grid.cell(pos[i]), &mut near);
            let mut next = 0;
            while next < near.len() {
                let j = near[next];
                next += 1;
                let [a0, a1, a2, a3] = box_rect(pos[i], boxes[i]);
                let [b0, b1, b2, b3] = box_rect(pos[j], boxes[j]);
                let ox = a2.min(b2) - a0.max(b0) + PAD;
                let oy = a3.min(b3) - a1.max(b1) + PAD;
                if ox <= 0.0 || oy <= 0.0 {
                    continue;
                }
                moved = true;
                let (pi, pj) = (pos[i], pos[j]);
                // Equal centers: the first goes left/up, deterministically.
                if ox < oy {
                    let s = if pos[i].0 <= pos[j].0 { -ox / 2.0 } else { ox / 2.0 };
                    pos[i].0 += s;
                    pos[j].0 -= s;
                } else {
                    let s = if pos[i].1 <= pos[j].1 { -oy / 2.0 } else { oy / 2.0 };
                    pos[i].1 += s;
                    pos[j].1 -= s;
                }
                grid.moved(i, pi, pos[i]);
                grid.moved(j, pj, pos[j]);
                // i moved, so the neighbours differ: next come candidates greater than j.
                grid.after(j, grid.cell(pos[i]), &mut near);
                next = 0;
            }
        }
        if !moved {
            return;
        }
    }
}

/// Radius of the circle of a note without links (links add to it).
const NOTE_R: f64 = 5.5;
/// Radius of a book chapter (always the same, smaller than any note).
const CHAPTER_R: f64 = 4.5;

/// Lays out the graph: radii, labels, coordinates, bounds. `groups` are all
/// groups of the vault (the order of colors).
pub fn place(graph: &Graph, groups: Vec<String>, center: Option<String>, forces: Forces) -> GraphLayout {
    let index: HashMap<&str, usize> = graph.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let pair = |e: &Edge| Some((*index.get(e.from.as_str())?, *index.get(e.to.as_str())?));
    let links: Vec<(usize, usize)> = graph.edges.iter().filter_map(pair).collect();
    // A book and its chapters are a cluster: the edges "book - chapter" are shorter and stiffer than links.
    let tight: Vec<(usize, usize)> = graph.edges.iter().filter(|e| e.chapter).filter_map(pair).collect();
    let mut degree = vec![0usize; graph.nodes.len()];
    for &(a, b) in &links {
        degree[a] += 1;
        degree[b] += 1;
    }
    let boxes: Vec<NodeBox> = graph
        .nodes
        .iter()
        .zip(&degree)
        .map(|(n, &d)| {
            let book = n.kind == Some(NoteKind::Book) && n.chapter.is_none();
            #[expect(clippy::cast_precision_loss, reason = "at most 8")]
            let grown = |base: f64| base + d.min(8) as f64 * 0.7;
            // A chapter is smaller than any note: it is a part of the book, not a separate note.
            let r = if n.chapter.is_some() {
                CHAPTER_R
            } else if book {
                grown(11.0)
            } else {
                grown(NOTE_R)
            };
            NodeBox { r, label: label_width(&n.title, if book { LABEL_SIZE * 1.1 } else { LABEL_SIZE }) }
        })
        .collect();
    let node_groups: Vec<usize> =
        graph.nodes.iter().map(|n| groups.iter().position(|g| g == node_group(n)).unwrap_or(0)).collect();
    let pos = layout_with(graph.nodes.len(), &links, &tight, &node_groups, Some(&boxes), forces);
    let bounds = pos.iter().zip(&boxes).map(|(&p, &b)| box_rect(p, b)).fold(None, |acc: Option<[f64; 4]>, r| {
        Some(acc.map_or(r, |a| [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]))
    });
    let nodes = graph
        .nodes
        .iter()
        .zip(pos)
        .zip(boxes.iter().zip(degree))
        .map(|((n, (x, y)), (b, degree))| PlacedNode {
            id: n.id.clone(),
            kind: n.kind,
            chapter: n.chapter.clone(),
            name: n.title.clone(),
            group: node_group(n).to_owned(),
            x,
            y,
            r: b.r,
            degree,
        })
        .collect();
    GraphLayout { nodes, edges: graph.edges.clone(), groups, bounds: bounds.unwrap_or([0.0, 0.0, 1.0, 1.0]), center }
}

/// Prefix of the graph in vault data: `/_vault/graph/...`.
pub const DATA_PREFIX: &str = "graph";

/// Provider of `/_vault/graph/<filter>.json`: the graph by the filter.
#[derive(Debug)]
pub struct GraphData {
    /// The vault.
    pub vault: Vault,
    /// Its source index.
    pub index: Arc<SourceIndex>,
    /// Layout cache.
    pub layouts: Arc<Layouts>,
}

/// The fingerprint is by the response itself (the default): a note edit
/// that did not change the graph does not rebuild a note with the graph.
impl DataProvider for GraphData {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        data_file(|| self.index.snapshot(&self.vault).map_err(|e| e.to_string()), &self.layouts, path)
    }
}

/// File `/_vault/graph/<path>` for a note: `<filter>.json`, where the filter
/// is the JSON of [`GraphFilter`] with `%`, `/`, `\` escaped
/// (`baluk/graph.typ`). The response is [`GraphLayout`] as JSON.
pub fn data_file(
    snapshot: impl FnOnce() -> Result<Snapshot, String>,
    layouts: &Layouts,
    path: &str,
) -> Result<Vec<u8>, String> {
    let query = path
        .strip_suffix(".json")
        .ok_or_else(|| format!("no vault data \"{DATA_PREFIX}/{path}\" (the graph is {DATA_PREFIX}/<filter>.json)"))?;
    let filter: GraphFilter =
        serde_json::from_str(&unescape(query)).map_err(|e| format!("graph: invalid filter ({e})"))?;
    let layout = snapshot()?.graph_layout_cached(&filter, layouts);
    serde_json::to_vec(&*layout).map_err(|e| e.to_string())
}

/// The reverse of the escaping in `baluk/graph.typ`: `%25` -> `%`, `%2F` -> `/`, `%5C` -> `\`.
fn unescape(s: &str) -> String {
    s.replace("%2F", "/").replace("%5C", "\\").replace("%25", "%")
}

/// How many layouts to remember.
const LAYOUTS: usize = 32;

/// Layout cache: the same shown graph (nodes, edges, groups, center) gives
/// the same layout, no need to compute it anew. The key is a hash of the
/// graph itself and the forces, so the cache does not go stale: a changed
/// vault is another key. The switches of the `/graph` page, the version of
/// a note with `#vault-graph` ([`GraphData`]) and its build take a ready one.
#[derive(Debug, Default)]
pub struct Layouts {
    /// Recent ones first.
    entries: parking_lot::Mutex<std::collections::VecDeque<(u64, Arc<GraphLayout>)>>,
}

impl Layouts {
    /// The graph layout (see [`place`]): from the cache or computed.
    pub fn place(
        &self,
        graph: &Graph,
        groups: Vec<String>,
        center: Option<String>,
        forces: Forces,
    ) -> Arc<GraphLayout> {
        let key = {
            let json = serde_json::to_vec(&(graph, &groups, &center, forces)).unwrap_or_default();
            crate::version::StableHasher::new().bytes(&json).finish()
        };
        {
            let mut entries = self.entries.lock();
            if let Some(hit) = entries.iter().position(|(h, _)| *h == key).and_then(|k| entries.remove(k)) {
                entries.push_front(hit.clone());
                return hit.1;
            }
        }
        let layout = Arc::new(place(graph, groups, center, forces));
        let mut entries = self.entries.lock();
        entries.push_front((key, layout.clone()));
        entries.truncate(LAYOUTS);
        layout
    }
}

impl Snapshot {
    /// The vault graph by the filter, laid out for drawing; the layout comes
    /// from the cache `layouts` if this graph was laid out before.
    pub fn graph_layout_cached(&self, f: &GraphFilter, layouts: &Layouts) -> Arc<GraphLayout> {
        let (shown, groups) = self.shown(f);
        layouts.place(&shown, groups, f.around.clone(), f.forces)
    }

    /// The vault graph by the filter, laid out for drawing.
    pub fn graph_layout(&self, f: &GraphFilter) -> GraphLayout {
        let (shown, groups) = self.shown(f);
        place(&shown, groups, f.around.clone(), f.forces)
    }

    /// The shown graph by the filter and all groups of the vault.
    fn shown(&self, f: &GraphFilter) -> (Graph, Vec<String>) {
        let full = self.graph_of(f.chapters);
        let mut groups: Vec<String> = Vec::new();
        for n in &full.nodes {
            let g = node_group(n);
            if !groups.iter().any(|x| x == g) {
                groups.push(g.to_owned());
            }
        }
        let has_tag = |n: &Node, tag: &str| match &n.chapter {
            Some(c) => self.chapter_has_tag(&c.book, &c.anchor, tag),
            None => self.has_tag(&n.id, tag),
        };
        (filter(&full, has_tag, f), groups)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::time::Instant;

    use super::*;
    use crate::graph::Node;

    #[derive(Debug)]
    struct BenchGraph {
        links: Vec<(usize, usize)>,
        boxes: Vec<NodeBox>,
        groups: Vec<usize>,
        degree: Vec<usize>,
    }

    #[derive(Debug)]
    struct Quality {
        edge_mean: f64,
        area_per_node: f64,
        overlaps: usize,
        intra_group: f64,
        group_gap: f64,
        isolated_radius: f64,
    }

    #[expect(clippy::cast_precision_loss, reason = "a synthetic graph of thousands of nodes")]
    fn synthetic_graph(n: usize) -> BenchGraph {
        let groups_count = (n / 120).clamp(4, 40);
        let mut groups = vec![0usize; n];
        for (i, group) in groups.iter_mut().enumerate() {
            *group = i.saturating_mul(groups_count) / n.max(1);
        }
        let mut edges = BTreeSet::new();
        let mut group_nodes: Vec<Vec<usize>> = vec![Vec::new(); groups_count];
        for (i, &group) in groups.iter().enumerate() {
            group_nodes[group].push(i);
        }
        // Local connectivity inside a folder: a chain plus short bridges.
        for nodes in &group_nodes {
            for pair in nodes.windows(2) {
                edges.insert((pair[0], pair[1]));
            }
            for pair in nodes.windows(4) {
                edges.insert((pair[0], pair[3]));
            }
        }
        // Links between folders: a ring of group centers.
        let centers: Vec<usize> = group_nodes.iter().filter_map(|nodes| nodes.first().copied()).collect();
        for i in 0..centers.len() {
            let a = centers[i];
            let b = centers[(i + 1) % centers.len()];
            if a != b {
                edges.insert((a.min(b), a.max(b)));
            }
        }
        // A few hubs with wide links across the whole vault.
        let hubs: Vec<usize> = [0usize, n / 7, n / 3, n / 2].into_iter().filter(|&i| i < n).collect();
        for &hub in &hubs {
            for step in [17usize, 41, 89, 157] {
                let mut v = hub + step;
                while v < n {
                    if groups[v] != groups[hub] {
                        edges.insert((hub.min(v), hub.max(v)));
                    }
                    v += step;
                }
            }
        }
        // Isolated notes: as in a real vault, some nodes have no links.
        for i in (19..n).step_by(53) {
            if i % 7 == 0 {
                edges.retain(|&(a, b)| a != i && b != i);
            }
        }
        let links: Vec<(usize, usize)> = edges.into_iter().collect();
        let mut degree = vec![0usize; n];
        for &(a, b) in &links {
            degree[a] += 1;
            degree[b] += 1;
        }
        let boxes: Vec<NodeBox> = (0..n)
            .map(|i| {
                let text_len = 8 + (i * 17 + 11) % 26;
                let label = "x".repeat(text_len);
                let r = 5.5 + (degree[i].min(10) as f64) * 0.35;
                NodeBox { r, label: label_width(&label, LABEL_SIZE) }
            })
            .collect();
        BenchGraph { links, boxes, groups, degree }
    }

    #[expect(clippy::cast_precision_loss, reason = "thousands of nodes, fractional metrics")]
    fn quality(pos: &[(f64, f64)], graph: &BenchGraph) -> Quality {
        let edge_len: Vec<f64> =
            graph.links.iter().map(|&(a, b)| (pos[a].0 - pos[b].0).hypot(pos[a].1 - pos[b].1)).collect();
        let edge_mean = edge_len.iter().sum::<f64>() / edge_len.len().max(1) as f64;
        let bounds =
            pos.iter().zip(&graph.boxes).map(|(&p, &b)| box_rect(p, b)).fold(None, |acc: Option<[f64; 4]>, r| {
                Some(acc.map_or(r, |a| [a[0].min(r[0]), a[1].min(r[1]), a[2].max(r[2]), a[3].max(r[3])]))
            });
        let [x0, y0, x1, y1] = bounds.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let area_per_node = ((x1 - x0) * (y1 - y0)) / pos.len().max(1) as f64;

        let mut overlaps = 0usize;
        for i in 0..pos.len() {
            for j in i + 1..pos.len() {
                let [a0, a1, a2, a3] = box_rect(pos[i], graph.boxes[i]);
                let [b0, b1, b2, b3] = box_rect(pos[j], graph.boxes[j]);
                if a2.min(b2) > a0.max(b0) && a3.min(b3) > a1.max(b1) {
                    overlaps += 1;
                }
            }
        }

        let groups_count = graph.groups.iter().copied().max().map_or(0, |x| x + 1);
        let mut centroids = vec![(0.0, 0.0, 0usize); groups_count];
        for (i, &(x, y)) in pos.iter().enumerate() {
            let g = graph.groups[i];
            centroids[g].0 += x;
            centroids[g].1 += y;
            centroids[g].2 += 1;
        }
        let mut centroid_pos = vec![(0.0, 0.0); groups_count];
        for (g, &(sx, sy, c)) in centroids.iter().enumerate() {
            if c > 0 {
                centroid_pos[g] = (sx / c as f64, sy / c as f64);
            }
        }
        let mut intra = 0.0;
        for (i, &(x, y)) in pos.iter().enumerate() {
            let c = centroid_pos[graph.groups[i]];
            intra += (x - c.0).hypot(y - c.1);
        }
        let intra_group = intra / pos.len().max(1) as f64;

        let mut gap_sum = 0.0;
        let mut gap_n = 0usize;
        for i in 0..groups_count {
            for j in i + 1..groups_count {
                if centroids[i].2 == 0 || centroids[j].2 == 0 {
                    continue;
                }
                gap_sum += (centroid_pos[i].0 - centroid_pos[j].0).hypot(centroid_pos[i].1 - centroid_pos[j].1);
                gap_n += 1;
            }
        }
        let group_gap = gap_sum / gap_n.max(1) as f64;

        let (cx, cy) = (
            pos.iter().map(|p| p.0).sum::<f64>() / pos.len().max(1) as f64,
            pos.iter().map(|p| p.1).sum::<f64>() / pos.len().max(1) as f64,
        );
        let isolated: Vec<f64> = pos
            .iter()
            .enumerate()
            .filter_map(|(i, &(x, y))| (graph.degree[i] == 0).then_some((x - cx).hypot(y - cy)))
            .collect();
        let isolated_radius = isolated.iter().sum::<f64>() / isolated.len().max(1) as f64;
        Quality { edge_mean, area_per_node, overlaps, intra_group, group_gap, isolated_radius }
    }

    /// Layout and label separation time (ms) and quality.
    fn bench_current(graph: &BenchGraph) -> (f64, f64, Quality) {
        let started = Instant::now();
        let mut pos = layout_forces(graph.boxes.len(), &graph.links, &[], &graph.groups, Forces::default());
        let forces = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        separate(&mut pos, &graph.boxes);
        let separated = started.elapsed().as_secs_f64() * 1000.0;
        (forces, separated, quality(&pos, graph))
    }

    fn node(id: &str, kind: Option<NoteKind>) -> Node {
        Node { id: id.into(), kind, title: id.rsplit('/').next().unwrap_or(id).into(), chapter: None }
    }

    fn edge(from: &str, to: &str, count: usize) -> Edge {
        Edge { from: from.into(), to: to.into(), count, chapter: false }
    }

    /// A -> B -> C, D -> B, E without links, "Нет" is a link to a missing note.
    fn sample() -> Graph {
        Graph {
            nodes: vec![
                node("Сеть/A", Some(NoteKind::Note)),
                node("Сеть/B", Some(NoteKind::Note)),
                node("Мат/C", Some(NoteKind::Book)),
                node("D", Some(NoteKind::Note)),
                node("E", Some(NoteKind::Note)),
                node("Нет", None),
            ],
            edges: vec![
                edge("Сеть/A", "Сеть/B", 1),
                edge("Сеть/B", "Мат/C", 2),
                edge("D", "Сеть/B", 1),
                edge("D", "Нет", 1),
            ],
        }
    }

    fn tags(n: &Node, tag: &str) -> bool {
        let tags: &[&str] = match n.id.as_str() {
            "Сеть/A" => &["сеть"],
            "Сеть/B" => &["сеть", "ssh"],
            _ => &[],
        };
        tags.contains(&tag)
    }

    fn ids(g: &Graph) -> Vec<&str> {
        let mut v: Vec<_> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        v.sort_unstable();
        v
    }

    fn sorted(set: HashSet<String>) -> Vec<String> {
        let mut v: Vec<_> = set.into_iter().collect();
        v.sort();
        v
    }

    #[test]
    fn group_is_top_folder() {
        assert_eq!(group_of("Сеть/SSH/Ключи"), "Сеть");
        assert_eq!(group_of("Начало"), ROOT_GROUP);
    }

    #[test]
    #[ignore = "a performance measurement tool; run in release with --ignored --nocapture"]
    fn layout_bench_synthetic() {
        for n in [100usize, 300, 1000, 5000] {
            let graph = synthetic_graph(n);
            let (forces, separated, q) = bench_current(&graph);
            println!(
                "layout_bench n={n} total_ms={:.1} layout_ms={:.1} separate_ms={:.1} edge_mean={:.0} area_per_node={:.0} overlaps={} intra_group={:.0} group_gap={:.0} isolated_radius={:.0}",
                forces + separated,
                forces,
                separated,
                q.edge_mean,
                q.area_per_node,
                q.overlaps,
                q.intra_group,
                q.group_gap,
                q.isolated_radius,
            );
        }
    }

    #[test]
    fn neighbours_both_directions() {
        let g = sample();
        assert_eq!(sorted(neighbourhood(&g, "Сеть/A", 1)), ["Сеть/A", "Сеть/B"]);
        assert_eq!(sorted(neighbourhood(&g, "Сеть/A", 2)), ["D", "Мат/C", "Сеть/A", "Сеть/B"]);
    }

    #[test]
    fn filters() {
        let g = sample();
        let f = GraphFilter::default();
        assert_eq!(filter(&g, tags, &f).nodes, g.nodes);
        let no_net = filter(&g, tags, &GraphFilter { hidden: vec!["Сеть".into()], ..f.clone() });
        assert_eq!(ids(&no_net), ["D", "E", "Мат/C", "Нет"]);
        assert_eq!(no_net.edges, [edge("D", "Нет", 1)]);
        let only_net = filter(&g, tags, &GraphFilter { folders: vec!["Сеть".into()], ..f.clone() });
        assert_eq!(ids(&only_net), ["Сеть/A", "Сеть/B"]);
        assert_eq!(ids(&filter(&g, tags, &GraphFilter { tag: Some("ssh".into()), ..f.clone() })), ["Сеть/B"]);
        let linked = filter(&g, tags, &GraphFilter { missing: false, orphans: false, ..f.clone() });
        assert_eq!(ids(&linked), ["D", "Мат/C", "Сеть/A", "Сеть/B"]);
        // A folder with its subfolders, but not a neighbour with the same name start.
        let mut deep = g.clone();
        deep.nodes.extend(["Сеть/Linux/SSH", "Сетевое"].map(|id| node(id, Some(NoteKind::Note))));
        assert_eq!(
            ids(&filter(&deep, tags, &GraphFilter { folder: Some("Сеть".into()), ..f.clone() })),
            ["Сеть/A", "Сеть/B", "Сеть/Linux/SSH"]
        );
        assert_eq!(
            ids(&filter(&deep, tags, &GraphFilter { folder: Some("Сеть/Linux".into()), ..f })),
            ["Сеть/Linux/SSH"]
        );
    }

    #[test]
    fn center_stays() {
        let g = sample();
        let f = GraphFilter {
            around: Some("E".into()),
            orphans: false,
            hidden: vec![ROOT_GROUP.into()],
            ..GraphFilter::default()
        };
        assert_eq!(ids(&filter(&g, tags, &f)), ["E"]);
        let f = GraphFilter { around: Some("D".into()), ..GraphFilter::default() };
        assert_eq!(ids(&filter(&g, tags, &f)), ["D", "Нет", "Сеть/B"]);
    }

    const RING: [(usize, usize); 4] = [(0, 1), (1, 2), (2, 0), (0, 3)];

    #[test]
    fn layout_is_deterministic_and_keeps_orphans_near() {
        let pos = layout(7, &RING, &[], None, Forces::default());
        assert_eq!(pos, layout(7, &RING, &[], None, Forces::default()));
        let dist = |p: usize, q: usize| (pos[p].0 - pos[q].0).hypot(pos[p].1 - pos[q].1);
        let cluster = dist(0, 1).max(dist(1, 2)).max(dist(0, 3));
        for lone in 4..7 {
            assert!(dist(lone, 0) < cluster * 8.0, "unlinked ones do not fly away");
        }
        assert!(dist(0, 1) > 20.0);
    }

    #[test]
    fn labels_do_not_overlap() {
        let names = [
            "Интеграл Эйлера–Пуассона",
            "Ряд Тейлора",
            "Бинарный поиск",
            "Сортировка пузырьком",
            "SSH",
            "Git",
            "Итоги",
        ];
        let star: Vec<_> = (1..names.len()).map(|i| (0, i)).collect();
        let boxes: Vec<_> = names.iter().map(|n| NodeBox { r: 6.0, label: label_width(n, LABEL_SIZE) }).collect();
        let pos = layout(names.len(), &star, &[], Some(&boxes), Forces::default());
        assert_eq!(pos, layout(names.len(), &star, &[], Some(&boxes), Forces::default()));
        for i in 0..names.len() {
            for j in i + 1..names.len() {
                let [a0, a1, a2, a3] = box_rect(pos[i], boxes[i]);
                let [b0, b1, b2, b3] = box_rect(pos[j], boxes[j]);
                let overlap = a2.min(b2) > a0.max(b0) && a3.min(b3) > a1.max(b1);
                assert!(!overlap, "{} × {}", names[i], names[j]);
            }
        }
    }

    #[test]
    fn data_file_path() {
        assert_eq!(unescape("a%2Fb%5Cc%25252F"), "a/b\\c%252F");
        let layouts = Layouts::default();
        let err = data_file(|| Err("not needed".into()), &layouts, "{}.txt").unwrap_err();
        assert!(err.contains("graph/"), "{err}");
        let err = data_file(|| Err("not needed".into()), &layouts, "{нет.json").unwrap_err();
        assert!(err.contains("filter"), "{err}");
    }

    #[test]
    fn layouts_are_cached_by_graph() {
        let layouts = Layouts::default();
        let g = sample();
        let f = Forces::default();
        let a = layouts.place(&g, vec![], None, f);
        assert!(Arc::ptr_eq(&a, &layouts.place(&g, vec![], None, f)), "the same graph: from the cache");
        assert_eq!(*a, place(&g, vec![], None, f));
        let other = layouts.place(&g, vec![], Some("D".into()), f);
        assert!(!Arc::ptr_eq(&a, &other), "another center: another layout");
        let mut g2 = sample();
        g2.edges.pop();
        assert!(!Arc::ptr_eq(&a, &layouts.place(&g2, vec![], None, f)), "another graph: anew");
        let looser = layouts.place(&g, vec![], None, Forces { repel: 200, ..f });
        assert!(!Arc::ptr_eq(&a, &looser), "other forces: anew");
    }

    #[test]
    fn layout_handles_empty_single_and_without_links() {
        assert!(layout(0, &[], &[], Some(&[]), Forces::default()).is_empty());
        assert_eq!(layout(1, &[], &[], None, Forces::default()), vec![(0.0, 0.0)]);
        let pos = layout(32, &[], &[], None, Forces::default());
        assert_eq!(pos.len(), 32);
        assert!(pos.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
    }

    #[test]
    fn layout_is_bit_identical_for_generated_graph() {
        let graph = synthetic_graph(480);
        let a = layout(480, &graph.links, &[], Some(&graph.boxes), Forces::default());
        let b = layout(480, &graph.links, &[], Some(&graph.boxes), Forces::default());
        let bits = |v: &[(f64, f64)]| v.iter().map(|p| (p.0.to_bits(), p.1.to_bits())).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&b));
    }

    #[test]
    fn generated_labels_do_not_overlap() {
        let mut graph = synthetic_graph(180);
        for b in &mut graph.boxes {
            b.label = label_width("узел", LABEL_SIZE);
        }
        let pos = layout(180, &graph.links, &[], Some(&graph.boxes), Forces::default());
        let q = quality(&pos, &graph);
        assert_eq!(q.overlaps, 0);
    }

    #[test]
    fn forces_do_what_they_say() {
        let graph = synthetic_graph(300);
        let q = |forces| quality(&layout(300, &graph.links, &graph.groups, Some(&graph.boxes), forces), &graph);
        // Each force against the neutral layout (folders 0 %).
        let neutral = Forces { clusters: 0, ..Forces::default() };
        let base = q(neutral);
        let clusters = q(Forces { clusters: 100, ..neutral });
        let repel = q(Forces { repel: 200, ..neutral });
        let center = q(Forces { center: 50, ..neutral });
        let links = q(Forces { links: 500, ..neutral });
        assert_eq!(q(Forces::default()).overlaps, 0, "by default (folders 5 %)");
        let apart = |q: &Quality| q.group_gap / q.intra_group;
        assert!(apart(&clusters) > apart(&base) * 1.4, "folders apart");
        assert!(repel.area_per_node > base.area_per_node * 1.3, "roomier");
        assert!(center.area_per_node > base.area_per_node * 1.3, "weaker to the center: roomier");
        assert!(links.edge_mean < base.edge_mean * 0.9, "shorter links");
        for q in [clusters, repel, center, links] {
            assert_eq!(q.overlaps, 0);
        }
    }

    #[test]
    fn placed_nodes_and_bounds() {
        let g = sample();
        let l = place(&g, vec!["Сеть".into(), "Мат".into(), ROOT_GROUP.into()], None, Forces::default());
        let c = l.nodes.iter().find(|n| n.id == "Мат/C").unwrap();
        assert_eq!((c.name.as_str(), c.group.as_str(), c.degree), ("C", "Мат", 1));
        assert!(c.r > l.nodes.iter().find(|n| n.id == "E").unwrap().r, "a book is larger");
        for n in &l.nodes {
            assert!(l.bounds[0] <= n.x && n.x <= l.bounds[2] && l.bounds[1] <= n.y && n.y <= l.bounds[3]);
        }
    }
}
