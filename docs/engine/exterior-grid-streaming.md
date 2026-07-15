# Exterior Grid Streaming: Loading, Following the Player, and Door Swaps

Companion to [Pipeline Overview](pipeline-overview.md), which traces a
single interior cell load end-to-end and explicitly skips this flow. This
doc covers what happens outside an interior box: resolving a worldspace
grid cell to terrain + REFRs, the background pre-parse worker that keeps
streaming off the main thread, and the two ways a loaded scene changes
wholesale — walking across a cell boundary, and a door teleport.

> **Currency note.** Verified against the tree as of 2026-07-15, source
> citations below. Two other docs currently misdescribe this system and
> are due a fix: **README.md**'s "State" section still reads "World
> streaming Phase 1 ... + Phase 2 ... shipped; multi-cell grid pending"
> and cites `script.activate <door>` as the swap trigger — multi-cell
> grid streaming is not pending, it shipped (ROADMAP's M40 row, closed
> 2026-05-24), and the actual trigger command is `door.teleport`, not
> `script.activate`. **ROADMAP.md**'s M40 row itself cites
> `byroredux/src/main.rs:1100-1113` and `radius_load=3` for the
> transition/streaming radius — both moved: the logic now lives in
> `byroredux/src/app_step.rs` (per `#1858`/TD1-003) with
> `DEFAULT_TRANSITION_RADIUS = 5`. Not fixed here — flagged so the next
> doc pass on those two files knows where to look.

## 1. CLI entry: `--esm X.esm --grid gx,gy --radius N`

`scene::setup_scene` (`byroredux/src/scene.rs:75` — the same dispatch
function [Pipeline Overview](pipeline-overview.md) covers for `--cell`)
handles `--grid` at `scene.rs:228-263`. `--radius` is parsed by
`parse_exterior_radius` (`scene.rs:50-55`) and **clamped to `1..=12`**,
default 5 — CLAUDE.md's Quick Reference and README's `--radius 3
(1..=7)` note are both stale on the actual bound.

There's no separate "bulk grid loader" — the CLI's initial load *is* the
streaming system's first batch. Dispatch calls
`cell_loader::build_exterior_world_context` (`byroredux/src/cell_loader/exterior.rs:83`)
to build a once-per-session `ExteriorWorldContext` (worldspace selection:
`--wrld` override → worldspace containing the requested grid → a
preferred-game-default list → most-cells fallback), constructs
`WorldStreamingState::new(...)` (`byroredux/src/streaming.rs:244`), and
calls `stream_initial_radius` (`byroredux/src/scene/world_setup.rs:456`)
to synchronously populate the starting grid.

## 2. WRLD/LAND → terrain + REFRs

WRLD parsing is `parse_wrld_group`/`parse_wrld_children`
(`crates/plugin/src/esm/cell/wrld.rs:15,186`). LAND heightmap/splat data
is `parse_land_record` (`crates/plugin/src/esm/cell/walkers.rs:1091`),
decoding VHGT/VNML/VCLR into
`EsmIndex.cells.exterior_cells[worldspace][(gx,gy)].landscape`.

Consumption is `cell_loader::load_one_exterior_cell`
(`byroredux/src/cell_loader/exterior.rs:265`): looks up `(gx,gy)`, calls
`terrain::spawn_terrain_mesh` (`byroredux/src/cell_loader/terrain.rs:307`)
for heightmap+splat geometry, `water::spawn_water_plane` for XCLW/default
water, then the **same** `references::load_references` path
[Pipeline Overview](pipeline-overview.md) traces for interior placed
REFRs (plus FO4 precombine absorption). Exterior and interior cells
converge on identical REFR-spawn machinery — the only difference is what
feeds it (heightmap-driven terrain vs. an interior's floor/wall meshes).

See [ESM Records](esm-records.md) for the WRLD/CELL/REFR record layout
and [EXAL — Exterior Abstraction Layer](exal.md) for how the resulting
terrain/sky/sun/weather/water state is translated into a canonical,
game-agnostic representation for rendering.

## 3. Radius → grid cells

Chebyshev (square) neighborhoods, not circular, computed in two places
that agree on shape: the initial-load loop in `stream_initial_radius`
(`byroredux/src/scene/world_setup.rs:456`), and the steady-state diff in
`compute_streaming_deltas` (`byroredux/src/streaming.rs:700-745`) — a
pure function: for `dx, dy` in `-radius_load..=radius_load`, insert
`(px+dx, py+dy)` into the desired set, diff against the currently-loaded
set, closest-first sort for `to_load`. Being a pure function decoupled
from I/O is what makes it unit-testable without a live `World`.

## 4. Streaming Phase 1: async pre-parse worker

A real background thread + `mpsc` pipeline in `byroredux/src/streaming.rs`.
`WorldStreamingState::new` (`streaming.rs:244`) spawns a worker thread
(`streaming.rs:256`) running `cell_pre_parse_worker` (`streaming.rs:421`),
which pulls `LoadCellRequest`s off an `mpsc::Receiver` and does the
NIF-parse/BSA-extract work off the main thread via `pre_parse_cell`
(`streaming.rs:539`), sending `LoadCellPayload` back on a second channel.

Trigger: `App::step_streaming` (`byroredux/src/app_step.rs:21`) runs once
per tick, converts the active camera's position to a grid coordinate via
`world_pos_to_grid` (`streaming.rs:752`), and only acts when that grid
cell changed since last tick. It calls `compute_streaming_deltas`,
dispatches new load requests, and drains ready payloads via
`payload_rx.try_recv()` into `consume_streaming_payload`
(`byroredux/src/streaming_helpers.rs`) — capped at
`MAX_CELLS_SPAWNED_PER_FRAME = 2` (`app_step.rs:19`) so a large batch of
simultaneously-ready cells can't spike one frame's cost.

(`streaming.rs`'s own module-doc comment still describes this as a
future "Phase 1b (next commit)" — that comment is stale; the worker
below it is fully implemented.)

## 5. Streaming Phase 2: door teleport

The interior↔exterior (and interior↔interior) cell swap is triggered by
the `door.teleport <entity_id>` console command
(`byroredux/src/commands/scene.rs:327`), **not** `script.activate` —
that command drives an unrelated `ActivateEvent` path
(`crates/scripting/src/events.rs`) consumed only by hand-translated demo
scripts, with no cell-transition side effect.

`DoorTeleport` (`byroredux/src/components.rs:60-73`, `SparseSetStorage`)
holds a destination FormID plus Z-up position/rotation, stamped onto
placement-root entities at spawn time
(`byroredux/src/cell_loader/spawn.rs:281-290`) from each door REFR's
XTEL data. `door.teleport` resolves the destination FormID to its parent
cell, builds a `TransitionDestination::{Interior,Exterior}`, and queues
it in `PendingCellTransitionSlot`.

Next tick, `App::step_cell_transition` (`byroredux/src/app_step.rs:255`)
takes the pending transition and dispatches:

- **Interior destination**: `cell_loader::load_interior_cell`
  (`byroredux/src/cell_loader/transition.rs:237`) despawns the current
  interior (`unload_current_interior` → `unload_cell`), then calls the
  **same** `load_cell_with_masters` [Pipeline Overview](pipeline-overview.md)
  traces — not a separate transition-only variant.
- **Exterior destination**: `app_step.rs:322-394` tears down any interior
  and drains the existing `WorldStreamingState`, then calls the same
  `build_exterior_world_context` + `WorldStreamingState::new` +
  `stream_initial_radius` used at boot, with
  `DEFAULT_TRANSITION_RADIUS = 5` (`app_step.rs:270`).

Despawn, in both cases, walks `CellRoot` (`crates/core/src/ecs/components/cell_root.rs:20`)
— every cell-owned entity carries one, pointing at that cell's root
entity. `unload_cell` (`byroredux/src/cell_loader/unload.rs:32`) looks up
victims via the inverted `CellRootIndex` (`byroredux/src/components.rs:968`,
added to avoid an O(total entities) scan) and despawns them, releasing
mesh/BLAS/texture refcounts as it goes.

## 6. What's actually still open

Despite README's "multi-cell grid pending" wording, ROADMAP's M40 row
(closed 2026-05-24) confirms multi-cell streaming, BLAS LRU
eviction/reload as cells stream out, and hysteresis (`radius_unload` >
`radius_load`, preventing boundary-crossing thrash) are all live. The
genuinely open items, per that row:

- No smoke test yet against a real multi-cell exterior workspace (FNV
  WastelandNV, Skyrim's Whiterun plains, FO4 Sanctuary Hills) to bench
  cell-crossing latency end-to-end.
- The per-cell parse stutter (~50-100 ms on FNV, pre-async-worker
  measurement) is mitigated by the pre-parse worker but not eliminated
  — `MAX_CELLS_SPAWNED_PER_FRAME` exists specifically because draining
  every ready payload in one frame still spikes frame time when several
  cells finish pre-parsing at once.

See [ROADMAP.md's M40 row](../../ROADMAP.md) for the full closure
history and commit references.
