# #1866: LC0703-01: VWD full-model cull consumer untracked — flag parses (#1731) but is never read at spawn

**Severity**: MEDIUM
**Location**: `byroredux/src/cell_loader/object_lod.rs`, `byroredux/src/cell_loader/placement_lod.rs`

The issue framed this as "the VWD record-header flag parses but has no
production consumer," suggesting the fix was to wire `is_visible_when_distant`
into the object-LOD / placement-LOD spawn paths to cull full REFRs at the
boundary.

## Investigation finding (root cause is more precise than the issue's framing)
Traced the actual z-fight mechanism instead of reverse-engineering Bethesda's
exact VWD algorithm (which the issue itself doesn't require — the coarser
ring-only rule was already meant to obsolete needing the flag for the common
case). Found a concrete, precisely-defined synchronization bug:

- `streaming.rs::compute_streaming_deltas` only unloads a full cell when
  `d > radius_unload`, where `radius_unload = radius_load + 1` (a one-cell
  hysteresis band that prevents load/unload thrash at the boundary). So a
  full cell at exactly `radius_load + 1` can still be resident.
- `object_lod.rs` / `placement_lod.rs` gated their LOD-ring load on
  `d > radius_load` (called `full_radius_load`) — NOT `radius_unload`. A quad/
  cell at exactly `radius_load + 1` was therefore LOD-eligible while a full
  cell at that same distance could still be resident under hysteresis.

This is the exact, concrete mechanism producing the z-fight the issue
describes — a real internal-consistency bug in ByroRedux's own streaming
radii, not a missing per-record VWD read.

## Fix (scope confirmed with user: object/placement only, terrain_lod.rs filed separately as #1871)
- Renamed the misleading `full_radius_load` parameter to `max_full_cell_radius`
  in both `stream_object_lod_blocks` and `stream_placement_lod_blocks` /
  `placement_lod_cells_in_radius`, with doc comments stating explicitly:
  "must be the caller's `radius_unload`, not `radius_load`."
  Extracted `object_lod_quads_in_radius` as a pure, testable function
  (mirroring `placement_lod_cells_in_radius`'s existing shape) so the ring
  math is unit-testable without a `World`/`VulkanContext`.
- Changed both call sites (`main.rs`, `scene/world_setup.rs`) to pass
  `state.radius_unload` instead of `state.radius_load` for the object-LOD
  and placement-LOD calls specifically. `terrain_lod`'s call is unchanged
  (same root cause, filed separately as #1871, out of scope here per user
  confirmation).

## Completeness Checks
- [x] **TESTS**: Added `ring_excludes_hysteresis_band_when_gated_on_radius_unload`
      to both `object_lod.rs` and `placement_lod.rs` — each pins that a cell at
      exactly `radius_load + 1` is excluded when gated on `radius_unload` (and
      explicitly reproduces the pre-fix bug when gated on `radius_load`, as a
      sanity check that the test itself would have caught the regression).
- [x] **CANONICAL-BOUNDARY**: N/A — EXAL LOD-culling logic, not the NIFAL
      material boundary, per the issue's own note.
- Sibling finding filed as #1871 (`terrain_lod.rs`'s hole-mask has the
  identical gap) rather than silently fixed or silently dropped.

---

# #1867: CONC-D1-NEW-01: bind_inverses requeue/rollback skipped on fatal queue_submit/queue_present Err path

**Severity**: LOW (informational)
**Status**: Closed with no code change, per the issue's own recommendation.

Confirmed the finding against current code: `main.rs`'s `draw_frame` match
`Err(e)` arm only logs and calls `event_loop.exit()` — no rollback/requeue
call, unlike the `Ok` arm's `#1791`/`#1796` logic. The analysis in the issue
is accurate.

The issue itself explicitly recommends no fix ("None recommended — flagging
for completeness only"), on the grounds that:
- The path is already fatal — `event_loop.exit()` fires in the same tick
  regardless of whether the CPU-side bookkeeping rolls back.
- Severity is explicitly below the "recoverable path" bar for MEDIUM.
- Not practically testable without fault-injecting the Vulkan loader.

Closed with an explanatory comment rather than shipping a speculative fix on
an untestable Vulkan fault path — consistent with this project's stance
against speculative Vulkan changes whose failure modes are invisible to
`cargo test`.
