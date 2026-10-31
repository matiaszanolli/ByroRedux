**HEAD**: `670f81917` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-27.md` (HEAD `fa6be4a6b`, 136 commits since) · **Audited**: Dims 1, 5, 7, 8 (delta read plus guard lanes) · **Unchanged since baseline (skimmed)**: Dims 2, 3, 4, 6 (Dim 4 and Dim 2 only carry the #6358 timer call-site migration, comment edits and the still-open #6485)

# Performance Audit - 2026-10-31

**Command**: `/audit-performance` inside `/audit-suite --preset comprehensive`. Per the suite rules there were no sub-agents and no engine launch. The build used the shared repo `target/` with `CARGO_INCREMENTAL=0`. Scratch: `/tmp/audit/performance/dim_{1..8}.md`.

**Mode**: static analysis, guard-test lanes, two standalone `rustc -O` micro-benchmarks (scratchpad, not in the repo, no cargo target) and the two check scripts. **No GPU or CPU frame time was measured on the engine.** Costs are labelled *derived* or *micro-benchmarked*. Anything that needs live telemetry is deferred to the runtime audit.

## Executive Summary

| Severity | NEW | Existing / tracked (still present) |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 2 (#6248, #5959) |
| LOW | 1 | 6 (#6485, #6486, #6313, #6315, #6316, #6341) |

- **No guard has eroded.** Every `Guard:` symbol in all eight dimensions resolves (`git grep`, 60 names). Lanes: renderer `--lib` 1503 passed, core `--features inspect --lib` 891 passed, bin `byroredux` 3423 passed / 1 failed. The one failure is `readme_and_roadmap_hold_their_line_ceilings` (ROADMAP.md 808 lines against 800), which is a docs ceiling and not a perf matter (already in the ECS report). `per_frame_driver_functions_stay_within_their_size_pins`, the 10-27 failure, now passes. The physics `water_scan_latches_are_not_siphash`, debug-server `metrics_response_mirrors_core_snapshot` and plugin `locate_refr_*` lanes are green.
- **10-27 findings:** PERF-D1-01 (#6484, `vats_scan`) is **fixed in code** by `44cad6523` (#6444). `target_bodies_by_owner` is now built once per scan and passed to `visible_fraction`. The issue is still open in the snapshot and can be closed. PERF-D4-01 (#6485) and PERF-D5-01 (#6486) are unchanged and still present, so they are *Existing*.
- **Both NEW findings are in code added by #6114 and #6464.** The NIF-registry deferral (#6114) turned a per-insert O(N) LRU scan into a bulk trim on the main thread when a streaming batch settles (PERF-2026-10-31-D7-01, MEDIUM). #6464 widened the interior-to-exterior full ESM re-parse to Fallout 4 (PERF-2026-10-31-D7-02, LOW).
- **The M43 gameplay systems are not a CPU hot-path concern.** They are `random_conversation_system` (1 Hz), `mine_system` (4 Hz), `attraction_system` (2 Hz), `dead_body_discovery_system` (1 Hz) and `iron_sights_system` (trivial per frame). They sit behind Story Manager root or book gates and run on a clock. The two scans that walk every `SceneAliasCandidate` row cost 0.08 / 0.16 ms at 20 k rows and 0.42 / 0.98 ms at 100 k (micro-bench, SipHash probe per row), so no finding is filed. If either grows more probes per row, move membership into a spawn-time marker.
- **Observed vs ROADMAP: no delta can be stated.** No bench was run. The harness is byte-stable against the live record `d9a201c71` (`check-bench-harness-provenance.sh`). R6a-regress-23 is still open and cannot be attributed statically. The GPU-side delta since 10-27 is only constants, one UBO lane multiply in `composite.frag` (#6415) and the timer refactor, none of which adds a pass.

## Hot Path Analysis

### Per-pass GPU cost
Not measured, because no engine was launched. Inventory check: `GpuTimerSnapshot` has 30 `_ms` brackets (25 named + 5 main-render sub-phases), `QUERIES_PER_FRAME` is 60 and `BENCH_GPU_KEYS` has 24 entries. `every_named_bracket_has_exactly_one_pass_row` and `bench_gpu_keys_match_the_reported_bracket_order` pass. No new `cmd_dispatch` / `cmd_draw*` / copy / clear / fill / blit was added under `crates/renderer/src/vulkan` since the baseline, so the pass inventory needs no new row. RT quality tier: not applicable, since no frame was rendered. `scripts/check-shader-artifacts.sh`: "38 shaders + opaque early-test variant match glslang 11:16.4.0". `ENABLE_LEGACY_WRS` is 0.

### Per-frame / per-event CPU

| Site | Cost shape | Cadence |
|---|---|---|
| `NifImportRegistry::set_eviction_deferred(false)` (D7-01) | up to (N-cap) victims x up to 3 sweeps of N entries. Micro-bench: 0.11 s at N=4,096, 0.96 s at 10,000, 2.98 s at 16,384 (cap 2,048) | Once per settled streaming batch whenever the registry exceeded the cap |
| `parse_record_indexes_shared(.., exterior=true)` on FO4 (D7-02) | full load-order parse: `Fallout4.esm` 1.7-2.0 s, FO4 + 7 DLC about 2.85 s (memory-budget.md) | Once per session that started in an interior, at the first interior-to-exterior door |
| `mine_system` scan | Fx-free SipHash probe per placement row | 4 Hz; 0.08-0.42 ms at 20-100 k rows |
| `attraction_system` scan | two SipHash probes per row, plus actors x objects pairs | 2 Hz; 0.16-0.98 ms at 20-100 k rows |
| `random_conversation_system` pass | a few Vecs and std sets over eligible actors | 1 Hz, behind an ADIA/AHEL root check |
| `populate_scene_runtime` (per interior load) | now also rebuilds Crafting/Attraction/Mine/AimWeapons books (item, NPC and keyword scans; clones `furniture_keywords`) | Once per interior load; derived low single-digit ms, no finding |

## Findings

### Eroded guards
None.

### New issues

### PERF-2026-10-31-D7-01: the #6114 eviction deferral turns the registry trim into an O(excess x N) main-thread loop, about 1 s at the Fallout 4 working set
- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/nif_import_registry.rs` - `evict_over_cap` (907-1002), reached from `set_eviction_deferred(false)`; caller `sync_nif_cache_eviction` in `byroredux/src/app_step.rs`
- **Status**: NEW (introduced by `304f3b587`, Fix #6114). No open issue; the closed #6114 only covers the deferral itself.
- **Description**: `evict_over_cap` removes one victim per loop iteration, and choosing the victim sweeps `access_tick` up to three times (cell-unique and clip-free, then clip-free, then any), each entry doing a `clip_handles.contains_key(&str)` SipHash probe. That was fine when it ran once per insert at a registry one entry over the cap. With the deferral the registry may hold up to `DEFERRED_EVICTION_HEADROOM` (8) x cap (16,384), and when the batch settles `set_eviction_deferred(false)` calls `evict_over_cap(None)` on the main thread to drop everything above the cap in one go. That is E evictions x O(N) sweeps = O(E x N).
- **Evidence**: `while self.core.len() > cap { let victim = self.access_tick.iter().filter(... is_cell_unique_key(k) && !self.clip_handles.contains_key(k.as_str())).min_by_key(...).or_else(|| ...second sweep...).or_else(|| ...third sweep...) ...; self.core.remove(&victim_key); ... }`. The existing tests (`deferred_eviction_keeps_the_batch_resident_then_trims_to_the_cap`, `deferred_eviction_is_bounded_by_the_headroom`) use a handful of entries, so nothing times the trim.
- **Impact**: A main-thread hitch at the end of every large streaming batch on Fallout 4's default radius (about 9,700 shared models plus per-cell precombines against a 2,048 cap). A standalone replica of the loop (`rustc -O`, synthetic path keys, same three-tier victim choice, scratchpad `lru/b.rs`) measured 0.11 s for 4,096 entries, 0.96 s for 10,000 and 2.98 s for 16,384. Real keys and the real `core` map will shift the constant but not the shape. It runs between frames, but nothing budgets it (`FrameTimeBudget` does not wrap `sync_nif_cache_eviction`). No quantitative guard exists for this site.
- **Related**: #6114 (closed; this is a cost it introduced), `docs/engine/memory-budget.md` "Streaming deferral" row (per-entry RAM at the deferred peak is also unmeasured).
- **Suggested Fix**: Make the trim a single pass: when `len > cap`, collect `(tier, tick, key)` for all entries once, `select_nth_unstable` the `len - cap` smallest, and remove them (O(N)). Keep the per-insert path as is for the steady state. Add a test that trims a 10,000-entry registry under a loose wall-clock bound, or at least asserts the sweep count.

### PERF-2026-10-31-D7-02: #6464 makes Fallout 4's first interior-to-exterior door re-parse the whole load order on the main thread
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/load_order.rs` - `needs_exterior_processing` (607-614) and `SharedParseCache::get_or_parse` (697-727); the exterior leg is `build_exterior_world_context` called from `byroredux/src/app_step.rs` (~1465)
- **Status**: NEW (introduced by `977936719`, Fix #6464)
- **Description**: `needs_exterior_processing` now includes `GameKind::Fallout4`, and the shared parse's `exterior_ready` reads the same predicate. An index parsed for an interior is therefore not served to an exterior session on FO4 (`EsmIndex` is not `Clone`, so it cannot be processed in place) and the whole order is parsed again. Before #6464 FO4 interior-to-exterior reused the resident index, which was #6123's point. The skill's premise names only Oblivion and FO3/FNV as re-parsing "by design". The cost is paid once: after the re-parse the slot is `exterior_ready`, and later exterior-to-interior transitions reuse it.
- **Evidence**: `matches!(game, Oblivion | Fallout3NV | Fallout4)`; `let ready = shared.exterior_ready || !exterior; if shared.files == files && ready { return reuse }`, else `parse(plugin_paths)` then `apply_exterior_processing`. The test `the_cache_and_the_processing_step_share_one_gate` pins that FO4 now reparses.
- **Impact**: A 1.7-2.0 s (`Fallout4.esm`) to about 2.85 s (FO4 + 7 DLC) synchronous parse, with a parse-time RSS peak of roughly 1.1-1.4 GB on top of the still-resident interior index, at the first door out of an FO4 interior when the session began in an interior. Sessions that begin in an exterior (`--grid`) are unaffected. The correctness gain (29 LAND-less cells now stand on terrain) is real, so this is a cost to bound, not to revert.
- **Related**: #6123, #6407, #5505.
- **Suggested Fix**: Apply `inherit_parent_landscapes` in place when the slot's `Arc<EsmIndex>` is uniquely held (`Arc::get_mut` after the interior session drops its clone), or run the inheritance on the interior parse too (it is idempotent and only fills LAND-less cells) so one parse serves both session kinds.

### Existing / still present
- **#6485** (LOW, PERF-D4-01): `moved_surface_ids` is still a fresh per-frame `Vec` with a linear `contains` dedup (`build_and_upload_instances.rs:394`, `:975`). Unchanged.
- **#6486** (LOW, PERF-D5-01): `composite.frag:592` still does `inverse(params.inv_view_proj)` per submerged pixel. The window added one multiply (`cameraCloudSunT`, #6415) and replaced the IOR literal with `WATER_CLEAN_IOR` (#6418); neither changes the cost shape. The submerged `composite_ms` measurement is still owed to the runtime audit.
- **#6484** (PERF-D1-01): fixed in code by `44cad6523`; the issue can be closed.
- **#6248** (MEDIUM), **#5959** (MEDIUM), **#6313**, **#6315**, **#6316**, **#6341** (LOW): still present in code, not re-filed. #6248 is still amplified by the 1.69x blade-area change from c9433d501 with no A/B on record.

### Checked, no finding
- `random_conversation_system`: `derive_location` is the memoised path (`StoryLocationMemo`), so the #6335 regression shape is absent. `conversation_hold` is one empty sparse query on a quiet frame.
- `dropped_item_story_event_system`: early-outs on an empty pending list. `step_texture_swaps` flushes DDS uploads only when `pending_dds_upload_count() > 0` (#6111).
- `bench_capture` (`--bench-capture-every`): the off path is one `Option` check per frame.
- Coarse terrain LOD's one-cell resident margin (#6452) adds a ring of LOD quads; negligible against the pool reserve.
- ESM parse additions (`kwda_keywords`, `AORU`): allocate only on records that carry `KWDA`, load time only.
- Geometry admission tiers: no scene-geometry upload site changed.
- Origin: `rebase_model_matrix` and `origin_corrected_prev_view_proj` unchanged.

## Prioritized Fix Order
1. **PERF-2026-10-31-D7-01**: single-pass bulk trim in `evict_over_cap`. Local to one function; a timing-bounded test can pin it.
2. **#6485** with REN-D2-01 (#6493): persistent mover scratch with bounded dedup and a `ScratchTelemetry` row.
3. **#6486**: carry the forward `view_proj` in the composite UBO (shader-struct lockstep), then measure submerged `composite_ms` at the runtime audit.
4. **PERF-2026-10-31-D7-02**: avoid the FO4 re-parse by processing in place or earlier.
5. Close #6484 (fixed by 44cad6523).

## Stale skill premises
- Dim 7 says Oblivion and FO3/FNV re-parse interior-to-exterior "by design"; since `977936719` (#6464) Fallout 4 does too (`needs_exterior_processing`).
- Dim 7 describes the deferral as holding eviction "up to 8x the cap and trims back"; it does not state that the trim is the O(excess x N) loop above.
- Dim 4 still has no line or `memory-budget.md` row for the #6250 moved-surface buffer (binding 25, 260 B per frame in flight, `upload_moved_surface_ids`).
- Dim 1 should list the M43 clocked systems (`mine_system` 4 Hz, `attraction_system` 2 Hz, `dead_body_discovery_system` and `random_conversation_system` 1 Hz) and the fact that they scan `SceneAliasCandidate` with SipHash maps; the micro-bench figures above give the baseline.
- Confirmed unchanged: `VERTEX_POOL_HARD_CAP` 7 M / `INDEX_POOL_HARD_CAP` 28 M, `DRAW_SORT_PARALLEL_THRESHOLD` 3000, `SKINNED_BLAS_REFIT_THRESHOLD` 600 + jitter 60, `QUERIES_PER_FRAME` 60, 30 brackets, `BENCH_GPU_KEYS` 24.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-31.md`
