**HEAD**: `41f9c2c6c` (area code identical to `807c58890`; the two later commits touch no `byroredux/src`) · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-12.md` (HEAD `5d31fd637`; the 10-17 report was volumetrics-only and is not a baseline for this area) · **Audited**: Dim 1 (`npc_spawn/ai_package.rs` `ambient_ai_package_system` only), Dim 7 (streaming, cell_loader, npc_spawn, `app_step.rs`, `streaming_helpers.rs`); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: Dims 2, 3, 4, 5, 6, 8 are out of area (their `Paths:` do not touch `streaming/`, `npc_spawn/` or `cell_loader/`). Inside the area, `stream_snapshot.rs`, `streaming/telemetry.rs`, `terrain*.rs`, `precombined.rs` and `water.rs` have no functional change since the baseline and were skimmed.

**THIS REPORT IS AREA-SCOPED (streaming-deep: `byroredux/src/streaming/`, `npc_spawn/`, `cell_loader/` plus the `app_step.rs` / `streaming_helpers.rs` callers), NOT A FULL-CODEBASE PERFORMANCE AUDIT.** The next full run must not treat it as a baseline for the renderer, skinning, SSBO or telemetry dimensions.

# Performance Audit — 2026-10-19 (streaming-deep)

**Command**: `/audit-performance` as one leg of `--area streaming-deep`, run solo, no sub-agents. Scratch: `/tmp/audit/streaming-deep/performance/`.

**Mode**: static analysis, the area's unit tests, and four throwaway micro-benchmarks. No engine, GPU process or bench was launched (suite rule 4). **No GPU cost is claimed. Every CPU number below is labelled as measured (archive extraction and NIF import ran against the real FNV and Skyrim SE archives with the repo's own `byroredux-bsa` / `byroredux-nif` crates), quoted from a dated earlier audit, or derived from a scratch replica of the code.** Private cargo target dir: `/tmp/audit/streaming-deep/performance/target` (scratch crates only; the area tests ran in the repo `target/`).

## Executive Summary

| Severity | NEW | Existing / tracked |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 1 (#5715, still present) |
| LOW | 4 | 1 (#5999, related) |

- **The one finding with real weight is a main-thread stall on every door transition**: both destinations re-parse the whole ESM load order from disk although the same parsed index is already resident (PERF-SD-01). Quoted cost: 1.54 s on FNV, ~2.9 s FO4 + DLC, ~7.7 s Starfield (all earlier measurements, see the finding).
- **Every landed guard in the area still holds** (list in "Guard verification"). 854 area tests pass.
- The newest code (#6054 starved-LOD verdict, #5705 water rebuild gate, #4913 tree LOD, #5408 head parts, #5489 pack-in instancing, M47.2 created references / object graphs) adds no per-frame cost in the steady state, except the small per-frame items filed as LOW.
- Candidates I measured and **disproved** are listed under "Checked and not filed"; they would otherwise be rediscovered.
- No observed-vs-ROADMAP delta can be reported: no bench was run, and ROADMAP's bench-of-record is still `R6a-stale-25` (predates every frame-path change since #5154).

## Hot Path Analysis

Out of scope for GPU pass cost (no `gpu_timers` read; the pass inventory table is untouched by this area). Per-frame CPU in the area, derived from code:

| Per-frame site | Steady-state cost | Notes |
|---|---|---|
| `step_streaming` (`app_step.rs:56`) | one camera read, `apply_cell_climate_override` / `apply_forced_weather` / `apply_cell_region_ambient` each an `Option` compare, `refresh_lod_water` two comparisons | `populate_object_graphs` is the exception, PERF-SD-04 |
| LOD reconcile | early `return` unless `lod_reconcile_pending` / `loaded_residency_changed` (`lod_reconcile_budget_for_frame`) | tree ring adds ~2 small clones + 2 sorts of ≤386 quads per call, only while pending |
| `step_cell_transition` (`app_step.rs:1173`) | five new M47.2 steps run before the transition check; four are one resource read | `step_created_references` is not, PERF-SD-03 |
| `ambient_ai_package_system` | `tick_script_packages`, a `BTreeMap::range` over the ObScript ledger, three empty `Vec`s, one `Option<u16>` compare per NPC | per-minute burst below |

`ambient_ai_package_system` per-minute burst (derived): at the default time scale one game minute is ~2 real seconds, and every NPC becomes due on the same frame (`last_evaluated_game_minute != Some(minute)`). Each due actor does ~10 sparse lookups, 3–4 small `Vec`s and the PACK condition scan; I estimate 5–10 µs per actor, so ~1 ms per 2 s at 100 NPCs. Not measured and not filed. Staggering by `actor % 60` would flatten it if a bench ever shows it.

## Findings

### Eroded guards

None. See "Guard verification".

### New issues

### PERF-SD-2026-10-19-01: Every door transition re-parses the whole ESM load order on the main thread, though the parsed index is already resident
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells (Dim 7)
- **Location**:
  - `byroredux/src/cell_loader/load.rs:550` (`InteriorCellApplyJob::begin` → `parse_record_indexes_in_load_order`), reached from `cell_loader/transition.rs:543` (`InteriorCellApply::begin`) and `app_step.rs:1288`.
  - `byroredux/src/cell_loader/exterior.rs:1795` (`build_exterior_world_context` → the same parse), reached from `app_step.rs:1333` for any door whose destination is exterior.
  - No cache: `cell_loader/load_order.rs:568-727` does `std::fs::read` + a fresh rayon walk + `merge_from` on every call.
- **Status**: NEW. Searched open and closed issues for `door transition parse`, `interior transition ESM parse`, `LoadedCellIndex reuse`, `esm_parse`: nothing. Related, not the same: #3671 (budgeted interior REFR phase), #5288 (archives opened twice per door), #3415.
- **Description**: A door transition is dispatched by `step_cell_transition`. Interior destination: `unload_current_interior`, then `InteriorCellApply::begin`, whose first act is a full-load-order parse (`phases.esm_parse`). Exterior destination: `build_exterior_world_context` parses it again. The destination's `masters` / `esm_path` come from `LoadedPluginSet` (`transition.rs:352`-ish, `queue_transition_to_reference`), so within a session they are always the plugin set that built the `Arc<EsmIndex>` already installed as `LoadedCellIndex` (and held in the streaming `wctx.record_index` for exterior sessions). The code already knows a parse is expensive: `app_step.rs:1380-1384` avoids "a second ESM parse" for the preserved persistent root, but the one remaining parse per door is not avoided. The skill's Dim 7 text describes the interior door path as budgeted; the `begin` half is not (the `FrameTimeBudget` only governs `advance`).
- **Evidence**:
  ```rust
  // load.rs:549-551, InteriorCellApplyJob::begin, run once per door
  let phase_started = Instant::now();
  let (index, load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;
  phases.esm_parse = phase_started.elapsed();
  ```
  Quoted costs (none re-measured here): `AUDIT_FNV_2026-10-18.md:60` — FNV interior load `esm_parse 1.54 s, references 0.41 s, finalization 0.07 s, total 2.01 s`, so the parse is 77 % of the interior load; the skill's Dim 7 text — FO4 + 7 DLC 2.85 s, Starfield 14 plugins 7.72 s (2026-09-28 parallel-walk measurements).
- **Disproof attempted**: (1) a parsed-index cache inside `parse_record_indexes_in_load_order` — none (`fs::read` per call, no `OnceLock` / memo). (2) The loading screen hiding the cost — it hides the frame, not the block: `step_cell_transition` runs the parse inline before returning, so no frame is presented, no input or window events are pumped (a multi-second stall on a Wayland/X11 compositor can trip "not responding" handling; not verified here). (3) Plugin set could differ between sessions — it cannot within one session; the destination comes from `LoadedPluginSet`. (4) The live `EsmIndex` could be mutated after parse in a way a reuse would inherit — `LoadedCellIndex` is an `Arc` of an immutable index; `InteriorCellApplyJob` takes `index: EsmIndex` by value and only moves it into the `Arc` at `finish` (`load.rs:776`), so reuse needs `Arc<EsmIndex>` plumbing, not a semantic change (the one thing I did not verify: whether any step between parse and `finish` mutates `index`; the code read shows none).
- **Impact**: per door, the FNV/FO3/Oblivion reference title blocks ~1.5 s; FO4+DLC ~3 s; Starfield ~8 s. Memory: the previous index stays alive in `LoadedCellIndex` while the new one parses, so the transient peak holds two indexes (`memory-budget.md` "ESM Index" prices one). Confidence: high on structure, medium on magnitude per game (quoted, not re-run; a door-specific `Cell load phases:` log line on FNV would pin it).
- **Related**: #3671, #5288, #5715 (also a main-thread hitch in the same step), `save_io` reloads (a new session legitimately re-parses).
- **Suggested Fix**: when the destination's `(masters, esm_path)` equals the resident `LoadedPluginSet`, pass the existing `Arc<EsmIndex>` + `LoadOrder` into `InteriorCellApplyJob::begin` / `build_exterior_world_context` instead of parsing; fall back to the parse when the sets differ. Pin it with a test that a second interior transition performs zero `parse_record_indexes_in_load_order` calls.

### PERF-SD-2026-10-19-02: NPC spawn extracts every shared NIF's bytes before asking the import cache whether it already has the scene
- **Severity**: LOW (measured 1.31 ms per FNV humanoid; ~50 ms per 40-NPC cell; negligible on Skyrim SE)
- **Dimension**: Streaming & Cells (NPC spawn)
- **Location**:
  - `byroredux/src/npc_spawn/resumable/mod.rs:455` (`spawn_armor_unit`) and `:516` (`spawn_skeleton_unit`).
  - `byroredux/src/npc_spawn/resumable/runtime.rs:579` (body parts), `:973` (head), `:1133` (`spawn_shared_skeleton_part`: hair, eyes, brows, teeth, tongue, mouth).
  - `byroredux/src/npc_spawn/resumable/prebaked.rs:349` (fallback head parts).
  - The consumer: `byroredux/src/scene/nif_loader.rs:443-474` (`load_nif_bytes_with_skeleton` → `cached_scene`, `:213-235`), which reads `data` only on a `SceneImportCache` miss.
- **Status**: NEW. Not a regression of #880 / #4794 (both about parses and textures, and still guarded: `SharedToneCache` is present in `seam_blend/tone.rs`).
- **Description**: Each spawn unit calls `tex_provider.extract_mesh(path)` (archive read + inflate + a `Vec` copy) and passes the bytes in. `cached_scene` then looks the lower-cased path up in `SceneImportCache` and returns the cached `Arc<ImportedScene>` without touching the bytes. For the shared parts (skeleton, body, hands, feet, head parts, hair, armor) every NPC after the first is a cache hit, so the extraction is discarded. `cached_scene` already accepts `data: Option<&[u8]>` and extracts lazily when `None` (the `peek_or_parse_scene` path), so the mechanism for a lazy extraction exists.
- **Evidence**: measured with a scratch crate on the repo's `byroredux-bsa` (release, warm page cache, 20 repetitions per file):

  | Archive | Parts | Decoded | Time |
  |---|---|---:|---:|
  | FNV `Fallout - Meshes.bsa` (v104, zlib) | skeleton, upperbody, 2 hands, headhuman + 6 head parts | 799 KB | **1.31 ms per NPC** |
  | Skyrim SE `Skyrim - Meshes0/1.bsa` (v105, LZ4) | skeleton, malebody_0, malehands_0, malefeet_0 | 301 KB | 0.07 ms per NPC |

  Armor and hair add more on FNV (not measured). Skyrim/FO4 per-NPC FaceGeom heads are unique per NPC, so their extract is a genuine miss and is correctly needed.
- **Impact**: wasted main-thread time inside the budgeted apply slices of every NPC cell on the zlib games: ~1.3 ms × NPCs (FNV Megaton-class ~40 NPCs ≈ 50 ms; order 10–25 % of the 5–12 ms per-NPC spawn estimate in `AUDIT_PERFORMANCE_2026-09-23b.md`). It lengthens apply completion, it does not stall a frame (units yield).
- **Disproof attempted**: checked that no extract-side cache exists (`TextureProvider::extract_mesh` is `extract_first` over the archives, no memo — `asset_provider/texture.rs:209`); checked the cache key (lower-cased path) matches what the callers pass, so hits are real; checked the hook path (`apply_spawn_hook` → `Arc::make_mut`) still needs no bytes. The only unknown is the hit rate in a live cell, which is structurally "all but the first NPC of each race/gender" for shared parts.
- **Suggested Fix**: take a lazy extractor (`impl FnOnce() -> Option<Vec<u8>>`) or `Option<&[u8]>` in `load_nif_bytes_with_skeleton` and have the NPC units probe `SceneImportCache` first; keep the eager extract only for the unique FaceGeom head. Pin: a second NPC of the same race performs zero `extract_mesh` calls for skeleton/body/hands.

### PERF-SD-2026-10-19-03: `step_created_references` runs every frame for the rest of the session once any created reference exists, scales with every record ever created, and `locate_refr` is a full-index scan
- **Severity**: LOW
- **Dimension**: CPU Hot Paths / Streaming & Cells
- **Location**:
  - `byroredux/src/app_step.rs:981-1060` (`step_created_references`, called from `step_cell_transition` each frame).
  - `byroredux/src/cell_loader/created.rs:82-141` (`place_created_references`) and `:146-208` (`unspawned_in`).
  - `crates/scripting/src/created_references.rs:128-190` (records are never pruned: `retain_quests` only touches `by_alias`; `scripted` and `Deleted` tombstones stay, and are saved).
  - `crates/plugin/src/esm/cell/index.rs:554` (`locate_refr`).
- **Status**: NEW. Searched `created references`, `CreatedReferences`: nothing open.
- **Description**: The early-out is `created.iter().next().is_none()`. The first record ever created (quest alias or a script `PlaceAtMe`, which five different ObScript/Papyrus call sites feed) makes the step run every frame forever, because tombstones are kept "so the alias refresh does not create it again". Each frame: `place_created_references` iterates all records; in an exterior session the step builds a `Vec` of the loaded cells, clones the worldspace `String` once per loaded cell, and calls `unspawned_in` per cell, which iterates all records again and (for a cell with a live record) builds a `HashSet` from `CreatedReferenceEntities`. In an interior session every spawn event (a wanted, not-live record) rebuilds both archive providers from the command line (`build_texture_provider` + `build_material_provider`, `app_step.rs:1009-1011`) instead of reusing a kept provider as `step_actor_graphs` does with `ActorClipProvider`; archive handles are shared (`open_shared`, ~0.1 ms per #5288), so this is per event, not per frame, unless the record never becomes live. (Forwarded concurrency note, confirmed here.) Separately, a created record whose target cannot be placed (`"its target has no placement yet"`, `created.rs:130`) is retried every frame, and each retry calls `locate_refr`, a miss that walks every `PlacedRef` in the index.
- **Evidence**: scratch replica of the exterior loop (49 loaded cells, records mostly `Deleted` tombstones, release): 10 records 0.8 µs/frame, 100 → 4 µs, 1,000 → 32 µs, 10,000 → 276 µs. `locate_refr` miss scan, scratch replica with a 512-byte stride (`PlacedRef` has 33 fields; its real size was not measured): 63 k REFRs 0.10 ms (L3-warm), 800 k 5.7 ms, 1.55 M 8.7 ms.
- **Impact**: negligible for a normal session (tens of records, a few µs per frame). It becomes a per-frame cost only with unbounded `PlaceAtMe` churn (hundreds of thousands of frames-worth of tombstones in a long session and in the save file), and a stuck record on a large index costs a few ms per frame. Not a bottleneck today.
- **Disproof attempted**: checked that every spawn path stamps `SceneAliasCandidate` (so a successfully spawned record does become live and stops being retried): `spawn_synth_child` and the actor path call `stamp_quest_reference` on all branches, so the interior provider-rebuild loop needs a record whose spawn silently produced no entity; I could not construct one from the code, so that sub-claim is not made. Checked whether a stuck record is reachable: needs a created record whose `at_reference` is neither in the index nor a placed created record; plausible via `PlaceAtMe` on a dropped record, not demonstrated.
- **Suggested Fix**: gate the step on a dirty key (record count / generation + `loaded_residency_changed` / interior root change); prune `Deleted` tombstones whose quest is gone; add a lazily built `form_id → locator` map to `EsmCellIndex` next to the existing `cell_by_form_id` `OnceLock` pattern (`index.rs:323`) and use it in `locate_refr` (also used once per door by `queue_transition_to_reference` and `interior_spawn.rs:143`).

### PERF-SD-2026-10-19-04: `populate_object_graphs` retries a prop whose behaviour graph is unavailable on every streaming frame
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/app_step.rs:93` (called each `step_streaming`), `byroredux/src/asset_provider/object_graph.rs:156-236`.
- **Status**: NEW. Searched `populate_object_graphs`: nothing.
- **Description**: The pass selects entities that carry `BehaviorGraphRef` and lack `ObjectAnimationGraph`. When `load_graph` fails (project/behaviour/skeleton not in the archives, or a decode error) the failure is cached in `ObjectGraphCache` as `None`, but nothing marks the entity, so it stays in `wanting` forever. Each frame such an entity pays a `to_ascii_lowercase`, a `names.join(",")`, a `format!`, a sort, a hash lookup of the cached `None`, then `continue`. Two `Vec`s are also built per frame. The interior path calls the pass once at load, so only exterior streaming loops.
- **Evidence**: code read (`:156-216`); `ObjectAnimationGraph::new` also returns `None` when the graph has no machine (`animation_graph.rs:424-426`), a second way to stay unattached.
- **Impact**: ~0.3–0.5 µs per stuck prop per frame (derived, not measured); the number of such props was not counted.
- **Related**: nothing. **Suggested Fix**: insert a negative marker component (or record the entity in a `HashSet`) when the cached result is `None`, and skip the whole pass while the `BehaviorGraphRef` storage generation is unchanged.

### PERF-SD-2026-10-19-05: `effective_absorbed_refs` rebuilds a SipHash `HashSet` on every apply slice
- **Severity**: LOW (FO4 only)
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/exterior.rs:2245` (`ExteriorCellApplyJob::advance`) and `byroredux/src/cell_loader/load.rs:649` (`InteriorCellApplyJob::advance`); the function is `cell_loader/precombined.rs:121`.
- **Status**: NEW. Searched `effective_absorbed_refs`: only #5755 matched (unrelated).
- **Description**: Both `advance` methods recompute the absorbed-ref set from `cell.absorbed_ref_bakes` and `baked_hashes` on each call, i.e. once per frame slice until the REFR walk finishes, although the inputs are fixed once the precombine phase completes. It is empty and free when no precombine baked (every game except FO4).
- **Evidence**: scratch replica (release): 1,000 bake entries 24 µs, 5,000 → 106 µs, 20,000 → 511 µs per call (≈21 ns per entry). The real `absorbed_ref_bakes` length per FO4 cell was not measured.
- **Impact**: ~0.1 ms (5 k entries) per slice, ≈1 % of the 16 ms apply budget, times the slices of every FO4 cell.
- **Suggested Fix**: compute once when `baked_hashes` is finalised and keep the set in the job (an `FxHashSet` while there).

## Existing (verified still present, not re-filed)

- **#5715 (MEDIUM, open)**: stale-payload salvage still runs every parsed import of each stale payload in a plain `for` before charging the budget (`streaming_helpers.rs:940-949`), and `cancel_streaming_apply_job` salvages the remaining `FinishImports` (`:806-815`). Each entry's main-thread cost grew slightly since the baseline: `finish_partial_import` now also runs `scene_satellites` (see "Checked and not filed" for the measured size).
- **#5999 (open, scripting)**: `.pex` re-extracted/decompiled per scripted REFR — relevant to the spawn path; not re-assessed here.

## Guard verification

Ran `cargo test -p byroredux --bin byroredux -- cell_loader:: streaming npc_spawn:: scene_import_cache nif_import_registry` in the repo `target/`: **854 passed, 0 failed, 27 ignored**. Named guards confirmed passing and present:

- #5705 `lod_water_rebuilds_once_per_settled_crossing`, `lod_water_rebuild_defer_is_bounded_and_spaced`; #5289 `lod_water_rebuild_reuses_the_spawn_time_projection`.
- #5418 `detach_victims_takes_one_parent_guard_for_the_whole_sweep`.
- #5529 `a_climate_override_is_evaluated_once_per_grid_cell`.
- #5522 `negative_registry_entry_is_a_known_failure_not_unseen`.
- #5512 `stale_payload_imports_are_salvaged_into_the_registry`; #6054 `a_capacity_starved_lod_reconcile_is_not_reported_settled`.
- `static_blas_recovery_runs_between_frames_not_in_the_render_driver` (`app_step.rs`).
- #5448: `m42_debug_enabled` reads `BYRO_M42_DEBUG` once through a `OnceLock` (`ai_package.rs:796`); the per-minute gate and the no-`Vec`-before-gate shape in `ambient_ai_package_system` are intact (`:941-960`).
- #3659 / #3089 pre-parse: `parse_nif_pipeline` still admits against `STREAM_PARSE_INPUT_BYTES` using `mesh_declared_size` before the pool task extracts; `PRE_PARSE_RAYON_MIN` = 8 serial fast path; batch memo cleared when the queue empties (`pre_parse.rs:212-231`). `PartialNifImport: Send` compile guard restored (#5442).
- #4794: `SharedToneCache` (world-lifetime tone reuse) present in `npc_spawn/seam_blend/tone.rs`.

## Checked and not filed (quantified or disproved)

- **`scene_satellites` on the main thread** (`cell_loader/partial.rs:125`; `import_kf` for every unique streamed NIF, duplicating the worker's first-sequence conversion). Measured with `byroredux-nif` on 3,613 sampled FNV non-creature NIFs (release): 94 % have no sequence and cost **0.43 µs** per `import_kf`; the 6 % with sequences average 49.8 µs (worst 6.9 ms, one DLC04 effects NIF). Mean ≈ 3 µs per NIF. Below any threshold; the skill's "no further import work on the main thread" rule is technically crossed, the cost is not.
- **`retire_active_packages` / `raise_unload_events` per unload victim** (`unload.rs:437-439`): one SipHash `remove` and one sparse lookup per victim, derived ≈0.05–0.1 ms per cell. Not measured; dominated by the despawn batch.
- **Tree-LOD ring per-reconcile clones** (`tree_lod.rs:390-408`): species list and ≤386-quad list cloned and sorted twice per call, only while a reconcile is pending; ≈50 µs derived.
- **`SceneImportCache` FIFO half-eviction at 300 entries** (`scene_import_cache.rs`): per-NPC unique Skyrim/FO4 FaceGeom heads share the FIFO with the shared skeleton/body entries, so every ~150 new unique NPCs the shared entries (oldest) are evicted and re-parsed. Amortised well under 1 ms per NPC; noted, not filed.
- **`prebaked_head_fallback_paths` computed even when a FaceGeom exists** (#5408): a few µs per NPC.
- **`effective_absorbed_refs`-style per-slice work in the persistent cell job**, `load_references_budgeted` prelude (`load_idle_pool`, `load_walk_clips`, `load_sit_clip`): all path-memoised, a registry hit per call.
- **ESM-parse hitches at boot / save-load**: unbudgeted by design (skill text); only the door-transition case is filed.

## Prioritized Fix Order

1. **PERF-SD-01** reuse the resident `Arc<EsmIndex>` + `LoadOrder` on door transitions (largest single block in the area; also removes the 2× index peak). Needs a design pass for the `Arc` plumbing, so architectural.
2. **PERF-SD-02** probe `SceneImportCache` before extracting in the NPC units (quick win, signature change in one loader).
3. **PERF-SD-05**, **PERF-SD-04** cache the absorbed set in the job; mark graph-less props (quick wins).
4. **PERF-SD-03** dirty-gate `step_created_references`, prune tombstones, give `EsmCellIndex` a REFR locator map (small, but the map also helps door lookups on Starfield-sized indexes).
5. Existing #5715 (budget the stale salvage) stays where its issue puts it.

## Stale skill premises

- Dim 7 describes "the interior door path" as budgeted and lists boot, save-load restore and debug loads as the unbudgeted ones. The door path's `begin` also runs an unbudgeted full ESM parse (PERF-SD-01); add door transitions to the unbudgeted list, or fix them.
- Dim 7 `Paths:` list `byroredux/src/npc_spawn.rs` but not `npc_spawn/resumable/`, `npc_spawn/seam_blend/` or `scene/nif_loader.rs`, where the NPC extraction cost (PERF-SD-02) lives; add them.
- Dim 1's `Paths:` omit `byroredux/src/cell_loader/created.rs` and the `app_step.rs` M47.2 steppers (`step_created_references`, `step_actor_graphs`, `step_kf_idles`), which now run per frame.
- Dim 7's NIF-import line ("any further import work back on the main thread is the regression") should name `scene_satellites` (`nif_import_registry.rs`) as the accepted main-thread residue, with the 0.43 µs / 49.8 µs numbers above.

## Not checked

No engine, GPU or bench run: no `gpu_ms:` / `cpu_ms:` read, no `npc_spawn_wall`, no `worker_stale_salvaged_imports`, no live door transition timing (PERF-SD-01 magnitude is quoted). `PlacedRef` real size and FO4 `absorbed_ref_bakes` lengths were not measured. The plugin crate's `--ignored` tests were not run (OOM rule).
