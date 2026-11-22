//! `PGRD` pathgrid pathing — Oblivion's walk graph (PRD `ai-packages-completion`
//! Q5, AIP-WALK-003; the `PGRD` format is in `crates/plugin/src/esm/records/pathgrid.rs`).
//!
//! Oblivion authors no `NAVM`; it authors a *path grid* per cell: a set of points
//! the level designer laid on the floor, joined by edges an actor may walk along
//! (CS *Path Grid Generation*). Without it Travel / Wander / Follow / Guard / Escort
//! walk straight at their goal and the character controller stops them at the first
//! wall — the Imperial Prison's Glenroy, sent to meet the player at stage 14, stood
//! against a ledge for ever.
//!
//! The route is the shortest chain of grid points from the point nearest the actor
//! to the point nearest the goal ([`find_path_over_grid`]), then the goal itself.
//! Unlike a navmesh the grid has no surface to localise on, so "on the grid" means
//! "within [`PATHGRID_SNAP_RADIUS`] of one of its points"; a start or goal farther
//! from every point than that gets no route and walks straight, as it did before.
//!
//! Scope of this first slice: one grid at a time. A route is found only when the
//! actor and the goal snap onto the *same* cell's grid; the `PGRI` links to a
//! neighbouring cell's grid (`PathGridRecord::inter_cell`) are not followed yet.
//! Waypoints are the grid points themselves — no string-pulling.
//!
//! Every function here takes and returns engine Y-up [`Vec3`]; the grid's raw
//! Z-up points are converted as they are read, like [`super::navmesh_path`].

use byroredux_core::math::coord::zup_to_yup_pos;
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::PathGridRecord;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// How far (engine units) a start or goal may lie from the nearest grid point and
/// still count as on the grid. Measured over every `PGRD` in `Oblivion.esm`: a
/// point's nearest neighbour lies a median 257 units away (p90 394), so an actor
/// within one such spacing of a point is standing on the grid's own ground. Any
/// farther and the nearest point may be on another level or behind a wall — the
/// Emperor's party, 340 units from the nearest prison point on a gallery the grid
/// does not cover, was sent back to a stair it could not reach — so it walks
/// straight, as it did before the grid was read.
pub(crate) const PATHGRID_SNAP_RADIUS: f32 = 256.0;

fn point(grid: &PathGridRecord, index: usize) -> Vec3 {
    Vec3::from(zup_to_yup_pos(grid.points[index].position))
}

/// The grid point nearest `to`, if one lies within [`PATHGRID_SNAP_RADIUS`].
fn nearest_point(grid: &PathGridRecord, to: Vec3) -> Option<usize> {
    (0..grid.points.len())
        .map(|i| (i, point(grid, i).distance_squared(to)))
        .filter(|(_, d)| *d <= PATHGRID_SNAP_RADIUS * PATHGRID_SNAP_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

struct Scored {
    /// Path cost so far plus the straight-line distance still to go.
    estimate: f32,
    cost: f32,
    node: usize,
}

impl PartialEq for Scored {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Scored {}
impl PartialOrd for Scored {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Scored {
    /// Reversed on `estimate` so the max-heap pops the cheapest node first.
    fn cmp(&self, other: &Self) -> Ordering {
        other.estimate.total_cmp(&self.estimate)
    }
}

/// A\* over the grid's authored edges from point `start` to point `goal`: the point
/// indices in walking order, both ends included. Edges leaving the cell
/// (`None`) and indices past the point list are skipped.
fn astar(grid: &PathGridRecord, start: usize, goal: usize) -> Option<Vec<usize>> {
    let goal_at = point(grid, goal);
    let mut best = vec![f32::INFINITY; grid.points.len()];
    let mut came_from: Vec<Option<usize>> = vec![None; grid.points.len()];
    let mut open = BinaryHeap::new();
    best[start] = 0.0;
    open.push(Scored { estimate: point(grid, start).distance(goal_at), cost: 0.0, node: start });

    while let Some(Scored { cost, node, .. }) = open.pop() {
        if node == goal {
            let mut route = vec![goal];
            while let Some(previous) = came_from[*route.last()?] {
                route.push(previous);
            }
            route.reverse();
            return Some(route);
        }
        if cost > best[node] {
            continue; // a cheaper route to this point was found after this entry was pushed
        }
        let here = point(grid, node);
        let Some(edges) = grid.edges.get(node) else {
            continue; // a grid that authored no edge list for this point
        };
        for next in edges.iter().flatten().map(|&n| usize::from(n)) {
            if next >= grid.points.len() {
                continue;
            }
            let next_cost = cost + here.distance(point(grid, next));
            if next_cost < best[next] {
                best[next] = next_cost;
                came_from[next] = Some(node);
                open.push(Scored {
                    estimate: next_cost + point(grid, next).distance(goal_at),
                    cost: next_cost,
                    node: next,
                });
            }
        }
    }
    None
}

/// The waypoints from `start` to `goal` along one grid: the shortest chain of grid
/// points, then `goal`. Never includes `start` itself; always ends with `goal`.
///
/// `None` when either end is farther than [`PATHGRID_SNAP_RADIUS`] from every point,
/// when both snap to the same point (the straight walk is already the route), or when
/// the grid's edges do not connect the two points — the caller then walks straight.
pub(crate) fn find_path_over_grid(grid: &PathGridRecord, start: Vec3, goal: Vec3) -> Option<Vec<Vec3>> {
    let first = nearest_point(grid, start)?;
    let last = nearest_point(grid, goal)?;
    if first == last {
        return None;
    }
    let route = astar(grid, first, last)?;
    let mut waypoints: Vec<Vec3> = route.into_iter().map(|i| point(grid, i)).collect();
    // The point nearest the actor can lie behind it; do not walk back to it when the
    // actor is already past it on the way to the next one.
    while waypoints.len() >= 2 && start.distance(waypoints[1]) < waypoints[0].distance(waypoints[1]) {
        waypoints.remove(0);
    }
    waypoints.push(goal);
    Some(waypoints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::PathGridPoint;

    /// A grid from engine-space points and undirected index pairs (the file lists
    /// both directions of every edge).
    fn grid(points: &[[f32; 3]], links: &[(u16, u16)]) -> PathGridRecord {
        let mut edges = vec![Vec::new(); points.len()];
        for &(a, b) in links {
            edges[usize::from(a)].push(Some(b));
            edges[usize::from(b)].push(Some(a));
        }
        PathGridRecord {
            points: points
                .iter()
                .zip(&edges)
                .map(|(p, e)| PathGridPoint {
                    // engine Y-up (x, y, z) → file Z-up (x, -z, y)
                    position: [p[0], -p[2], p[1]],
                    connection_count: e.len() as u8,
                })
                .collect(),
            declared_point_count: points.len() as u16,
            edges,
            ..Default::default()
        }
    }

    /// An L: a wall between (0,0,0) and (200,0,0) forces the route round (0,0,200)–(200,0,200).
    fn l_grid() -> PathGridRecord {
        grid(
            &[[0.0, 0.0, 0.0], [0.0, 0.0, 200.0], [200.0, 0.0, 200.0], [200.0, 0.0, 0.0]],
            &[(0, 1), (1, 2), (2, 3)],
        )
    }

    #[test]
    fn the_route_follows_the_authored_edges_then_ends_at_the_goal() {
        let route = find_path_over_grid(&l_grid(), Vec3::new(10.0, 0.0, 0.0), Vec3::new(190.0, 0.0, 0.0)).unwrap();
        assert_eq!(
            route,
            vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 200.0),
                Vec3::new(200.0, 0.0, 200.0),
                Vec3::new(200.0, 0.0, 0.0),
                Vec3::new(190.0, 0.0, 0.0),
            ],
            "round the L, not through the wall"
        );
    }

    #[test]
    fn the_point_behind_the_actor_is_not_walked_back_to() {
        // 50 along the way from point 0 to point 1: point 0 is nearest but behind.
        let route = find_path_over_grid(&l_grid(), Vec3::new(0.0, 0.0, 50.0), Vec3::new(190.0, 0.0, 0.0)).unwrap();
        assert_eq!(route[0], Vec3::new(0.0, 0.0, 200.0));
    }

    #[test]
    fn a_goal_off_the_grid_gets_no_route() {
        assert!(find_path_over_grid(&l_grid(), Vec3::ZERO, Vec3::new(5000.0, 0.0, 0.0)).is_none());
        assert!(find_path_over_grid(&l_grid(), Vec3::new(5000.0, 0.0, 0.0), Vec3::ZERO).is_none());
    }

    #[test]
    fn both_ends_on_one_point_walk_straight() {
        assert!(find_path_over_grid(&l_grid(), Vec3::new(5.0, 0.0, 0.0), Vec3::new(-5.0, 0.0, 9.0)).is_none());
    }

    #[test]
    fn unconnected_points_get_no_route() {
        let split = grid(&[[0.0, 0.0, 0.0], [300.0, 0.0, 0.0]], &[]);
        assert!(find_path_over_grid(&split, Vec3::ZERO, Vec3::new(300.0, 0.0, 0.0)).is_none());
    }

    #[test]
    fn an_edge_that_leaves_the_cell_or_points_past_the_list_is_skipped() {
        let mut g = l_grid();
        g.edges[0].push(None);
        g.edges[0].push(Some(77));
        assert!(find_path_over_grid(&g, Vec3::new(10.0, 0.0, 0.0), Vec3::new(190.0, 0.0, 0.0)).is_some());
    }

    #[test]
    fn a_grid_with_no_edges_gets_no_route() {
        let mut g = l_grid();
        g.edges.clear();
        assert!(find_path_over_grid(&g, Vec3::new(10.0, 0.0, 0.0), Vec3::new(190.0, 0.0, 0.0)).is_none());
    }
}
