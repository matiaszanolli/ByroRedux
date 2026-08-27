# PERF-D7-2026-08-27-01: The distant-LOD reconcile re-runs the whole desired-quad descent, including a per-quad archive presence probe whose answer is static for the session

- **Issue**: [#3385](https://github.com/matiaszanolli/ByroRedux/issues/3385)
- **Finding ID**: `PERF-D7-2026-08-27-01`
- **Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `medium,performance,terrain-exterior,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3385 --json state`.

---

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
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `PERF-D7-2026-08-27-01`._
