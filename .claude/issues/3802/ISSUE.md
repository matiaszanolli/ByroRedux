# #3802 — EX-16: cross-tile NAVM path connectivity via geometric vertex-matching join

Split from #2372 (EX-16 acceptance criterion 2). Labels: enhancement, ai,
terrain-exterior.

## Status at fetch (verified 2026-08-31)
Load/unload already worked. Cross-tile connectivity was explicitly not
implemented — `navmesh_path.rs` was deliberately single-tile only, because
`NavmExternalConnection`'s source-triangle field was corpus-verified
unrecoverable (#3300, 2026-08-27). That same investigation identified the
resolution already: recover the join **geometrically** — two tiles' border
edges that share exact vertex positions are a real portal — with a
measured bound (67.1% of adjacent `FalloutNV.esm` mesh pairs share at
least one exact vertex at 1e-2 world-unit precision). The research spike
was done; only the implementation ("a decision to build it") was pending.

## Fix
`byroredux/src/systems/navmesh_path.rs`:
- `border_edges()` — derives a mesh's border edges (vertex-index pairs
  belonging to exactly one triangle) directly from vertex-index-set
  membership, independent of `edge_neighbours`'s still-unconfirmed slot
  ordering. A triangle's 3 vertices give its 3 edges unambiguously
  regardless of winding, so this adds no new assumption.
- `CrossTileGraph::build()` — buckets every resident tile's border edges
  by quantized (1e-2 world-unit) world-space position; two tiles' edges
  landing in the same bucket are a portal, carrying the shared midpoint.
- `astar_cross_tile_path()` — A\* over `(mesh_form, triangle)` nodes,
  walking both `edge_neighbours` (within-tile) and the geometric portal
  graph (cross-tile), same centroid-distance cost model as Phase 1's
  single-tile search so the straight-line-to-goal heuristic stays
  admissible across tile boundaries.
- `cross_tile_corridor_to_waypoints()` — extends waypoint extraction across
  a mixed corridor: within-tile steps reuse Phase 1's `shared_edge`
  midpoint exactly; cross-tile steps use the matched portal midpoint.
- `path_from_resident_tiles()` — tries Phase 1's same-tile search first
  (byte-identical behavior/tests for that case) and only falls through to
  the cross-tile graph when `current`/`goal` localize onto different
  resident tiles.

Accuracy bound accepted and stated, not silently assumed away: ~67% of
adjacent tile pairs join automatically; the remaining ~33% (small
authored coordinate drift between separately-baked meshes) needs a
tolerance sweep this issue does not attempt — same no-guessing posture as
every other unconfirmed NAVM field in this codebase. Documented in both
`docs/engine/navmesh-pathfinding.md` §9 Phase 2 and
`docs/engine/exterior-readiness-plan.md`'s EX-16 item 3 entry.

6 new tests: `border_edges` pinned against the quad fixture's known
perimeter, the portal graph's adjacency/symmetry/non-adjacency, a direct
A\* corridor test, a full ECS-level `path_from_resident_tiles` test
crossing two distinct `NavmeshTile` entities, and a same-tile regression
test confirming Phase 2 didn't change Phase 1's output for the common
case.

`cargo test -q -p byroredux` (1715 tests) and the full workspace
`cargo test -q` (93 test binaries) both pass clean with zero new warnings.

Door-aware pathing and FO4 `NVNM` body support remain open, unrelated
follow-ups already tracked separately.
