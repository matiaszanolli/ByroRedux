# Performance Audit — 2026-08-27 (Dimension 7 ONLY)

> **Scope warning — read before citing this file.**
> This is a **single-dimension** run of `/audit-performance`, preset
> `streaming-deep`: **Dimension 7 — World Streaming & Cell Transitions (M40)**
> and nothing else. Dimensions 1–6 and 8–9 (CPU hot paths, draw/instancing,
> GPU memory pressure, SSBO sizing, GPU pipeline, skinning/BLAS, NIF parse,
> telemetry/origin cost) were **not run**. Do not read a clean section here as
> coverage of those areas — the most recent full sweeps are
> `AUDIT_PERFORMANCE_2026-08-24.md` and `AUDIT_PERFORMANCE_2026-08-20.md`.

- **HEAD**: `7f78ad9d` (branch `main`, clean tree)
- **Dedup baseline**: cached 400-issue snapshot (open + closed) at
  `/tmp/audit/issues.json`, plus every prior `docs/audits/AUDIT_PERFORMANCE_*`
  report and the 2026-08-26 FNV / 2026-08-20 FO3 reports that touch this code.
- **Method**: static analysis + read-only `cargo`/`git`. **No engine process was
  launched** (the user may have a live instance; parallel launch is forbidden by
  project policy). Every quantity below is derived from checked-in constants and
  named as such.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 4 |
| **Total** | **5** |

| ID | Sev | Title |
|---|---|---|
| PERF-D7-2026-08-27-01 | MEDIUM | Distant-LOD reconcile re-runs the whole desired-quad descent — including a per-quad **archive presence probe** — on every reconcile frame, though the probe's answer is static for the session |
| PERF-D7-2026-08-27-02 | LOW | `drain_streaming_state` and the shutdown sweep tear the resident set down one cell at a time through `unload_cell`, re-running the global batch finalization N times; the batched `unload_cells` API exists and is used only by the boundary path |
| PERF-D7-2026-08-27-03 | LOW | `NifImportRegistry::touch_keys` allocates a fresh `String` per key it has *already proved present*, and its input list is O(placements) rather than O(unique models) |
| PERF-D7-2026-08-27-04 | LOW | `stamp_cell_root_range` inserts `CellRoot` one entity at a time where `World::insert_batch` exists for exactly this shape — and its own sibling index half already uses the batched form |
| PERF-D7-2026-08-27-05 | LOW | `block_hole_mask`'s `resident_full_cells.contains()` linear scan cannot fire under the invariant its own caller establishes, and costs O(\|loaded\|) per masked cell per reconcile frame |

### Observed-vs-ROADMAP delta

**None available, and that is itself the Dimension-7 finding about measurement.**
The live Bench-of-record (`ROADMAP.md` §"Bench-of-record (LIVE) — stepped-camera
refresh (2026-08-14, HEAD `34074b93`)") is a 5-scene × 5-config × 3-run matrix
whose five scenes are **Prospector, WhiterunBanneredMare, MedTekResearch01, FO4
Dugout Inn, and Cornell** — four interiors and one synthetic control. **No
exterior/streaming scene is in the matrix**, so there is no bench-of-record
number a Dimension-7 regression could be a delta *against*.

The harness already has the pieces: `BenchCameraPath::GridCross` / `GridSoak`
(`byroredux/src/app_step.rs:463-467`) advance a *logical* frame that pauses while
a boundary transaction is in flight, and `StreamingTelemetry::bench_line`
(`byroredux/src/streaming.rs:314-376`) emits per-phase p50/p95/max for
dispatch / unload / worker-queue / worker-parse / apply / LOD-slice, plus the six
`UnloadPhaseTimings` phases. They are simply not wired into
`scripts/fsr-bench-matrix.sh`'s scene list. Until one exterior scene is, every
streaming finding — including the three in this report that are cost-shaped
rather than correctness-shaped — is **unquantifiable except by derivation**.

### Hot-path cost table (derived from checked-in constants — not sampled)

| Phase | Cadence | Cost shape | Source |
|---|---|---|---|
| `step_streaming` steady state (no crossing, nothing pending) | every tick | 2 `Instant::now()`, ~4 `HashMap` gets (`apply_cell_climate_override` + `apply_cell_region_ambient`), one `try_recv` | `app_step.rs:74-87, 196-217` |
| Boundary dispatch | per crossing | `compute_streaming_deltas` O(radius²) + `NifImportRegistry::snapshot_keys` O(≤ `BYRO_NIF_CACHE_MAX` = 2048) String clones | `app_step.rs:117-187`, `nif_import_registry.rs:429-431` |
| Boundary teardown | per crossing | 3 cells (hysteresis ring) through **one** `unload_cells` → one `shrink_storages` + one `shrink_blas_scratch_to_fit` | `app_step.rs:141`, `unload.rs:111-124` |
| Full-detail apply | per tick while pending | cooperative, capped by `STREAMING_APPLY_BUDGET` (`Duration::from_millis(16)`, `app_step.rs:33`) shared with the LOD stage via one `FrameTimeBudget::until` | `app_step.rs:196-241` |
| LOD reconcile | per tick while `lod_reconcile_pending` | budget admits `MAX_LOD_ATTEMPTS_PER_PROVIDER_PER_IDLE_FRAME = 2` real attempts/provider, but the **desired-set descent it runs first is O(ring quads × mesh archives)** — see PERF-D7-2026-08-27-01 | `app_step.rs:37, 225-241`, `terrain_lod.rs:372-392`, `object_lod.rs:150-160` |
| Worldspace drain (door / worldspace swap / save reload) | per transition | N × (`unload_cell_inner` + **full** `finish_unload_batch`), N = whole resident set (49 at `--radius 3`, 121 at the transition default of 5) — see PERF-D7-2026-08-27-02 | `streaming_helpers.rs:420-425` |

---

## Findings

### PERF-D7-2026-08-27-01: the distant-LOD reconcile re-runs the whole desired-quad descent — including a per-quad archive presence probe — on every reconcile frame, though the probe's answer is static for the session

- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:372-392` (terrain ring)
  and `byroredux/src/cell_loader/object_lod.rs:150-160` (baked-object ring);
  descent in `byroredux/src/cell_loader/lod_bands.rs:278-339`; driven from
  `byroredux/src/streaming_helpers.rs:95-121` on every
  `reconcile_lod_rings` call, i.e. every tick while
  `state.lod_reconcile_pending` (`byroredux/src/app_step.rs:225-241`).
- **Status**: NEW
- **Description**: `stream_lod_blocks` and `stream_object_lod_blocks` each begin
  by recomputing their *entire* desired quad set with a fresh top-down
  `select_lod_quads` descent. The descent takes two closures: `resident(...)`
  (a `HashMap::contains_key` — genuinely dynamic, it changes as blocks land) and
  `available(...)` — **"does the game ship a baked asset for this quad?"**. That
  second predicate is a pure function of `(worldspace_key, level, qx, qy)` and
  the opened archive set. Neither input can change for the life of a
  `WorldStreamingState`. It is nonetheless re-evaluated from scratch every
  reconcile frame, and each evaluation is not cheap:

  - terrain, combined-`.btr` games: `btr_archive_path(...)`
    (`terrain_lod_btr.rs:84-87`) does a `worldspace_key.to_ascii_lowercase()`
    **plus** a `format!` — two `String` allocations — and hands the result to
    `TextureProvider::has_mesh` (`asset_provider/texture.rs:73-78`), which
    allocates again in `normalize_mesh_path` and then **once more per archive**
    inside `BsaArchive::contains` / `Ba2Archive::contains`, both of which are
    `self.files.contains_key(&normalize_path(path))`
    (`crates/bsa/src/archive/mod.rs:91-94`, `crates/bsa/src/ba2.rs:349-351`).
  - terrain, legacy games: `translate_terrain_lod_textures`
    (`byroredux/src/env_translate.rs:98-128`) builds **both** the diffuse and the
    normal path with two `format!`s (plus up to four `fmt_oblivion_lod_coord`
    `String`s, `env_translate.rs:79-85`) so the caller can test **one** of them.
  - baked objects: `object_lod_archive_path` (`object_lod.rs:475-491`) has the
    same `to_ascii_lowercase()` + `format!` shape, and — unlike the terrain
    closure, which short-circuits with `level == k ||` — is called at **every**
    band including the finest.

  So the reconcile's fixed per-frame cost is `O(quads visited × archives)`
  string allocations and hash probes, while the work it is allowed to *do* that
  frame is `MAX_LOD_ATTEMPTS_PER_PROVIDER_PER_IDLE_FRAME = 2`. The throttle's own
  overhead scales with the ring; the throttled work does not.
- **Evidence**: the availability closure, verbatim (`terrain_lod.rs:379-391`):
  ```rust
  |level, qx, qy| {
      if combined_lod_supported(game) {
          tex_provider.has_mesh(&super::terrain_lod_btr::btr_archive_path(
              worldspace_key, level, qx, qy,
          ))
      } else {
          translate_terrain_lod_textures(game, worldspace_key, world_form_id, level, qx, qy)
              .is_some_and(|lod| tex_provider.has_texture(&lod.diffuse_path))
      }
  }
  ```
  Nothing between `WorldStreamingState::new` and `drain_streaming_state` mutates
  `worldspace_key`, `record_index.game`, or `tex_provider`'s archive list —
  `tex_provider` is an `Arc<TextureProvider>` cloned into the worker
  (`streaming.rs:596`), never rebuilt in place. `lod_missing_blocks`
  (`streaming.rs:624`) and `ObjectLodBlock`'s empty sentinel memoise the *load
  attempt*, but not this probe: a `false` from `available()` makes the descent
  **subdivide**, so the sentinel is never consulted on this path.
- **Impact**: pure waste on exactly the frames the deferred-LOD budget exists to
  protect. Derived order of magnitude on the Skyrim ladder
  (`coarsest_level` 16, `max_cells ≈ 61` → 81 roots, ~200 nodes visited,
  ~100 reaching `available()`), 2 mesh archives: roughly 500 `String`
  allocations and 200 hashed archive lookups **per provider per frame**, ×2 live
  providers, for the length of the settle window (a handful of frames per
  ordinary crossing, tens of frames on worldspace entry / bootstrap where the
  whole ring is cold). It scales with `--radius` and with the ladder's
  `max_cells`, and it is on the main thread inside the shared
  `STREAMING_APPLY_BUDGET` deadline, so it directly eats the allowance the
  boundary hitch is bounded by. **No quantitative guard exists for this site** —
  see the bench note above.
- **Related**: #3142 (`PERF-D7-01`, OPEN) is the *other* per-frame cost in the
  same `reconcile_lod_rings` call chain; #2371 / EX-11 is the band-ladder work
  that introduced the descent; #3203 / #3100 / #3321 extended it to FO3/FNV, so
  the probe path is live on FNV, FO3, Skyrim and FO4.
- **Suggested Fix**: memoise availability on `WorldStreamingState` as a
  `HashMap<(i32, i32, i32), bool>` (or `FxHashMap` — this is a streaming path
  with an integer-tuple keyspace), filled lazily by the existing closure and
  cleared only in `drain_streaming_state` alongside the LOD rings. That collapses
  every frame after the first to two integer-tuple lookups per node. Two cheap
  independent wins while in there: give `object_lod`'s closure the same
  `level == finest ||` short-circuit terrain already has, and make
  `translate_terrain_lod_textures` build the normal path lazily so the presence
  test stops paying for a string it discards.

---

### PERF-D7-2026-08-27-02: the worldspace drain and the shutdown sweep tear the resident set down one cell at a time through `unload_cell`, re-running the global batch finalization once per cell

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming_helpers.rs:420-425`
  (`drain_streaming_state`) and `byroredux/src/app_events.rs:50-52`
  (`App::shutdown`); the batched alternative is
  `byroredux/src/cell_loader/unload.rs:111-124` (`unload_cells`), already used at
  `byroredux/src/app_step.rs:141`.
- **Status**: NEW
- **Description**: `unload_cell` is `unload_cell_inner` **plus**
  `finish_unload_batch` (`unload.rs:99-102`). `finish_unload_batch`
  (`unload.rs:321-342`) is the *global* pass: `world.shrink_storages()` over every
  registered storage (#2148) and `AccelerationManager::shrink_blas_scratch_to_fit`
  (#495). The 2026-08-05 boundary-teardown change added `unload_cells` precisely
  so the usual three-cell eviction ring pays that pass **once** instead of three
  times, and its own doc says so: *"repeating those global passes per cell only
  multiplies the boundary hitch."*

  The two remaining bulk-teardown call sites never adopted it, and they unload
  far more cells at once than the boundary path ever does — the whole resident
  set (`state.loaded.drain()`), which is 49 cells at `--radius 3` and 121 at
  `exterior_transition_radius`'s `DEFAULT_TRANSITION_RADIUS = 5`
  (`app_step.rs:926`).
- **Evidence**: `streaming_helpers.rs:420-425`
  ```rust
  for ((_gx, _gy), slot) in cells {
      cell_loader::unload_cell(world, ctx, slot.cell_root);
  }
  if let Some(cell_root) = persistent_root {
      cell_loader::unload_cell(world, ctx, cell_root);
  }
  ```
  Reached from three live paths: the Exterior→Interior door transition and the
  Exterior→Exterior worldspace swap (`app_step.rs:744, 840`), the M45.1 live
  save-load reload (`save_io.rs:1125, 1237`), and the debug-UI load ops
  (`debug_load.rs:279, 368`). Per-cell semantics are identical between
  `unload_cell` and `unload_cells` — the only difference is how many times the
  finalization runs — so the substitution is mechanical.
- **Impact**: modest and bounded, and I want to be honest about which half is
  which. `world.shrink_storages()` largely **amortises**: `shrink_sparse_tail`
  (`crates/core/src/ecs/sparse_set.rs:202-214`) is a backwards scan that stops at
  the first live slot, and `Vec::shrink_to_fit` is a no-op once `len ==
  capacity`, so across a drain the total scan/realloc work is roughly one pass'
  worth however many times it is called. The **non-amortised** term is
  `shrink_blas_scratch_to_fit`, which walks `blas_entries: Vec<Option<BlasEntry>>`
  (`acceleration/mod.rs:81`) in full on every call to recompute the scratch peak
  — and that `Vec` is indexed by mesh handle, which is documented as never
  reused (`unload.rs:86-90`), so it grows monotonically across a session. 121
  full walks of a five-figure handle table, plus 121 iterations over the ~150
  registered storages, is single-digit-to-low-tens of milliseconds added to a
  transition that the code's own note already prices at *"a few-hundred-ms BSA
  re-open"* (`app_step.rs:673-675`). Real, mechanical to remove, not a hitch on
  its own.
- **Related**: the 2026-08-26 FNV audit verified "batch finalization runs once,
  not per cell" — but checked only the `unload_cells` path, not these two call
  sites. #3254 (OPEN) is a correctness issue in the same function.
- **Suggested Fix**: collect the drained `cell_root`s (plus `persistent_root`)
  into one `Vec<EntityId>` and call `cell_loader::unload_cells` once; do the same
  in `App::shutdown`. `drain_streaming_state` gains the `UnloadPhaseTimings`
  return for free, which is a prerequisite for ever budgeting the interior
  transition.

---

### PERF-D7-2026-08-27-03: `NifImportRegistry::touch_keys` allocates a fresh `String` per key it has already proved present, and its input list is O(placements) rather than O(unique models)

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/nif_import_registry.rs:437-445`
  (`touch_keys`); producer at
  `byroredux/src/cell_loader/references/synth_child.rs:492`
  (`accum.pending_hits.push(cache_key.clone())`); consumer at
  `byroredux/src/cell_loader/references/complete.rs:121`.
- **Status**: NEW
- **Description**: Two compounding wastes on the same list, both in the
  end-of-cell batched commit that every cell load — interior and streamed
  exterior — funnels through.

  1. `touch_keys` guards with `contains_key(key)` and then, having proved the
     entry exists, calls `insert(key.to_string(), t)`. That allocates a `String`
     and re-hashes the key, to overwrite a `u64` in a slot it already located.
     This is the same shape as #832 (`or_insert(name.to_string())` in the NIF
     per-block counters, which leaked ~150 KB/cell of throwaway short strings on
     Oblivion) — the fix there was the `entry().get_mut()/insert` split.
  2. `pending_hits` is a `Vec<String>` pushed **per placement**, not per unique
     model. A cell with 2 000 static placements over 150 unique meshes stores
     ~1 850 duplicate copies of 150 strings, holds them for the whole (possibly
     multi-frame, resumable) cell apply, and then makes `touch_keys` allocate and
     hash each duplicate again — writing a fresh tick to the same ~150 slots over
     and over, where only the last write survives.
- **Evidence**: `nif_import_registry.rs:438-444`
  ```rust
  for key in keys {
      if self.access_tick.contains_key(key) {
          let t = self.next_tick;
          self.next_tick = self.next_tick.wrapping_add(1);
          self.access_tick.insert(key.to_string(), t);
      }
  }
  ```
  `access_tick` is `HashMap<String, u64>` (`:257`). The producer side is inside
  `spawn_synth_child`, which runs once per synthetic child placement.
- **Impact**: ~2 × (cache-hit placement count) throwaway `String` allocations per
  cell load, on the main thread inside the streaming apply budget. The engine's
  own logged example ("6 new unique meshes parsed, NIF cache hits/misses 156/6
  this cell", quoted at `references/mod.rs:1338-1342`) puts that in the low
  hundreds for a Riverwood-class cell and the low thousands for a dense one — so
  tens to low hundreds of microseconds per cell, three cells per boundary
  crossing. Not a hitch; a clean, guard-shaped win with a named precedent.
- **Related**: #832 (CLOSED — the same anti-pattern, fixed in `crates/nif`);
  #523 / #635 (the batching invariant this code correctly preserves — the fix
  below does not disturb it). The 2026-08-26 FNV audit noted "`touch_keys` only
  bumps ticks for already-present keys", which is semantically right and is
  exactly why the allocation is unnecessary.
- **Suggested Fix**: replace the guard-then-insert with
  `if let Some(slot) = self.access_tick.get_mut(key) { *slot = t; }` — one hash,
  zero allocations. Then make `pending_hits` a `HashSet<String>` (or keep the
  `Vec` and `sort_unstable` + `dedup` before the commit) so the tick loop is
  O(unique models) instead of O(placements).

---

### PERF-D7-2026-08-27-04: `stamp_cell_root_range` inserts `CellRoot` one entity at a time where `World::insert_batch` exists for exactly this shape

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/load.rs:209-239`
- **Status**: NEW
- **Description**: The function stamps one component type over a contiguous
  `first..last` entity range — the textbook batch shape. It uses per-entity
  `world.insert(eid, CellRoot(cell_root))`, so every entity pays the full
  `World::insert` preamble: a `TypeId` lookup in `self.type_names`, a second in
  `self.storages` (both `std` SipHash maps), a `RwLock::get_mut`, and an
  `as_any_mut().downcast_mut()` — around six times the cost of the O(1)
  `SparseSetStorage::insert` it wraps.

  `World::insert_batch` (`crates/core/src/ecs/world.rs:238-263`) exists and
  documents itself as being for precisely this ("amortizes the per-call HashMap
  lookup + `downcast_mut` across the batch… prefer this when a loader / import
  path has a natural 'collect all Transforms then all GlobalTransforms' shape").
  The #512 note attached to it correctly says the cell loader's *scatter-shot*
  per-entity multi-type pattern does not benefit — but this site is not that
  pattern, it is the batch pattern, and the function's own second half already
  reaches for the batched form on the index side (`entry.reserve(span)` +
  `entry.extend(first..last)`, `:229-231`, landed as #885).
- **Evidence**: `load.rs:215-222`
  ```rust
  for eid in first..last {
      world.insert(eid, CellRoot(cell_root));
  }
  ```
  immediately above `entry.extend(first..last)` for the `CellRootIndex` half.
- **Impact**: called once per phase of every cell apply
  (`exterior.rs:1604, 1652, 1663, 1699` plus the interior loader), over the whole
  spawned range, so the total is O(entities in the cell) with the ~30 ns/entity
  preamble on top of the ~5 ns insert. Order of ~70 µs per 2 000-entity cell —
  small, but it is a strictly-dominated call in the boundary path and the
  replacement is one line.
- **Related**: #885 (the sibling index half of this same function, already
  batched); #512 (the migration note that scopes when `insert_batch` helps).
- **Suggested Fix**: `world.insert_batch((first..last).map(|eid| (eid, CellRoot(cell_root))));`

---

### PERF-D7-2026-08-27-05: `block_hole_mask`'s `resident_full_cells` linear scan cannot fire under the invariant its own caller establishes

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:201-217`; the input is
  built at `byroredux/src/streaming_helpers.rs:73` and passed with
  `max_full_cell_radius: state.radius_unload` at `:80-81`.
- **Status**: NEW
- **Description**: The hole predicate is
  ```rust
  cell_is_full_detail(gx, gy, player_grid, max_full_cell_radius)
      || resident_full_cells.contains(&(gx, gy))
      || cells_map.get(&(gx, gy)).and_then(|cell| cell.landscape.as_ref()).is_none()
  ```
  `cell_is_full_detail` is `chebyshev((gx, gy), player_grid) <= max_full_cell_radius`
  (`:118-125`), and `max_full_cell_radius` **is** `state.radius_unload`.
  `resident_full_cells` is `state.loaded.keys()` — and
  `compute_streaming_deltas` (`streaming.rs:1423-1431`) evicts every loaded cell
  whose Chebyshev distance exceeds `radius_unload` on the same tick the player
  grid changes, before any reconcile runs. So every element of
  `resident_full_cells` satisfies arm 1, and arm 2 can never be the arm that
  returns `true`.

  Because `||` short-circuits, the scan only *runs* for cells arm 1 rejected —
  i.e. exactly the cells for which it is guaranteed to fail — and it runs to
  completion over the full `Vec` every time.
- **Evidence**: the invariant chain is three links, all in this audit's own
  scope: `streaming_helpers.rs:73` (`resident_full_cells = state.loaded.keys()`),
  `:80` (`max_full_cell_radius = state.radius_unload`), and
  `streaming.rs:1423-1431` (`to_unload` = every loaded coord with
  `d > radius_unload`, applied at `app_step.rs:126-148` before the reconcile at
  `:234`). Bootstrap and door-transition both start from an empty `loaded`.
- **Impact**: `block_hole_mask` is called for every desired finest-band quad in
  the invalidation pass, which runs on **every** reconcile frame regardless of
  budget (`terrain_lod.rs:415-431`). At 16 cells per 4×4 block and `|loaded|`
  = 121 at the transition-default radius, that is on the order of 10⁵ tuple
  comparisons per reconcile frame that provably cannot change the result —
  roughly 100–200 µs on top of PERF-D7-2026-08-27-01's cost, on the same frames.
- **Related**: #1871 / LC0703-02 (the fix that moved this gate from
  `radius_load` to `radius_unload` — which is what made arm 2 redundant);
  PERF-D7-2026-08-27-01 (same per-reconcile-frame budget).
- **Suggested Fix**: either drop arm 2 (its `radius_unload` gate subsumes it, and
  the pinned `cell_is_full_detail_covers_hysteresis_band_when_gated_on_radius_unload`
  test at `terrain_lod.rs:1015-1042` is the guard that keeps it subsumed), or —
  if it is wanted as defence-in-depth against a future caller that passes a
  tighter radius — pass a `HashSet`/`FxHashSet` instead of a `&[(i32, i32)]` so
  the probe is O(1). Whichever is chosen, say so in the doc comment; today it
  reads as a live check.

---

## Regression guards verified INTACT (not re-proposed)

Every guard the skill's Dimension 7 names, checked against HEAD:

- **#877 two-phase `pre_parse_cell`** — Phase 1 serial BSA extract
  (`streaming.rs:1326-1332`), Phase 2 parallel parse
  (`streaming.rs:1350`). Phases are not collapsed.
- **#1262 small-batch serial fast path** — `PRE_PARSE_RAYON_MIN = 8`
  (`streaming.rs:1167`), branch at `streaming.rs:1179-1184`.
- **#3089 dedicated parse pool** — `build_stream_parse_pool` is called **once**
  per worker thread (`streaming.rs:1052`), not per request, and the fan-out runs
  in `stream_pool.install(..)` (`streaming.rs:1186`) rather than rayon's global
  pool. The #3211 observable (`parallel_parse_threads`) is still emitted.
- **`STREAMING_APPLY_BUDGET`** — `Duration::from_millis(16)` (`app_step.rs:33`,
  with its own rationale block at `:27-32`); seeds one
  `FrameTimeBudget::until(streaming_deadline)` at `:196-197` that is shared with
  the LOD stage at `:240`. The skill text no longer transcribes the value, so the
  #3143 doc-rot that made this look like a regression cannot recur.
- **Batched exterior teardown (2026-08-05)** — the boundary ring goes through
  `cell_loader::unload_cells` (`app_step.rs:141`), which per-cell calls
  `unload_cell_inner` and runs `finish_unload_batch` **once** after the last
  victim (`unload.rs:111-124`). `World::despawn_batch` still sorts + dedups once
  and hands a sorted slice to `remove_entities_erased`
  (`crates/core/src/ecs/world.rs:170-188`); `PackedStorage`'s override is still
  the single sorted merge pass, not a per-entity `Vec::remove`
  (`crates/core/src/ecs/packed.rs:256-284`); `SparseSetStorage`'s #2397
  empty-storage O(1) skip is still there (`sparse_set.rs:172-186`).
- **`UnloadPhaseTimings`** — six plain CPU `Instant`/`elapsed` pairs
  (`unload.rs:57-80, 132-318`), accumulated into `StreamingTelemetry` only while
  a boundary sample is active (`streaming.rs:243-255`), so the aggregate stays
  bounded. Still wired only on the `app_step.rs` exterior batch path, as the
  skill states.
- **#862 cache-snapshot-per-crossing** — `snapshot_keys()` taken once per
  crossing and shared by the whole request batch (`app_step.rs:178-182`).
- **#2113 in-flight cancellation** — `stale_pending_coords` still drops pending
  requests that left the unload radius (`app_step.rs:161-165`,
  `streaming.rs:1448-1461`).
- **#3038 key convergence** — both the sync REFR loader
  (`references/synth_child.rs:475`) and the streaming worker
  (`streaming.rs:1283`) route through the single `canonical_model_path_key`.
- **#856 / #1167 shutdown** — `request_tx` is dropped before the join,
  `join_with_timeout` polls `is_finished` with no watcher thread, and `Drop`
  short-circuits after an explicit `shutdown` (`streaming.rs:907-1006`).
- **CDB parsed once** — `MaterialProvider::sf_cdb_count` is presence-only and the
  byte cache is the module-scope `sf_cdb_cache` (`asset_provider/material.rs:150-206`),
  so it survives every provider rebuild. Nothing parses a CDB per cell or per
  material.
- **CSG archive cached across cells** — `open_csgs_for` routes through
  `MaterialProvider::geometry_csg` (`precombined.rs:498-500`,
  `asset_provider/material.rs:553-564`), which memoises the opened
  `Arc<CsgArchive>` by plugin path, so the "240 MB-class blob" is inflated once
  per session, not once per cell.

## Known-open, deliberately NOT re-reported

- **#3142** (`PERF-D7-01`, OPEN) — `resident_vwd_refr_cells` still takes a fresh
  `World::get::<GlobalTransform>` storage read-lock per VWD entity and still
  accumulates into a `std::collections::HashSet`
  (`streaming_helpers.rs:313-326`). Unchanged since 2026-08-20. Note that
  PERF-D7-2026-08-27-01 and -05 land on the same per-reconcile-frame budget, so
  fixing all three together is worth more than any one of them.
- **Interior cell load has no per-frame spawn budget** — `load_references` still
  hard-codes `FrameTimeBudget::unlimited()`
  (`cell_loader/references/mod.rs:218`); only the exterior path uses
  `load_references_budgeted`. Stated as open-for-interiors in the skill's own
  Dimension 7 and recorded by the 08-16 and 08-20 sweeps; still no GitHub issue
  (checked against the 400-issue all-state snapshot). Recorded so a future sweep
  knows it was checked, not missed.
- **#1793** (missing rigid BLAS has no recovery; a synchronous multi-cell burst
  can false-evict a not-yet-drawn entry) and **#1797** (the shared
  `blas_scratch_buffer` serialises N dirty skinned entities) — documented-not-fixed
  and unreachable on the 12 GB dev card.
- **#3254** (OPEN) — cinematic unload-retention permanently orphans entities out
  of cell ownership. A correctness issue in `unload_cell_inner`, owned by
  `/audit-ecs`, not re-litigated here.

## Candidates investigated and dropped (so a later sweep does not re-derive them)

- **`apply_cell_climate_override` / `apply_cell_region_ambient` running outside
  the `grid_changed` guard** (`app_step.rs:74-87`) — deliberate (a session
  starting inside an override cell must apply it on frame 0) and genuinely cheap:
  two-to-four `HashMap` gets plus a compare. `RegionAmbientRes::resolve`
  (`components.rs:523-551`) is a single `select_active_region_sound` pass, and the
  write plus music dispatch are change-guarded.
- **`find_overlaps`' `O(n²)`** (`lod_coverage.rs:53-64`) — `n` is the quadtree
  *partition* size, structurally bounded to ~90 quads over the ±64-cell ring, so
  ~4 k rectangle tests. `find_full_detail_overlaps` is ~90 × 121. Both are
  microseconds; the doc's "real runs keep in the tens" is close enough.
- **`update_terrain_seam_stats`** (`streaming_helpers.rs:228-302`) — `check_seam`
  (`terrain_seam.rs:124-161`) is a 33-vertex edge compare whose `Vec::new()`
  does not allocate unless a mismatch is found; ~2 × `|loaded|` pairs per
  reconcile frame. Cheap, and its own doc prices it correctly.
- **`spawn_navmesh_tiles`' `navm.clone()`** (`components.rs:1722-1729`) — a deep
  clone of the parsed `NavmRecord` per cell load, including FO4's retained
  `packed_geometry` blob. Real duplication of `Arc`-resident data, but the
  per-cell `navmeshes` list is one-to-a-few records, so it is not a streaming
  cost. If it ever needs fixing the shape is `Arc<NavmRecord>`.
- **`parse_extracted_nifs`' per-item thread-name `String`**
  (`streaming.rs:1186-1199`) and **`record_worker(payload.timings.clone())`**
  (`streaming_helpers.rs:570`, which clones a `Vec<String>` to read two
  `Duration`s) — both real, both trivially small, both off the frame path or
  once-per-payload. Not worth an issue on their own; fold into any future edit of
  those functions.
- **`StreamingLatencySummary` being `Copy` with a 512 B inline ring** — the
  by-value `average_ms(self)` / `max_ms(self)` copies are confined to
  `bench_line`, which runs at bench print, not per frame. No per-frame
  `StreamingTelemetry` clone exists.
- **`PersistentRefIndex` / `CellRootRefIndex` full-world rebuild**
  (`form_id_root_index.rs:36-58`, which iterates every `FormIdComponent` entity
  and takes a per-entity `world.get::<CellRoot>` lock — the #3142 shape again) —
  **both resolvers are `#[allow(dead_code)]` with no production caller**
  (`boot.rs:496-497` only inserts the empty resources). Zero runtime cost today.
  Flagged here only so that whoever wires the EX-14/15 / EX-16 consumers hoists
  the `CellRoot` query out of the loop *before* it goes live, rather than filing
  a third instance of #3142 afterwards.
- **`label = format!("exterior({},{})", …)`** (`exterior.rs:1681`) — one
  `String` per resumable-apply frame. Below the noise floor.
- **`geometry_batch_in_progress()` including `lod_reconcile_pending`**
  (`streaming.rs:725-730`) — checked for a rebuild-starvation path where a
  continuously-walking player never lets the LOD ring settle and so never lets
  the global geometry SSBO rebuild start (`app_frame.rs:193-207`). Could not
  substantiate it: a cell is 4096 units wide, so crossings are seconds apart
  while the ring settles in a handful of frames at 2 attempts/provider, and #3298
  already keeps an *in-flight* rebuild advancing regardless of the gate. The
  per-frame masking pass is O(draws) with an O(1) `is_geometry_resident`
  (`crates/renderer/src/mesh.rs:1487-1500`) — renderer-side and by design.

## Prioritized fix order

1. **PERF-D7-2026-08-27-01** — memoise LOD quad availability. Biggest per-frame
   win in this dimension, self-contained, and it makes the reconcile's cost
   independent of ring size. Bundle **-05** (drop or `HashSet` the
   `resident_full_cells` probe) and **#3142** (hoist the `GlobalTransform` query,
   `FxHashSet` the accumulator) into the same change — all three are
   per-reconcile-frame costs in one call chain.
2. **PERF-D7-2026-08-27-03** — one-line `get_mut` fix plus a `dedup` on
   `pending_hits`. Zero-risk, and it clears a live instance of an anti-pattern
   the project already closed once (#832).
3. **PERF-D7-2026-08-27-04** — one-line `insert_batch` swap.
4. **PERF-D7-2026-08-27-02** — route the two bulk-teardown call sites through
   `unload_cells`. Mechanical, and it hands `drain_streaming_state` the
   `UnloadPhaseTimings` it currently discards.
5. **Measurement, not code**: add one exterior scene (an FNV `WastelandNV` or
   Skyrim `Tamriel` grid-cross) to `scripts/fsr-bench-matrix.sh` so
   `StreamingTelemetry::bench_line`'s per-phase p50/p95/max enters the
   bench-of-record. Until that exists, no Dimension-7 finding — including the
   four above — can be reported as a delta rather than a derivation.

---

*Generated by `/audit-performance --focus 7` (preset `streaming-deep`).
Publish with `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`.*
