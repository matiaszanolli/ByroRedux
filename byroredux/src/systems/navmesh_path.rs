//! NAVM pathfinding — Phases 1, 2 and 3 of
//! `docs/engine/navmesh-pathfinding.md`'s rollout (EX-16 item 3, #2372).
//!
//! A\* over one [`NavmRecord`]'s triangle-adjacency graph
//! (`edge_neighbours`), plus a shared-edge-midpoint waypoint extraction
//! turning the resulting triangle corridor into an actual walkable
//! polyline. The core algorithm ([`find_path_within_tile`] and below) is
//! pure geometry over parsed NAVM data — no ECS/`World` dependency, fully
//! exercised by synthetic fixtures below; [`path_from_resident_tiles`] is
//! the thin ECS bridge Phase 3 needed to reach real streamed data. Phase 3
//! shipped 2026-08-23 and it is now consumed by all six locomotion
//! procedures — travel / wander / patrol / follow / escort / guard — through
//! `locomotion::step_along_waypoints`, not by `travel_system` alone.
//!
//! **Cross-tile (Phase 2, #3802)**: `external_connections` itself is still
//! not walked — `NavmExternalConnection` has no confirmed source-triangle
//! field, corpus-verified unrecoverable from the sub-record (design doc
//! §9 Phase 2, "Resolved (2026-08-27, #3300)"). The join is instead
//! recovered **geometrically**: two tiles' border edges that share exact
//! vertex positions (within a 1e-2 world-unit bucket) are a genuine
//! portal between them. Measured coverage on `FalloutNV.esm`: 67.1% of
//! adjacent mesh pairs share at least one such vertex. The remaining ~33%
//! (separately-authored meshes with small coordinate drift at their
//! shared border) would need a tolerance sweep to recover, which the
//! design doc explicitly flags as unmeasured — this module doesn't
//! attempt it, same no-guessing posture as every other unconfirmed NAVM
//! field. [`path_from_resident_tiles`] tries the exact same-tile search
//! first (unchanged, zero behavior change for that case) and only reaches
//! for the geometric cross-tile graph when `current` and `goal` localize
//! onto different resident tiles.
//!
//! # Coordinate space
//! [`NavmRecord::vertices`] are raw `NVVX` floats in Bethesda **Z-up**
//! world units — `parse_navm` applies no coordinate conversion, unlike
//! REFR placement/NIF import. Every public function here takes and
//! returns **engine Y-up** [`Vec3`] (matching `step_toward` and every
//! other locomotion-facing API); vertices are converted via
//! [`byroredux_core::math::coord::zup_to_yup_pos`] the moment they're
//! read and never left in source space past this module's boundary. This
//! detail isn't called out in the design doc (an oversight there, not a
//! decision) — noted here since it's the one thing that would silently
//! produce a rotated/mirrored path if missed.
//!
//! # Funnel/string-pulling — deferred, not attempted
//! The design doc's §3 recommends a full funnel (string-pulling) pass
//! for the shortest in-corridor polyline. That needs each portal's two
//! vertices in a *consistent left/right orientation* along the corridor
//! — which in turn needs either a corpus-confirmed consistently-wound
//! `NavmTriangle::vertices` convention (not established anywhere in this
//! codebase; `parse_navm`'s own doc is silent on winding) or interactive
//! visual verification (not available in this environment). Rather than
//! guess an orientation and risk a silently-wrong (not panicking, not
//! failing any test, just *geometrically incorrect*) path — exactly the
//! failure mode this project's no-speculative-fixes convention warns
//! about — this module instead extracts waypoints as the **midpoint of
//! each shared edge** between consecutive corridor triangles
//! ([`shared_edge`]). This needs no orientation at all (it's derived from
//! plain vertex-index set intersection, provably correct for a watertight
//! mesh) and is guaranteed to stay inside the corridor, just not
//! shortest-path-optimal — a real, valid, and immediately useful
//! improvement over today's straight-line-through-walls locomotion,
//! honestly short of the doc's eventual funnel goal. Upgrading to a true
//! funnel pass is a well-scoped, isolated follow-up once winding is
//! confirmed against real data.

use byroredux_core::math::coord::zup_to_yup_pos;
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::{NavmRecord, NavmTriangle};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// How far a query point's barycentric weights may fall outside `[0, 1]`
/// (i.e. outside the triangle) and still be accepted — absorbs float
/// error at a shared edge, where two adjacent triangles' tests would
/// otherwise both reject a point sitting exactly on the border.
const BARYCENTRIC_EPSILON: f32 = 1.0e-3;

fn vertex_yup(navm: &NavmRecord, idx: u16) -> Option<Vec3> {
    navm.vertices
        .get(idx as usize)
        .map(|v| Vec3::from_array(zup_to_yup_pos(*v)))
}

fn triangle_vertices(navm: &NavmRecord, tri_idx: usize) -> Option<[Vec3; 3]> {
    let tri = navm.triangles.get(tri_idx)?;
    Some([
        vertex_yup(navm, tri.vertices[0])?,
        vertex_yup(navm, tri.vertices[1])?,
        vertex_yup(navm, tri.vertices[2])?,
    ])
}

fn centroid(v: &[Vec3; 3]) -> Vec3 {
    (v[0] + v[1] + v[2]) / 3.0
}

/// Barycentric weights of `p`'s XZ projection against triangle
/// `(v0, v1, v2)`'s own XZ projection. `None` for a degenerate
/// (zero-XZ-area) triangle. Weights sum to `1.0`; all three `>= 0` means
/// `p` projects inside the triangle.
fn barycentric_xz(p: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<(f32, f32, f32)> {
    let denom = (v1.z - v2.z) * (v0.x - v2.x) + (v2.x - v1.x) * (v0.z - v2.z);
    if denom.abs() < f32::EPSILON {
        return None;
    }
    let u = ((v1.z - v2.z) * (p.x - v2.x) + (v2.x - v1.x) * (p.z - v2.z)) / denom;
    let v = ((v2.z - v0.z) * (p.x - v2.x) + (v0.x - v2.x) * (p.z - v2.z)) / denom;
    let w = 1.0 - u - v;
    Some((u, v, w))
}

/// Locate the triangle whose XZ projection contains `point`, per
/// design-doc §5. When more than one tile-triangle plausibly contains it
/// (multi-story interiors, a bridge over a lower navmesh), the one whose
/// interpolated surface height is closest to `point.y` wins.
pub(crate) fn find_containing_triangle(navm: &NavmRecord, point: Vec3) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for tri_idx in 0..navm.triangles.len() {
        let Some(v) = triangle_vertices(navm, tri_idx) else {
            continue;
        };
        let Some((u, vv, w)) = barycentric_xz(point, v[0], v[1], v[2]) else {
            continue;
        };
        if u < -BARYCENTRIC_EPSILON || vv < -BARYCENTRIC_EPSILON || w < -BARYCENTRIC_EPSILON {
            continue;
        }
        let height = u * v[0].y + vv * v[1].y + w * v[2].y;
        let diff = (height - point.y).abs();
        if best.is_none_or(|(_, best_diff)| diff < best_diff) {
            best = Some((tri_idx, diff));
        }
    }
    best.map(|(idx, _)| idx)
}

/// The two vertex indices shared between `a` and `b` — the "portal" edge
/// a path crosses moving from one triangle to the other. Derived from
/// actual shared vertex *indices*, not an assumed
/// `edge_neighbours`-slot-to-vertex-pair convention (unverified against
/// real NAVM data anywhere in this codebase — see the module doc). A
/// watertight navmesh shares literal vertex indices at a border, so this
/// is exact, not an approximation; returns `None` only if `a`/`b` aren't
/// actually adjacent (fewer than 2 shared vertices).
fn shared_edge(a: &NavmTriangle, b: &NavmTriangle) -> Option<(u16, u16)> {
    let mut shared = a
        .vertices
        .iter()
        .copied()
        .filter(|v| b.vertices.contains(v));
    let first = shared.next()?;
    let second = shared.next()?;
    Some((first, second))
}

// ── Phase 2 — cross-tile geometric join (#3802) ─────────────────────────

/// A cross-tile graph node: `(mesh_form, triangle_index)`. Plain `usize`
/// (as [`astar_triangle_path`] uses) isn't enough once more than one
/// tile's triangles are in play — two different tiles' index spaces are
/// otherwise indistinguishable.
type CrossTileNode = (u32, usize);

/// World-space quantization bucket for geometric vertex matching — 1e-2
/// world units, matching the corpus-measured exact-match bound the design
/// doc's §9 Phase 2 entry establishes. Scaling and rounding to an integer
/// tuple (rather than comparing rounded floats directly) sidesteps float
/// equality entirely: two positions bucket together iff they round to the
/// same integer triple.
const VERTEX_QUANTIZE_SCALE: f32 = 100.0; // 1 / 1e-2

fn quantize(v: Vec3) -> (i64, i64, i64) {
    (
        (v.x * VERTEX_QUANTIZE_SCALE).round() as i64,
        (v.y * VERTEX_QUANTIZE_SCALE).round() as i64,
        (v.z * VERTEX_QUANTIZE_SCALE).round() as i64,
    )
}

type QuantizedEdgeKey = ((i64, i64, i64), (i64, i64, i64));

/// Order-independent key for the edge `(a, b)` — a portal's two tiles
/// don't necessarily list their shared vertices in the same order.
fn quantized_edge_key(a: Vec3, b: Vec3) -> QuantizedEdgeKey {
    let (qa, qb) = (quantize(a), quantize(b));
    if qa <= qb {
        (qa, qb)
    } else {
        (qb, qa)
    }
}

/// The 3 unordered vertex-index pairs of a triangle — its 3 geometric
/// edges. Unlike the funnel algorithm's left/right question (module doc),
/// this needs no winding convention: a triangle has exactly 3 vertices, so
/// each pair of them is unambiguously one edge regardless of storage
/// order.
fn triangle_edge_index_pairs(tri: &NavmTriangle) -> [(u16, u16); 3] {
    let [a, b, c] = tri.vertices;
    [(a, b), (b, c), (c, a)]
}

fn ordered_index_pair(a: u16, b: u16) -> (u16, u16) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Border edges of `navm`: `(triangle_index, vertex_a, vertex_b)` for
/// every vertex-index pair that belongs to exactly one triangle in this
/// mesh. Derived independently of `edge_neighbours`'s slot ordering
/// (unconfirmed convention — see module doc) via the same vertex-index-set
/// approach [`shared_edge`] already uses, so this doesn't add a new
/// assumption to what's already trusted; it only adds a new use of it.
fn border_edges(navm: &NavmRecord) -> Vec<(usize, u16, u16)> {
    let mut owners: HashMap<(u16, u16), Vec<(usize, u16, u16)>> = HashMap::new();
    for (tri_idx, tri) in navm.triangles.iter().enumerate() {
        for (a, b) in triangle_edge_index_pairs(tri) {
            owners
                .entry(ordered_index_pair(a, b))
                .or_default()
                .push((tri_idx, a, b));
        }
    }
    owners
        .into_values()
        .filter(|owners| owners.len() == 1)
        .map(|mut owners| owners.remove(0))
        .collect()
}

/// The geometric cross-tile portal graph (design doc §9 Phase 2): for
/// every resident tile's border edge, an adjacency-list entry to every
/// *other* tile's border edge sharing the same quantized world-space
/// position, carrying the shared position's midpoint (needed later for
/// waypoint extraction — see [`cross_tile_corridor_to_waypoints`]).
///
/// O(total border edges) to build, O(total border edges) space. Built
/// fresh on every cross-tile search rather than cached — resident tile
/// sets only change on cell load/unload (an infrequent, already-cached-
/// around event via [`resolve_cached_waypoints`]'s residency generation),
/// and per-cell border-edge counts are small, so this hasn't shown up as
/// a cost worth caching around; revisit if telemetry says otherwise.
struct CrossTileGraph {
    edges: HashMap<CrossTileNode, Vec<(CrossTileNode, Vec3)>>,
}

impl CrossTileGraph {
    fn build<'a>(tiles: impl Iterator<Item = &'a NavmRecord>) -> Self {
        let mut by_key: HashMap<QuantizedEdgeKey, Vec<(CrossTileNode, Vec3, Vec3)>> =
            HashMap::new();
        for navm in tiles {
            for (tri_idx, v_a, v_b) in border_edges(navm) {
                let (Some(a), Some(b)) = (vertex_yup(navm, v_a), vertex_yup(navm, v_b)) else {
                    continue;
                };
                by_key.entry(quantized_edge_key(a, b)).or_default().push((
                    (navm.form_id, tri_idx),
                    a,
                    b,
                ));
            }
        }
        let mut edges: HashMap<CrossTileNode, Vec<(CrossTileNode, Vec3)>> = HashMap::new();
        for owners in by_key.values() {
            if owners.len() < 2 {
                continue; // no match on this edge — the common case
            }
            for &(node, a, b) in owners {
                let midpoint = (a + b) / 2.0;
                for &(other_node, ..) in owners {
                    // Different *mesh*, not just a different triangle — a
                    // border edge only ever belongs to one triangle within
                    // its own mesh (that's what makes it a border edge),
                    // so a same-mesh match here would mean two distinct
                    // border edges of the same tile happen to land in the
                    // same quantized bucket. Guarded rather than assumed
                    // impossible.
                    if other_node.0 == node.0 {
                        continue;
                    }
                    edges.entry(node).or_default().push((other_node, midpoint));
                }
            }
        }
        Self { edges }
    }

    fn neighbours(&self, node: CrossTileNode) -> &[(CrossTileNode, Vec3)] {
        self.edges.get(&node).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Locate which resident tile's geometry contains `point`, returning its
/// `(mesh_form, triangle)`. Mirrors [`find_containing_triangle`] but
/// across every tile in `tiles` rather than one — the cross-tile
/// equivalent of Phase 1's single-tile localization.
fn locate_across_tiles<'a>(
    tiles: impl Iterator<Item = &'a NavmRecord>,
    point: Vec3,
) -> Option<CrossTileNode> {
    tiles
        .into_iter()
        .find_map(|navm| find_containing_triangle(navm, point).map(|tri| (navm.form_id, tri)))
}

/// Same shape as [`ScoredNode`], over [`CrossTileNode`] instead of a bare
/// triangle index — kept as its own type rather than a generic
/// `ScoredNode<T>` to avoid touching the well-tested single-tile path at
/// all for this addition.
#[derive(Copy, Clone)]
struct ScoredCrossTileNode {
    f_score: f32,
    node: CrossTileNode,
}
impl ScoredCrossTileNode {
    fn new(f_score: f32, node: CrossTileNode) -> Self {
        Self { f_score, node }
    }
}
impl PartialEq for ScoredCrossTileNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_score == other.f_score && self.node == other.node
    }
}
impl Eq for ScoredCrossTileNode {}
impl PartialOrd for ScoredCrossTileNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ScoredCrossTileNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed so `BinaryHeap` (a max-heap) pops the lowest f-score
        // first; ties broken by node for deterministic tests — same
        // convention as `ScoredNode`.
        other
            .f_score
            .total_cmp(&self.f_score)
            .then_with(|| self.node.cmp(&other.node))
    }
}

/// A\* over the combined within-tile (`edge_neighbours`) and cross-tile
/// (geometric portal) adjacency graph. Same cost model as
/// [`astar_triangle_path`] (centroid-to-centroid distance, admissible
/// straight-line heuristic to the goal centroid) extended to
/// [`CrossTileNode`]s so a straight-line distance across tiles remains a
/// valid lower bound regardless of which tile a node belongs to.
fn astar_cross_tile_path(
    navm_by_form: &HashMap<u32, &NavmRecord>,
    graph: &CrossTileGraph,
    start: CrossTileNode,
    goal: CrossTileNode,
) -> Option<Vec<CrossTileNode>> {
    if start == goal {
        return Some(vec![start]);
    }
    let node_centroid = |node: CrossTileNode| -> Option<Vec3> {
        Some(centroid(&triangle_vertices(
            navm_by_form.get(&node.0)?,
            node.1,
        )?))
    };
    let goal_centroid = node_centroid(goal)?;

    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<CrossTileNode, f32> = HashMap::new();
    let mut came_from: HashMap<CrossTileNode, CrossTileNode> = HashMap::new();
    let mut closed: HashSet<CrossTileNode> = HashSet::new();

    g_score.insert(start, 0.0);
    open.push(ScoredCrossTileNode::new(
        node_centroid(start)?.distance(goal_centroid),
        start,
    ));

    while let Some(ScoredCrossTileNode { node: current, .. }) = open.pop() {
        if current == goal {
            let mut path = vec![current];
            let mut cursor = current;
            while let Some(&prev) = came_from.get(&cursor) {
                path.push(prev);
                cursor = prev;
            }
            path.reverse();
            return Some(path);
        }
        if !closed.insert(current) {
            continue;
        }
        let Some(current_centroid) = node_centroid(current) else {
            continue;
        };

        // Collected up front rather than relaxed in-line: `graph.neighbours`
        // borrows `graph` (outside the loop's other mutable state) while
        // the within-tile arm borrows `navm_by_form`, and both need to
        // feed the same relax step below without fighting the borrow
        // checker over `g_score`/`open`/`came_from`.
        let mut neighbours: Vec<CrossTileNode> = navm_by_form
            .get(&current.0)
            .and_then(|navm| navm.triangles.get(current.1))
            .map(|tri| {
                tri.edge_neighbours
                    .iter()
                    .filter_map(|n| *n)
                    .map(|n| (current.0, n as usize))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        neighbours.extend(graph.neighbours(current).iter().map(|&(n, _)| n));

        for neighbour in neighbours {
            if closed.contains(&neighbour) {
                continue;
            }
            let Some(neigh_centroid) = node_centroid(neighbour) else {
                continue;
            };
            let tentative_g = g_score[&current] + current_centroid.distance(neigh_centroid);
            let better = g_score
                .get(&neighbour)
                .is_none_or(|&existing| tentative_g < existing);
            if better {
                came_from.insert(neighbour, current);
                g_score.insert(neighbour, tentative_g);
                open.push(ScoredCrossTileNode::new(
                    tentative_g + neigh_centroid.distance(goal_centroid),
                    neighbour,
                ));
            }
        }
    }
    None
}

/// Cross-tile equivalent of [`corridor_to_waypoints`]: `start`, then a
/// portal waypoint between every consecutive corridor node pair, then
/// `goal`. A within-tile step (same `mesh_form`) reuses
/// [`shared_edge`]'s exact-vertex-index-intersection midpoint, byte-for-
/// byte the same as Phase 1. A cross-tile step (different `mesh_form`)
/// uses the matched portal's midpoint the geometric join already computed
/// — geometrically the same world position from either tile's side, since
/// the join is an exact (quantized) vertex match.
fn cross_tile_corridor_to_waypoints(
    navm_by_form: &HashMap<u32, &NavmRecord>,
    graph: &CrossTileGraph,
    corridor: &[CrossTileNode],
    start: Vec3,
    goal: Vec3,
) -> Option<Vec<Vec3>> {
    let mut waypoints = vec![start];
    for pair in corridor.windows(2) {
        let &[a, b] = pair else { continue };
        if a.0 == b.0 {
            let navm = navm_by_form.get(&a.0)?;
            let tri_a = navm.triangles.get(a.1)?;
            let tri_b = navm.triangles.get(b.1)?;
            let (v0, v1) = shared_edge(tri_a, tri_b)?;
            waypoints.push((vertex_yup(navm, v0)? + vertex_yup(navm, v1)?) / 2.0);
        } else {
            let (_, midpoint) = graph.neighbours(a).iter().find(|(n, _)| *n == b)?;
            waypoints.push(*midpoint);
        }
    }
    waypoints.push(goal);
    Some(waypoints)
}

#[derive(Copy, Clone)]
struct ScoredNode {
    f_score: f32,
    node: usize,
}
impl PartialEq for ScoredNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_score == other.f_score && self.node == other.node
    }
}
impl Eq for ScoredNode {}
impl PartialOrd for ScoredNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ScoredNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed so `BinaryHeap` (a max-heap) pops the lowest f-score
        // first; ties broken by node index for deterministic tests.
        other
            .f_score
            .total_cmp(&self.f_score)
            .then_with(|| self.node.cmp(&other.node))
    }
}

/// A\* over `navm`'s triangle-adjacency graph (`edge_neighbours` only —
/// no cross-tile `external_connections`, per this module's Phase-1
/// scope). Edge cost is centroid-to-centroid distance: the design doc
/// suggests edge-midpoint distance as a closer-to-optimal refinement, but
/// that needs the same unconfirmed edge/vertex convention [`shared_edge`]
/// exists to avoid — centroid distance needs no such assumption and is
/// still an admissible, consistent cost for A\*. Returns the triangle
/// index corridor, or `None` if `goal_tri` isn't reachable from
/// `start_tri` within this tile.
fn astar_triangle_path(navm: &NavmRecord, start_tri: usize, goal_tri: usize) -> Option<Vec<usize>> {
    if start_tri == goal_tri {
        return Some(vec![start_tri]);
    }
    let goal_centroid = centroid(&triangle_vertices(navm, goal_tri)?);

    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<usize, f32> = HashMap::new();
    let mut came_from: HashMap<usize, usize> = HashMap::new();
    let mut closed: HashSet<usize> = HashSet::new();

    g_score.insert(start_tri, 0.0);
    open.push(ScoredNode {
        f_score: centroid(&triangle_vertices(navm, start_tri)?).distance(goal_centroid),
        node: start_tri,
    });

    while let Some(ScoredNode { node: current, .. }) = open.pop() {
        if current == goal_tri {
            let mut path = vec![current];
            let mut cursor = current;
            while let Some(&prev) = came_from.get(&cursor) {
                path.push(prev);
                cursor = prev;
            }
            path.reverse();
            return Some(path);
        }
        if !closed.insert(current) {
            continue;
        }
        let Some(current_verts) = triangle_vertices(navm, current) else {
            continue;
        };
        let current_centroid = centroid(&current_verts);
        let Some(tri) = navm.triangles.get(current) else {
            continue;
        };
        for neighbour in tri.edge_neighbours.iter().filter_map(|n| *n) {
            let neighbour = neighbour as usize;
            if closed.contains(&neighbour) {
                continue;
            }
            let Some(neigh_verts) = triangle_vertices(navm, neighbour) else {
                continue;
            };
            let neigh_centroid = centroid(&neigh_verts);
            let tentative_g = g_score[&current] + current_centroid.distance(neigh_centroid);
            let better = g_score
                .get(&neighbour)
                .is_none_or(|&existing| tentative_g < existing);
            if better {
                came_from.insert(neighbour, current);
                g_score.insert(neighbour, tentative_g);
                open.push(ScoredNode {
                    f_score: tentative_g + neigh_centroid.distance(goal_centroid),
                    node: neighbour,
                });
            }
        }
    }
    None
}

/// Turn an A\* triangle corridor into an actual waypoint polyline: `start`,
/// then the midpoint of each shared edge between consecutive corridor
/// triangles ([`shared_edge`] — orientation-free, see the module doc for
/// why this isn't full funnel string-pulling yet), then `goal`.
fn corridor_to_waypoints(
    navm: &NavmRecord,
    corridor: &[usize],
    start: Vec3,
    goal: Vec3,
) -> Option<Vec<Vec3>> {
    let mut waypoints = vec![start];
    for pair in corridor.windows(2) {
        let &[a, b] = pair else { continue };
        let tri_a = navm.triangles.get(a)?;
        let tri_b = navm.triangles.get(b)?;
        let (v0, v1) = shared_edge(tri_a, tri_b)?;
        let midpoint = (vertex_yup(navm, v0)? + vertex_yup(navm, v1)?) / 2.0;
        waypoints.push(midpoint);
    }
    waypoints.push(goal);
    Some(waypoints)
}

/// Find a walkable waypoint path from `start` to `goal` within one
/// resident `NavmRecord`. `None` when either point doesn't localize onto
/// this tile, or when no triangle-adjacency path connects them within it
/// — callers are expected to fall back to today's straight-line
/// `step_toward` behavior in either case (design doc §4).
pub(crate) fn find_path_within_tile(
    navm: &NavmRecord,
    start: Vec3,
    goal: Vec3,
) -> Option<Vec<Vec3>> {
    let start_tri = find_containing_triangle(navm, start)?;
    let goal_tri = find_containing_triangle(navm, goal)?;
    let corridor = astar_triangle_path(navm, start_tri, goal_tri)?;
    corridor_to_waypoints(navm, &corridor, start, goal)
}

/// Phase 3 ECS bridge: search every currently-resident [`NavmeshTile`]
/// for one whose geometry localizes `current`, and return the remaining
/// waypoints from there to `goal` (never including `current` itself,
/// always ending with `goal` when a path was found). `None` when no
/// resident tile localizes `current`, or `goal` doesn't localize onto
/// *any* resident tile — callers fall back to walking straight at `goal`,
/// exactly today's pre-pathing behavior (design doc §4's
/// residency-boundary degrade, applied to "no tile at all" as well as
/// "goal outside every known corridor").
///
/// Tries the same-tile search first — unchanged from Phase 1, so a
/// `current`/`goal` pair that both localize onto one tile costs and
/// behaves exactly as before. Only when that fails (or no single tile
/// contains both) does this reach for Phase 2's geometric cross-tile
/// graph (#3802) across every resident tile.
pub(crate) fn path_from_resident_tiles(
    tiles: &byroredux_core::ecs::QueryRead<'_, crate::components::NavmeshTile>,
    current: Vec3,
    goal: Vec3,
) -> Option<Vec<Vec3>> {
    if let Some(path) = tiles.iter().find_map(|(_, tile)| {
        find_path_within_tile(&tile.0, current, goal).map(|path| path.into_iter().skip(1).collect())
    }) {
        return Some(path);
    }

    let navms: Vec<&NavmRecord> = tiles.iter().map(|(_, tile)| &tile.0).collect();
    let start = locate_across_tiles(navms.iter().copied(), current)?;
    let goal_node = locate_across_tiles(navms.iter().copied(), goal)?;
    let navm_by_form: HashMap<u32, &NavmRecord> =
        navms.iter().map(|navm| (navm.form_id, *navm)).collect();
    let graph = CrossTileGraph::build(navms.iter().copied());

    let corridor = astar_cross_tile_path(&navm_by_form, &graph, start, goal_node)?;
    let waypoints =
        cross_tile_corridor_to_waypoints(&navm_by_form, &graph, &corridor, current, goal)?;
    Some(waypoints.into_iter().skip(1).collect())
}

/// Phase 4 ECS bridge: resolve the waypoint queue a locomotion system
/// should walk this tick — reusing `cached` when it's still close enough
/// to `goal`, recomputing via [`path_from_resident_tiles`] otherwise.
///
/// One function serves both cost postures the design doc's §6/§7
/// describe, distinguished only by `repath_threshold`:
/// - **Frozen-goal callers** (Travel, Guard, Escort's lead phase) pass
///   `0.0` — `goal` is set once and never changes for the rest of the
///   walk, so this only ever recomputes on the very first tick a given
///   goal is seen (bit-identical `Vec3::distance` is exactly `0.0` for
///   an unchanged, uncomputed-on value — no epsilon needed to catch the
///   "same goal" case).
/// - **Live-goal callers** (Follow, Escort's collect phase) pass a
///   real repath-threshold constant — `goal` moves every tick (a live
///   target's position), and repathing on every single-unit jitter would
///   be needlessly expensive for a target that's barely moved; only
///   recompute once it's moved far enough from the endpoint of the
///   currently-cached path to matter.
///
/// The recomputed (or reused) result is cached **including an empty
/// "no resident-tile path found" result** — see `travel_system`'s Pass 1a
/// for why that negative result matters: without caching it, an
/// off-navmesh actor would retry `path_from_resident_tiles` every single
/// tick instead of once per goal, defeating the whole point of caching.
/// Callers still own writing the returned queue back into their own
/// `NavPath` cache — paired with the returned `effective_goal`, **not**
/// this call's raw `goal` argument. On a cache hit those two differ on
/// purpose: `effective_goal` is the *cached* path's original goal, held
/// steady so a live-goal caller's per-tick jitter doesn't creep the
/// stored goal toward the target's current position every tick (which
/// would silently widen the effective repath tolerance to infinity —
/// caught by `follow_system`'s own regression test for this). Only a
/// genuine recompute (cache miss) advances `effective_goal` to this
/// call's `goal`.
pub(crate) fn resolve_cached_waypoints(
    cached: Option<&crate::components::NavPath>,
    tiles: Option<&byroredux_core::ecs::QueryRead<'_, crate::components::NavmeshTile>>,
    current: Vec3,
    goal: Vec3,
    repath_threshold: f32,
    residency_generation: u64,
) -> (Vec3, std::collections::VecDeque<Vec3>) {
    match cached {
        // #3256 — the generation check is what makes the *negative* cache
        // (a deliberately-stored empty path) recoverable. Goal distance
        // alone cannot express "the set of resident tiles changed", so with
        // a frozen goal and a `0.0` threshold the empty result matched
        // bit-identically forever and `path_from_resident_tiles` was never
        // retried, even once the relevant tile streamed in.
        Some(path)
            if path.residency_generation == residency_generation
                && path.goal.distance(goal) <= repath_threshold =>
        {
            (path.goal, path.waypoints.clone())
        }
        _ => {
            let waypoints = tiles
                .and_then(|tiles| path_from_resident_tiles(tiles, current, goal))
                .map(std::collections::VecDeque::from)
                .unwrap_or_default();
            (goal, waypoints)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::NavmTriangle;

    /// #3269 — every NAVM-pathed procedure used to clone its waypoint list a
    /// second time on the way into `step_along_waypoints`, purely because the
    /// per-tick scratch was iterated by reference while the callee takes the
    /// `VecDeque` by value. Three of the four now drain their `pending`
    /// buffer; `follow_system` cannot (Pass 2 still needs its decisions) and
    /// takes the field instead. Both shapes are load-bearing and neither is
    /// visible to a behavioural test, so pin them at the source: the clone
    /// this issue removed must not come back, in any of the four.
    #[test]
    fn no_navmesh_procedure_clones_its_waypoints_into_the_stepper() {
        for (label, src) in [
            ("travel.rs", include_str!("travel.rs")),
            ("guard.rs", include_str!("guard.rs")),
            ("escort.rs", include_str!("escort.rs")),
            ("follow.rs", include_str!("follow.rs")),
        ] {
            assert!(
                !src.contains(".waypoints.clone()"),
                "{label} re-introduced the per-tick waypoint clone (#3269)"
            );
        }
        for (label, src) in [
            ("travel.rs", include_str!("travel.rs")),
            ("guard.rs", include_str!("guard.rs")),
            ("escort.rs", include_str!("escort.rs")),
        ] {
            assert!(
                src.contains("for p in scratch.pending.drain(..)"),
                "{label} must consume its pending scratch by value"
            );
        }
        assert!(
            include_str!("follow.rs").contains("std::mem::take(&mut d.waypoints)"),
            "follow.rs must hand its waypoints to the stepper, not clone them"
        );
    }

    /// A 2-triangle quad on the XZ plane spanning `[0,10] x [0,10]`
    /// (Y-up), split along the diagonal: `(0,0)-(10,0)-(10,10)` and
    /// `(0,0)-(10,10)-(0,10)`, sharing the `(0,0)-(10,10)` diagonal edge
    /// (vertex indices 0 and 2). Vertices are stored pre-converted from
    /// the Z-up authoring space this fixture stands in for, so
    /// `vertex_yup`'s conversion round-trips back to these Y-up
    /// coordinates exactly — built via `zup_to_yup_pos`'s own inverse
    /// `(x, y, z) -> (x, -z, y)` so the fixture doesn't have to hardcode
    /// the conversion twice.
    fn zup_from_yup(p: [f32; 3]) -> [f32; 3] {
        [p[0], -p[2], p[1]]
    }

    fn two_triangle_quad() -> NavmRecord {
        let yup_verts = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 0.0, 10.0],
            [0.0, 0.0, 10.0],
        ];
        NavmRecord {
            vertices: yup_verts.iter().map(|v| zup_from_yup(*v)).collect(),
            triangles: vec![
                NavmTriangle {
                    vertices: [0, 1, 2],
                    edge_neighbours: [None, Some(1), None],
                    flags: 0,
                },
                NavmTriangle {
                    vertices: [0, 2, 3],
                    edge_neighbours: [Some(0), None, None],
                    flags: 0,
                },
            ],
            ..NavmRecord::default()
        }
    }

    /// Same shape as [`two_triangle_quad`], offset `x_offset` world units
    /// along X and tagged with `form_id` — a second navmesh tile for
    /// cross-tile (Phase 2, #3802) tests. `x_offset = 10.0` makes this
    /// tile's `[10,20] x [0,10]` footprint share its entire `x = 10` edge
    /// with `two_triangle_quad`'s `[0,10] x [0,10]` footprint exactly (no
    /// quantization tolerance needed), giving triangle 0's `(10,0,0)-
    /// (10,0,10)` border edge a real geometric portal to match.
    fn adjacent_quad(form_id: u32, x_offset: f32) -> NavmRecord {
        let yup_verts = [
            [x_offset, 0.0, 0.0],
            [x_offset + 10.0, 0.0, 0.0],
            [x_offset + 10.0, 0.0, 10.0],
            [x_offset, 0.0, 10.0],
        ];
        NavmRecord {
            form_id,
            vertices: yup_verts.iter().map(|v| zup_from_yup(*v)).collect(),
            triangles: vec![
                NavmTriangle {
                    vertices: [0, 1, 2],
                    edge_neighbours: [None, Some(1), None],
                    flags: 0,
                },
                NavmTriangle {
                    vertices: [0, 2, 3],
                    edge_neighbours: [Some(0), None, None],
                    flags: 0,
                },
            ],
            ..NavmRecord::default()
        }
    }

    // ── Phase 2 — cross-tile geometric join (#3802) ───────────────────

    #[test]
    fn border_edges_are_the_quads_perimeter_not_the_internal_diagonal() {
        let navm = two_triangle_quad();
        let mut edges: Vec<(u16, u16)> = border_edges(&navm)
            .into_iter()
            .map(|(_, a, b)| ordered_index_pair(a, b))
            .collect();
        edges.sort();
        // Perimeter: (0,1) bottom, (1,2) right, (2,3) top, (0,3) left.
        // The internal diagonal (0,2) — shared by both triangles — must
        // NOT appear; it's the one pair `border_edges` should exclude.
        assert_eq!(edges, vec![(0, 1), (0, 3), (1, 2), (2, 3)]);
    }

    #[test]
    fn cross_tile_graph_finds_the_shared_border_between_adjacent_tiles() {
        let tile_a = adjacent_quad(1, 0.0);
        let tile_b = adjacent_quad(2, 10.0);
        let graph = CrossTileGraph::build([&tile_a, &tile_b].into_iter());

        // Tile A's triangle 0 owns the shared `x=10` edge; tile B's
        // triangle 1 owns the matching edge from its own side (derived by
        // hand in this fn's caller-facing doc comment).
        let neighbours = graph.neighbours((1, 0));
        assert_eq!(
            neighbours.len(),
            1,
            "exactly one portal, to tile B's triangle 1"
        );
        let (node, midpoint) = neighbours[0];
        assert_eq!(node, (2, 1));
        assert_eq!(midpoint, Vec3::new(10.0, 0.0, 5.0));

        // Symmetric from the other side.
        let back = graph.neighbours((2, 1));
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].0, (1, 0));

        // No other triangle in either tile borders anything cross-tile.
        assert!(graph.neighbours((1, 1)).is_empty());
        assert!(graph.neighbours((2, 0)).is_empty());
    }

    #[test]
    fn cross_tile_graph_does_not_connect_tiles_that_do_not_touch() {
        let tile_a = adjacent_quad(1, 0.0);
        let tile_b = adjacent_quad(2, 1000.0); // far away, no shared vertices
        let graph = CrossTileGraph::build([&tile_a, &tile_b].into_iter());
        assert!(graph.neighbours((1, 0)).is_empty());
        assert!(graph.neighbours((1, 1)).is_empty());
    }

    #[test]
    fn astar_cross_tile_path_crosses_the_shared_portal() {
        let tile_a = adjacent_quad(1, 0.0);
        let tile_b = adjacent_quad(2, 10.0);
        let navm_by_form: HashMap<u32, &NavmRecord> =
            [(1, &tile_a), (2, &tile_b)].into_iter().collect();
        let graph = CrossTileGraph::build([&tile_a, &tile_b].into_iter());

        let corridor = astar_cross_tile_path(&navm_by_form, &graph, (1, 0), (2, 1))
            .expect("tile 0's triangle 0 reaches tile B's triangle 1 via the portal");
        assert_eq!(corridor, vec![(1, 0), (2, 1)]);
    }

    #[test]
    fn path_from_resident_tiles_crosses_a_tile_boundary() {
        let mut world = byroredux_core::ecs::World::new();
        world.register::<crate::components::NavmeshTile>();
        let a = world.spawn();
        world.insert(a, crate::components::NavmeshTile(adjacent_quad(1, 0.0)));
        let b = world.spawn();
        world.insert(b, crate::components::NavmeshTile(adjacent_quad(2, 10.0)));
        let tiles = world
            .query::<crate::components::NavmeshTile>()
            .expect("NavmeshTile registered");

        let start = Vec3::new(8.0, 0.0, 2.0); // tile A, triangle 0
        let goal = Vec3::new(12.0, 0.0, 8.0); // tile B, triangle 1

        let path = path_from_resident_tiles(&tiles, start, goal)
            .expect("A's triangle 0 and B's triangle 1 are joined by the geometric portal");
        // `path_from_resident_tiles` never re-includes `current`: just the
        // portal midpoint, then the goal.
        assert_eq!(path, vec![Vec3::new(10.0, 0.0, 5.0), goal]);
    }

    #[test]
    fn path_from_resident_tiles_still_prefers_the_same_tile_when_both_points_are_on_it() {
        // Regression: Phase 2's fallback must not change Phase 1's
        // behavior for the common case — a second, unrelated resident
        // tile in the query must not perturb a same-tile result.
        let mut world = byroredux_core::ecs::World::new();
        world.register::<crate::components::NavmeshTile>();
        let a = world.spawn();
        world.insert(a, crate::components::NavmeshTile(adjacent_quad(1, 0.0)));
        let b = world.spawn();
        world.insert(b, crate::components::NavmeshTile(adjacent_quad(2, 10.0)));
        let tiles = world
            .query::<crate::components::NavmeshTile>()
            .expect("NavmeshTile registered");

        let start = Vec3::new(8.0, 0.0, 2.0); // tile A, triangle 0
        let goal = Vec3::new(2.0, 0.0, 8.0); // tile A, triangle 1 — same tile as start

        let path = path_from_resident_tiles(&tiles, start, goal).expect("same-tile path exists");
        // Exactly Phase 1's single-tile result: one portal midpoint (the
        // shared diagonal), then goal — no cross-tile hop involved.
        assert_eq!(path, vec![Vec3::new(5.0, 0.0, 5.0), goal]);
    }

    /// A 4-triangle strip forming a `[0,40] x [0,10]` hallway (Y-up),
    /// four quads in a row, each split into 2 triangles, chained
    /// `0-1-2-3-4-5-6-7` via `edge_neighbours` so a path from one end to
    /// the other must cross every triangle.
    fn hallway_strip() -> NavmRecord {
        let mut yup_verts = Vec::new();
        for i in 0..5u32 {
            let x = i as f32 * 10.0;
            yup_verts.push([x, 0.0, 0.0]); // vertex 2*i   (near/z=0 side)
            yup_verts.push([x, 0.0, 10.0]); // vertex 2*i+1 (far/z=10 side)
        }
        let mut triangles = Vec::new();
        for i in 0..4u16 {
            // Vertex indices for this quad's four corners.
            let v_near_low = 2 * i;
            let v_far_low = 2 * i + 1;
            let v_near_high = 2 * i + 2;
            let v_far_high = 2 * i + 3;
            // Triangle indices: two triangles pushed per quad, so quad i's
            // pair sits at 2*i (first half) and 2*i+1 (second half).
            let tri_first_half = 2 * i;
            let tri_second_half = 2 * i + 1;
            let next_quad_first_half = 2 * i + 2;
            triangles.push(NavmTriangle {
                vertices: [v_near_low, v_far_low, v_near_high],
                edge_neighbours: [None, Some(tri_second_half), None],
                flags: 0,
            });
            triangles.push(NavmTriangle {
                vertices: [v_far_low, v_far_high, v_near_high],
                edge_neighbours: [
                    None,
                    (i < 3).then_some(next_quad_first_half),
                    Some(tri_first_half),
                ],
                flags: 0,
            });
        }
        NavmRecord {
            vertices: yup_verts.iter().map(|v| zup_from_yup(*v)).collect(),
            triangles,
            ..NavmRecord::default()
        }
    }

    #[test]
    fn zup_yup_round_trip_matches_the_real_conversion() {
        let yup = [1.0, 2.0, 3.0];
        let zup = zup_from_yup(yup);
        assert_eq!(zup_to_yup_pos(zup), yup);
    }

    #[test]
    fn locates_the_containing_triangle_on_each_side_of_the_diagonal() {
        let navm = two_triangle_quad();
        assert_eq!(
            find_containing_triangle(&navm, Vec3::new(8.0, 0.0, 2.0)),
            Some(0)
        );
        assert_eq!(
            find_containing_triangle(&navm, Vec3::new(2.0, 0.0, 8.0)),
            Some(1)
        );
    }

    #[test]
    fn returns_none_for_a_point_outside_every_triangle() {
        let navm = two_triangle_quad();
        assert_eq!(
            find_containing_triangle(&navm, Vec3::new(100.0, 0.0, 100.0)),
            None
        );
    }

    #[test]
    fn same_triangle_start_and_goal_is_a_trivial_direct_path() {
        let navm = two_triangle_quad();
        let path = find_path_within_tile(&navm, Vec3::new(1.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 2.0))
            .expect("both points are on triangle 0");
        assert_eq!(
            path,
            vec![Vec3::new(1.0, 0.0, 1.0), Vec3::new(2.0, 0.0, 2.0)]
        );
    }

    #[test]
    fn crosses_the_shared_diagonal_when_start_and_goal_are_on_different_triangles() {
        let navm = two_triangle_quad();
        let start = Vec3::new(8.0, 0.0, 2.0); // triangle 0
        let goal = Vec3::new(2.0, 0.0, 8.0); // triangle 1
        let path = find_path_within_tile(&navm, start, goal).expect("adjacent via the diagonal");
        // start, one portal midpoint (the shared 0-2 diagonal's midpoint), goal.
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], start);
        assert_eq!(path[2], goal);
        assert_eq!(path[1], Vec3::new(5.0, 0.0, 5.0)); // midpoint of (0,0)-(10,10)
    }

    #[test]
    fn walks_a_multi_triangle_corridor_end_to_end() {
        let navm = hallway_strip();
        let start = Vec3::new(1.0, 0.0, 1.0); // triangle 0, far end
        let goal = Vec3::new(39.0, 0.0, 9.0); // last triangle, other far end
        let path = find_path_within_tile(&navm, start, goal).expect("hallway is fully connected");
        assert_eq!(path.first().copied(), Some(start));
        assert_eq!(path.last().copied(), Some(goal));
        // Must cross through every intermediate portal, not teleport.
        assert!(
            path.len() > 2,
            "a 4-triangle corridor needs intermediate waypoints, got {path:?}"
        );
        // Every intermediate waypoint should be a monotonically increasing
        // step down the hallway's X axis -- not strictly required for
        // correctness, but a good smoke check that the corridor wasn't
        // reversed or looped.
        for pair in path.windows(2) {
            assert!(
                pair[1].x >= pair[0].x - 1e-4,
                "waypoints should progress down the hallway, got {path:?}"
            );
        }
    }

    #[test]
    fn returns_none_when_goal_does_not_localize_onto_the_tile() {
        let navm = two_triangle_quad();
        assert_eq!(
            find_path_within_tile(
                &navm,
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(500.0, 0.0, 500.0)
            ),
            None
        );
    }

    #[test]
    fn shared_edge_is_derived_from_vertex_index_intersection() {
        let a = NavmTriangle {
            vertices: [0, 1, 2],
            edge_neighbours: [None, None, None],
            flags: 0,
        };
        let b = NavmTriangle {
            vertices: [1, 2, 3],
            edge_neighbours: [None, None, None],
            flags: 0,
        };
        let edge = shared_edge(&a, &b).expect("triangles share vertices 1 and 2");
        assert!(edge == (1, 2) || edge == (2, 1));
    }

    #[test]
    fn shared_edge_is_none_for_non_adjacent_triangles() {
        let a = NavmTriangle {
            vertices: [0, 1, 2],
            edge_neighbours: [None, None, None],
            flags: 0,
        };
        let b = NavmTriangle {
            vertices: [3, 4, 5],
            edge_neighbours: [None, None, None],
            flags: 0,
        };
        assert_eq!(shared_edge(&a, &b), None);
    }

    // ── resolve_cached_waypoints: hit/miss + effective-goal contract ──

    use crate::components::NavPath;
    use std::collections::VecDeque;

    #[test]
    fn resolve_cached_waypoints_reuses_within_threshold_and_keeps_the_original_goal() {
        let original_goal = Vec3::new(100.0, 0.0, 100.0);
        let cached = NavPath {
            goal: original_goal,
            residency_generation: 0,
            waypoints: VecDeque::from(vec![Vec3::new(50.0, 0.0, 50.0), original_goal]),
        };
        // New goal is close (within threshold) but NOT identical — this is
        // exactly follow_system's per-tick live-target-jitter case.
        let new_goal = Vec3::new(105.0, 0.0, 100.0);

        let (effective_goal, waypoints) =
            resolve_cached_waypoints(Some(&cached), None, Vec3::ZERO, new_goal, 10.0, 0);

        assert_eq!(
            effective_goal, original_goal,
            "a cache hit must keep the ORIGINAL goal, not drift toward the new one \
             (regression: this exact bug shipped once and was caught by \
             follow_system's own test)"
        );
        assert_eq!(waypoints, cached.waypoints);
    }

    #[test]
    fn resolve_cached_waypoints_recomputes_beyond_threshold_and_advances_the_goal() {
        let cached = NavPath {
            goal: Vec3::new(100.0, 0.0, 100.0),
            residency_generation: 0,
            waypoints: VecDeque::from(vec![Vec3::new(100.0, 0.0, 100.0)]),
        };
        let new_goal = Vec3::new(500.0, 0.0, 500.0); // far beyond any reasonable threshold

        // No tiles (`None`) — recompute finds nothing, so this also pins
        // the "cache the negative result" contract: effective_goal still
        // advances to the new goal even though waypoints comes back empty.
        let (effective_goal, waypoints) =
            resolve_cached_waypoints(Some(&cached), None, Vec3::ZERO, new_goal, 10.0, 0);

        assert_eq!(effective_goal, new_goal);
        assert!(waypoints.is_empty());
    }

    /// #3256 (ECS-2026-08-24-08) regression — the negative cache must be
    /// recoverable when navmesh residency changes.
    ///
    /// The failure this pins: a frozen-goal actor (travel/guard use a `0.0`
    /// repath threshold) resolves its destination on a tick where no tile
    /// localizes it. The deliberately-cached empty result then matches the
    /// frozen goal *bit-identically* forever, so `path_from_resident_tiles`
    /// is never retried for the rest of that leash — even after the relevant
    /// tile streams in. Goal distance alone cannot express "the resident set
    /// changed"; the residency generation can.
    #[test]
    fn resolve_cached_waypoints_retries_a_negative_result_after_residency_changes() {
        // The cached negative result below is "already tried this goal, no
        // resident-tile path found", stamped with the generation it was
        // computed under.
        //
        // The tile that "streams in": a quad the goal actually lies on, so a
        // retry produces a real path and the two arms are distinguishable.
        // Without a resident tile both arms return empty and the test could
        // not fail, which is the trap the first draft of this test fell into.
        let mut world = byroredux_core::ecs::World::new();
        world.register::<crate::components::NavmeshTile>();
        let tile_entity = world.spawn();
        world.insert(
            tile_entity,
            crate::components::NavmeshTile(two_triangle_quad()),
        );
        let tiles = world
            .query::<crate::components::NavmeshTile>()
            .expect("NavmeshTile registered");
        let inside_goal = Vec3::new(9.0, 0.0, 9.0);
        let start = Vec3::new(1.0, 0.0, 1.0);
        let cached = NavPath {
            goal: inside_goal,
            residency_generation: 7,
            waypoints: VecDeque::new(),
        };

        // Same generation, frozen goal (`0.0` threshold): still a hit, so the
        // negative cache keeps doing its job and an off-navmesh actor does not
        // re-run the search every tick — even though a tile is now resident
        // and a search WOULD succeed.
        let (effective_goal, waypoints) =
            resolve_cached_waypoints(Some(&cached), Some(&tiles), start, inside_goal, 0.0, 7);
        assert_eq!(effective_goal, inside_goal);
        assert!(
            waypoints.is_empty(),
            "unchanged residency must still hit the negative cache — that is \
             the whole point of caching it (travel_system Pass 1a)"
        );

        // The tile streamed in: same goal, bit-identical, but the generation
        // moved, so the cache must MISS and the search must actually re-run.
        let (effective_goal, waypoints) =
            resolve_cached_waypoints(Some(&cached), Some(&tiles), start, inside_goal, 0.0, 8);
        assert_eq!(
            effective_goal, inside_goal,
            "a miss still advances the effective goal to the requested one"
        );
        assert!(
            !waypoints.is_empty(),
            "#3256: the whole defect — a frozen goal plus a cached empty path \
             matched bit-identically forever, so path_from_resident_tiles was \
             never retried once the tile became resident"
        );
        assert_eq!(
            waypoints.back().copied(),
            Some(inside_goal),
            "a recomputed path must end at the requested goal"
        );
    }

    /// #3256 sibling — a *positive* cached path is invalidated by a residency
    /// change too, not just the empty one. A path computed across tiles that
    /// have since been torn down is worse than no path: it walks the actor
    /// along waypoints derived from geometry that is no longer resident.
    #[test]
    fn resolve_cached_waypoints_invalidates_a_positive_path_after_residency_changes() {
        let goal = Vec3::new(100.0, 0.0, 100.0);
        let cached = NavPath {
            goal,
            residency_generation: 3,
            waypoints: VecDeque::from(vec![Vec3::new(50.0, 0.0, 50.0), goal]),
        };

        let (_, reused) = resolve_cached_waypoints(Some(&cached), None, Vec3::ZERO, goal, 10.0, 3);
        assert_eq!(reused, cached.waypoints, "same generation → reuse");

        let (_, recomputed) =
            resolve_cached_waypoints(Some(&cached), None, Vec3::ZERO, goal, 10.0, 4);
        assert!(
            recomputed.is_empty(),
            "#3256: a residency change must discard the stale waypoints rather \
             than walk the actor along tiles that may no longer be resident"
        );
    }

    #[test]
    fn resolve_cached_waypoints_with_no_cache_at_all_recomputes() {
        let (effective_goal, waypoints) =
            resolve_cached_waypoints(None, None, Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0), 10.0, 0);
        assert_eq!(effective_goal, Vec3::new(1.0, 0.0, 1.0));
        assert!(waypoints.is_empty());
    }
}
