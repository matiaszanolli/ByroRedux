//! Pure-function tests for the streaming control loop (M40 Phase 1a).
//!
//! These cover [`compute_streaming_deltas`] and [`world_pos_to_grid`]
//! — the testable seams of the streaming system that don't require
//! Vulkan / a worker thread / a parsed `EsmIndex`. They pin the diff
//! semantics so a future refactor of the App-level driver can't
//! silently miscompute the load / unload set.

use super::{compute_streaming_deltas, world_pos_to_grid, LoadedCell, StreamingDeltas};
use byroredux_core::ecs::storage::EntityId;
use std::collections::HashMap;

fn loaded_set(coords: &[(i32, i32)]) -> HashMap<(i32, i32), LoadedCell> {
    coords
        .iter()
        .copied()
        .enumerate()
        .map(|(i, c)| {
            (
                c,
                LoadedCell {
                    cell_root: i as EntityId,
                },
            )
        })
        .collect()
}

#[test]
fn empty_loaded_set_loads_full_radius() {
    // Player at (0,0), radius_load = 1 → expect 9 cells loaded
    // (3×3 grid centered on player), nothing to unload.
    let deltas = compute_streaming_deltas(&HashMap::new(), (0, 0), 1, 2);
    assert_eq!(deltas.to_load.len(), 9);
    assert!(deltas.to_unload.is_empty());
    // Closest-first: the player's own cell should land at index 0.
    assert_eq!(deltas.to_load[0], (0, 0));
}

#[test]
fn fully_loaded_set_no_work() {
    // Player at (0,0), 3×3 grid loaded → no deltas.
    let loaded = loaded_set(&[
        (-1, -1), (-1, 0), (-1, 1),
        (0, -1),  (0, 0),  (0, 1),
        (1, -1),  (1, 0),  (1, 1),
    ]);
    let deltas = compute_streaming_deltas(&loaded, (0, 0), 1, 2);
    assert_eq!(deltas, StreamingDeltas::default());
}

#[test]
fn moving_one_cell_loads_three_unloads_zero() {
    // Player crosses from (0,0) to (1,0). With radius_load=1 and
    // radius_unload=2, the new desired ring picks up 3 new cells on
    // the +x edge but nothing falls outside the unload radius yet
    // (the now-far -x edge is still within Chebyshev 2 of the new
    // player pos).
    let loaded = loaded_set(&[
        (-1, -1), (-1, 0), (-1, 1),
        (0, -1),  (0, 0),  (0, 1),
        (1, -1),  (1, 0),  (1, 1),
    ]);
    let deltas = compute_streaming_deltas(&loaded, (1, 0), 1, 2);
    // New cells along the +x edge — all Chebyshev distance 1 from
    // player, so the closest-first sort drops to lexical (gx, gy)
    // tiebreak: gy = -1, 0, 1.
    assert_eq!(deltas.to_load, vec![(2, -1), (2, 0), (2, 1)]);
    // Nothing to unload — Chebyshev distance from (1,0) to (-1,*)
    // is 2, still inside radius_unload=2.
    assert!(deltas.to_unload.is_empty());
}

#[test]
fn moving_two_cells_loads_and_unloads() {
    // Player crosses from (0,0) to (2,0). Now -x edge is at
    // Chebyshev distance 3 from player, outside unload_radius=2.
    // The new desired set centred on (2,0) overlaps the old loaded
    // set (centred on (0,0)) only in the column gx=1, so 6 of the
    // 9 desired cells are new: gx=2 (Chebyshev 1) and gx=3
    // (Chebyshev 2). Closest-first then lexical gives:
    //   gx=2 first (distance 1): gy = -1, 0, 1
    //   gx=3 next (distance 2):  gy = -1, 0, 1
    let loaded = loaded_set(&[
        (-1, -1), (-1, 0), (-1, 1),
        (0, -1),  (0, 0),  (0, 1),
        (1, -1),  (1, 0),  (1, 1),
    ]);
    let deltas = compute_streaming_deltas(&loaded, (2, 0), 1, 2);
    // (2,0) is the player's own cell (Chebyshev 0) → sorts first.
    // Then ring 1 = gx=2, gy=±1 (lexical on gy: -1, 1).
    // Then ring 2 = gx=3, gy ∈ {-1, 0, 1} (lexical on gy).
    assert_eq!(
        deltas.to_load,
        vec![(2, 0), (2, -1), (2, 1), (3, -1), (3, 0), (3, 1)]
    );
    // Old -x edge falls outside radius_unload (Chebyshev 3).
    assert_eq!(deltas.to_unload, vec![(-1, -1), (-1, 0), (-1, 1)]);
}

#[test]
fn hysteresis_prevents_boundary_thrash() {
    // Player at (1,0), 3×3 around (0,0) is loaded. radius_load=1,
    // radius_unload=2. The cell (-1,0) is at Chebyshev distance 2 from
    // player — exactly on the unload boundary, MUST stay loaded.
    // Pre-hysteresis (single radius), the same player position would
    // unload (-1,0) the moment it left the load radius and reload it
    // the next step back, thrashing every frame at the boundary.
    let loaded = loaded_set(&[
        (-1, -1), (-1, 0), (-1, 1),
        (0, -1),  (0, 0),  (0, 1),
        (1, -1),  (1, 0),  (1, 1),
    ]);
    let deltas = compute_streaming_deltas(&loaded, (1, 0), 1, 2);
    assert!(
        deltas.to_unload.is_empty(),
        "cells at exactly radius_unload=2 must stay loaded: {:?}",
        deltas.to_unload
    );
}

#[test]
fn closest_first_load_order() {
    // Empty world, player at (5, 5), radius=2 → expect 25 cells.
    // The player's own cell is index 0, then the 8 cells of the
    // immediate ring (Chebyshev=1), then the outer 16 cells
    // (Chebyshev=2). Pre-fix arbitrary ordering would surface the
    // visible cell-of-arrival as one of 25 in unspecified order.
    let deltas = compute_streaming_deltas(&HashMap::new(), (5, 5), 2, 3);
    assert_eq!(deltas.to_load.len(), 25);
    assert_eq!(deltas.to_load[0], (5, 5));
    // Indices 1..=8 are all Chebyshev distance 1.
    for c in &deltas.to_load[1..=8] {
        let d = (c.0 - 5).abs().max((c.1 - 5).abs());
        assert_eq!(d, 1, "expected ring-1 cell, got {:?}", c);
    }
    // Indices 9..=24 are all Chebyshev distance 2.
    for c in &deltas.to_load[9..=24] {
        let d = (c.0 - 5).abs().max((c.1 - 5).abs());
        assert_eq!(d, 2, "expected ring-2 cell, got {:?}", c);
    }
}

#[test]
fn radius_zero_loads_only_the_player_cell() {
    // Edge case: radius_load=0 → only the player's cell. radius_unload
    // must be >= radius_load; passing 1 to keep hysteresis valid.
    let deltas = compute_streaming_deltas(&HashMap::new(), (3, -2), 0, 1);
    assert_eq!(deltas.to_load, vec![(3, -2)]);
    assert!(deltas.to_unload.is_empty());
}

#[test]
fn world_pos_to_grid_origin() {
    assert_eq!(world_pos_to_grid(0.0, 0.0), (0, 0));
}

#[test]
fn world_pos_to_grid_positive_quadrant() {
    // Source-y axis is negated when populating world-z; cell (1, 1)
    // mid-point lives at source (4096+2048, 4096+2048, 0) which lands
    // at world (6144, *, -6144).
    assert_eq!(world_pos_to_grid(6144.0, -6144.0), (1, 1));
}

#[test]
fn world_pos_to_grid_negative_quadrant() {
    // Source (-4096, -4096, 0) → world (-4096, *, 4096) → grid (-1, -1).
    assert_eq!(world_pos_to_grid(-2048.0, 2048.0), (-1, -1));
}

#[test]
fn world_pos_to_grid_floor_semantics() {
    // floor(0.999 / 4096) = 0; floor(-0.001 / 4096) = -1. Catches
    // the off-by-one that would put a player straddling the boundary
    // into the wrong cell.
    assert_eq!(world_pos_to_grid(4095.99, 0.0), (0, 0));
    assert_eq!(world_pos_to_grid(4096.01, 0.0), (1, 0));
    assert_eq!(world_pos_to_grid(-0.01, 0.0), (-1, 0));
}
