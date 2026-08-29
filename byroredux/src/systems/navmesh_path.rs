//! Single-tile NAVM pathfinding — Phases 1 and 3 of
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
//! Deliberately **single-tile only**: `external_connections` (cross-tile
//! links) aren't walked here. Phase 2 (cross-tile search) turned out to
//! be genuinely **blocked**, not just unscheduled — see the design doc's
//! §9 Phase 2 entry for the corpus-verified finding (`NavmExternalConnection`
//! has no confirmed source-triangle field). A caller with a start/goal
//! that don't both localize onto the same resident tile gets `None`,
//! same "degrade, don't fail" posture the design doc's §4 establishes for
//! the residency boundary generally.
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
/// resident tile localizes `current`, or the one that does can't reach
/// `goal` within itself — callers fall back to walking straight at
/// `goal`, exactly today's pre-pathing behavior (design doc §4's
/// residency-boundary degrade, applied to "no tile at all" as well as
/// "goal outside the known corridor").
///
/// Cross-tile search (trying a *different* resident tile than the one
/// `current` localizes on) is Phase 2, genuinely blocked — see the
/// design doc's §9 Phase 2 entry — so this only ever searches the single
/// tile `current` is standing on.
pub(crate) fn path_from_resident_tiles(
    tiles: &byroredux_core::ecs::QueryRead<'_, crate::components::NavmeshTile>,
    current: Vec3,
    goal: Vec3,
) -> Option<Vec<Vec3>> {
    tiles.iter().find_map(|(_, tile)| {
        find_path_within_tile(&tile.0, current, goal).map(|path| path.into_iter().skip(1).collect())
    })
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
) -> (Vec3, std::collections::VecDeque<Vec3>) {
    match cached {
        Some(path) if path.goal.distance(goal) <= repath_threshold => {
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
            waypoints: VecDeque::from(vec![Vec3::new(50.0, 0.0, 50.0), original_goal]),
        };
        // New goal is close (within threshold) but NOT identical — this is
        // exactly follow_system's per-tick live-target-jitter case.
        let new_goal = Vec3::new(105.0, 0.0, 100.0);

        let (effective_goal, waypoints) =
            resolve_cached_waypoints(Some(&cached), None, Vec3::ZERO, new_goal, 10.0);

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
            waypoints: VecDeque::from(vec![Vec3::new(100.0, 0.0, 100.0)]),
        };
        let new_goal = Vec3::new(500.0, 0.0, 500.0); // far beyond any reasonable threshold

        // No tiles (`None`) — recompute finds nothing, so this also pins
        // the "cache the negative result" contract: effective_goal still
        // advances to the new goal even though waypoints comes back empty.
        let (effective_goal, waypoints) =
            resolve_cached_waypoints(Some(&cached), None, Vec3::ZERO, new_goal, 10.0);

        assert_eq!(effective_goal, new_goal);
        assert!(waypoints.is_empty());
    }

    #[test]
    fn resolve_cached_waypoints_with_no_cache_at_all_recomputes() {
        let (effective_goal, waypoints) =
            resolve_cached_waypoints(None, None, Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0), 10.0);
        assert_eq!(effective_goal, Vec3::new(1.0, 0.0, 1.0));
        assert!(waypoints.is_empty());
    }
}
