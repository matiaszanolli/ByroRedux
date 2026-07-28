# Exterior Grid Streaming: Loading, Following the Player, and Door Swaps

Companion to [Pipeline Overview](pipeline-overview.md), which traces a
single interior cell load end-to-end and explicitly skips this flow. This
doc covers what happens outside an interior box: resolving a worldspace
grid cell to terrain + REFRs, the background pre-parse worker that keeps
streaming off the main thread, and the two ways a loaded scene changes
wholesale — walking across a cell boundary, and a door teleport.

> **Currency note.** Verified against the tree as of 2026-07-27, source
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
`WorldStreamingState::new(...)`, and calls `stream_initial_radius` to
dispatch the starting grid through the worker.

Interactive startup is **foreground-first**: it waits for the center cell as
one coherent transaction, then returns to the render loop while peripheral
cells continue through the normal measured apply budget. This gives exterior
entry the same minimum readiness contract as an interior load without blocking
on the whole 11×11 default radius. `--bench-frames` selects the `FullRadius`
bootstrap mode instead, preserving deterministic measurements with no initial
population mixed into the sample.

## 2. WRLD/LAND → terrain + REFRs

WRLD parsing is `parse_wrld_group`/`parse_wrld_children`
(`crates/plugin/src/esm/cell/wrld.rs:15,186`). LAND heightmap/splat data
is `parse_land_record` (`crates/plugin/src/esm/cell/walkers.rs:1091`),
decoding VHGT/VNML/VCLR into
`EsmIndex.cells.exterior_cells[worldspace][(gx,gy)].landscape`.

Consumption converges in `cell_loader::ExteriorCellApplyJob`
(`byroredux/src/cell_loader/exterior.rs`): it looks up `(gx,gy)`, calls
`terrain::spawn_terrain_mesh` (`byroredux/src/cell_loader/terrain.rs:307`)
for heightmap+splat geometry, `water::spawn_water_plane` for XCLW/default
water, then the **same** budget-aware reference loader
[Pipeline Overview](pipeline-overview.md) traces for interior placed
REFRs (plus FO4 precombine absorption). `load_one_exterior_cell` drives that
job with an unlimited budget for synchronous bootstrap/bulk callers; live
streaming retains the continuation between frames. Exterior and interior cells
therefore converge on identical REFR-spawn machinery — only the driver and the
terrain input differ.

See [ESM Records](esm-records.md) for the WRLD/CELL/REFR record layout
and [EXAL — Exterior Abstraction Layer](exal.md) for how the resulting
terrain/sky/sun/weather/water state is translated into a canonical,
game-agnostic representation for rendering.

## 3. Radius → grid cells

Chebyshev (square) neighborhoods, not circular, come from the single
`compute_streaming_deltas` pure function in `byroredux/src/streaming.rs`:
for `dx, dy` in `-radius_load..=radius_load`, insert `(px+dx, py+dy)` into
the desired set, diff against the currently-loaded set, and closest-first
sort `to_load`. Both bootstrap and steady-state dispatch that result through
`WorldStreamingState::queue_loads`, which owns generation allocation,
pending bookkeeping, duplicate suppression, request construction, and
closed-channel rollback.

## 4. Streaming Phase 1: async pre-parse worker

A real background thread + `mpsc` pipeline in `byroredux/src/streaming.rs`.
`WorldStreamingState::new` spawns a worker thread running
`cell_pre_parse_worker`, which pulls `LoadCellRequest`s off an
`mpsc::Receiver`, does the NIF-parse/BSA-extract work off the main thread
via `pre_parse_cell`, and sends `LoadCellPayload` back on a second channel.

Trigger: `App::step_streaming` (`byroredux/src/app_step.rs:21`) runs once
per tick, converts the active camera's position to a grid coordinate via
`world_pos_to_grid` (`streaming.rs:752`). Cell diff/dispatch only runs when
that grid cell changed, but the step continues on stationary frames while a
deferred LOD reconcile or cell apply is pending.

Steady-state apply uses one `StreamingCellApplyJob` and a cooperative **4 ms
deadline** per frame (`STREAMING_APPLY_BUDGET`). The deadline spans:

1. main-thread completion of worker-parsed NIFs, one NIF per atomic unit;
2. terrain/water/precombine setup as one guaranteed-progress unit;
3. placed-reference spawning, one outer REFR per atomic unit.

An already-started unit always finishes, so a single complex NIF/REFR can exceed
4 ms once but cannot deadlock the queue. Dense cells otherwise yield and resume
without being marked `loaded`. Every yielded entity range is already stamped
under an early `CellRoot`, so removing the matching pending generation on a
boundary crossing immediately cancels it through the normal `unload_cell` path.
Queued texture uploads are flushed per yielded REFR slice instead of accumulating
into one final cell-sized fence wait.

Foreground/full-radius bootstrap still calls `consume_streaming_payload`
synchronously, but both drivers share the same import helper, exterior apply
job, reference pipeline, cache semantics, loaded-map insertion, temporal-history
invalidation, and stale-generation rules.

The same step advances terrain, Skyrim+/FO4 `.bto`, and Oblivion placement LOD
through `streaming_helpers::reconcile_lod_rings`. Full-detail work has priority:
only a frame that spent no NIF/setup/REFR unit receives new LOD work, capped at
two archive/import/upload attempts per provider. Candidates fill closest-first.
A boundary crossing still runs reclaim-only reconciliation on a full-detail
work frame, so geometry leaving the ring and stale terrain hole masks are
removed immediately. Interactive bootstrap defers this work; deterministic
`FullRadius` benchmark bootstrap passes an unlimited budget and settles all
three providers before returning.

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
  foreground-first `stream_initial_radius` used at interactive boot, with
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

- Real FNV foreground-first startup has been smoke-tested; a repeatable
  boundary-crossing benchmark across FNV, Skyrim, and FO4 is still needed to
  quantify the remaining atomic-unit outliers.
- Main-thread NIF finalization and the high-cardinality REFR walk now share a
  measured frame deadline. Individual NIFs/REFRs remain atomic, as do
  terrain/water/precombine setup and their GPU submissions, so one unusually
  complex unit can still exceed the target.
- Distant terrain/object LOD-ring construction is incremental, but its budget
  is an operation count. One `.lod` placement cell can parse and upload far
  more geometry than one terrain block, so the cap bounds work quantity, not
  wall-clock time.
- The next latency slice is to split the remaining atomic setup/placement work
  by upload bytes/mesh batches and bring LOD providers under the same measured
  deadline. That is the path from bounded progress to consistently hitch-free
  streaming.

See [ROADMAP.md's M40 row](../../ROADMAP.md) for the full closure
history and commit references.
