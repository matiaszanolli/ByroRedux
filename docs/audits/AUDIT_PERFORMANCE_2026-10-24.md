**HEAD**: `df33c84e5` (audited; the tree advanced to `da22354f1` during the run through the in-progress #6139 ObScript command-table commits, which touch only `crates/scripting/src/obscript_*` and no audited path) · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-23.md` (HEAD `1d88bfe85`, 40 commits since) · **Audited**: Dims 1, 3, 7 (delta read plus guard lanes) and Dims 2, 4, 5, 6, 8 (guard lanes plus skim) · **Unchanged since baseline (skimmed)**: Dims 2, 4, 5, 6 (no commit in their paths); Dim 8 changed only by the two fixes for the baseline's own findings (#6336, #6337)

# Performance Audit — 2026-10-24

**Command**: `/audit-performance`, three sub-agents (Dim 1; Dims 3+7; Dims 2/4/5/6/8). Scratch: `/tmp/audit/performance/`.

**Mode**: static analysis, filtered guard-test lanes, `scripts/check-shader-artifacts.sh` (read-only), `scripts/check-bench-harness-provenance.sh`, and a few throwaway probes outside the repo. No engine, GPU process or bench was launched. **No GPU frame time is claimed.** Every figure is labelled *measured* (probe or guard run) or *derived* (code plus a cited input).

## Executive Summary

| Severity | NEW | Existing / tracked (still present) |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 1 (#6248) |
| LOW | 5 | 3 (#6313, #6315, #6316 closable) |

- **No guard eroded.** Every `Guard:` symbol across all eight dimensions resolves and its filtered lane passes (renderer lib `acceleration` 151, `gpu_timers` 19, `origin`/`snap` 18; bin `draw_sort_key` 27 / `sort_key` 34; streaming and cell-loader bin lanes; plugin `locate_refr`; NIF dhat 7+1+1; core `--features inspect` and scripting lanes).
- **The baseline's only MEDIUM is closed.** PERF-D3-02 (the #6092 starvation re-arm cycling through compaction) is fixed by `021e1b824` (#6332). Traced end to end, not just by its tests: `abandon_scene_geometry_group` truncates the pool tails, so no hole is left, no compaction runs and the admission latch is never cleared. Both multi-sub-mesh rings use it with no early exit between `begin_` and the unwind, and the `drop_meshes` fallback is unreachable for them (both upload only through `upload_scene_mesh_global_only`).
- **Every other baseline LOW is fixed**: PERF-D1-03 (#6333), -04 (#6334), -05 (#6335), PERF-D8-01 (#6336, the provenance script now annotates `994620123`) and PERF-D8-02 (#6337, `bench_gpu_samples` is one keyed table and the order test pins all 24 rows).
- **The new findings are all in code added this window**: the new ambient-AI package systems (Find, Flee, UseWeapon), the crime pass, and one predicate mismatch in #6097's worker skip. Five NEW LOW (PERF-D1-07, -08, -09, PERF-D5-03, PERF-D7-01), none sustained at a scale that is measured to matter. The dominant theme: the new package systems skip the "resolve a FormID target once and cache it" rule that Follow/Escort/Travel/Guard honour (#5724).
- **Observed-vs-ROADMAP delta: none can be stated.** No bench was run. The live record is `d9a201c71`; the harness is byte-stable against it (Dim 8). R6a-regress-23 (HEAD 1–9% slower than the control on every scene, `gpu_main` +0.5–1.2 ms on Cornell / Whiterun / MedTek unattributed) is still open and a static pass cannot attribute it. Nothing in this window touches the renderer, so it cannot have added to it.

## Hot Path Analysis

### Per-pass GPU cost
Not measured (no engine). No render, shader, scene-buffer, skinning or timer code changed in this window, so the pass table is the same as the 2026-10-23 report's. Inventory check against the live `draw_frame`: every `cmd_*_start`/`_end` pair maps to a bracket; **30 brackets (25 named + 5 sub-phases), `QUERIES_PER_FRAME` 60**, `BENCH_GPU_KEYS` 24 (the dev-only `groundcover_bench` bracket is not on the bench line). One unbracketed per-frame pass is missing from the inventory: the water-caustic accumulator clear (PERF-D5-03).

RT quality tier: not applicable, since no frame was rendered. Any later A/B must pin `--rt-test-ray-quality-tier`.

### Per-frame CPU (measured unless marked derived)

| Site | Cost | Label |
|---|---|---|
| `resolve_entity_by_global_form_id` miss/full scan (PERF-D1-07), per call | 4.4 µs @ 2k, 22.4 µs @ 10k, 88.1 µs @ 40k `FormIdComponent` entities (≈2.2 ns/entity; a mid-storage hit is half) | measured, throwaway probe, release, hot cache |
| Same scan, independent probe by the Dims 3+7 agent | 10.5 µs @ 5k, 65.6 µs @ 30k, 131 µs @ 60k | measured, agrees to within ~10% |
| `find_package` Kind search shape (PERF-D1-08) | 23 µs @ 3k refs, 193 µs @ 20k, 642 µs @ 60k, once per second per target-less actor | measured, shape probe (real `ItemRecord` size not reproduced) |
| `find_package` Base search shape | 3 / 21 / 68 µs at the same sizes | measured, shape probe |
| `jail_bed_system` bed `Vec` (PERF-D1-09) | ≈0.5–3 µs per frame while jailed | derived |
| Instance/previous-model/material upload hash gates | 43 µs @ 7,359 instances, 114 µs @ 20,000; 17 µs previous models; 62 µs @ 4,000 materials, 123 µs @ 8,000 | measured, probe |
| `svgf_material_hash` loop (#6315) | unchanged since baseline: 98 µs @ 4,000 materials, 194 µs @ 8,000 | measured (baseline), code untouched |
| Composite caustic reads + water-side clear (PERF-D5-03) | ≈33 MB read + 8 MB cleared per frame at 1080p, ≈166 MB at 4K, upper bound before cache effects | derived, unmeasured |
| `populate_scene_runtime` (≈40 whole-index installers per interior load) | aggregate not sized (needs a loaded `EsmIndex`); the one new installer (`install_crime_faction_data`) is a single pass, microseconds | derived |

### Harness provenance
`scripts/check-bench-harness-provenance.sh d9a201c71` (repo not shallow), exit 0, verbatim: *"d9a201c71: harness byte-stable since this record — a re-run is a valid apples-to-apples comparison against it."* `a3a7a43b0` reads DIVERGED with **2** edits (`994620123`, `fc83608d3`) and `a37fcba3c` with **4**; every one now prints a recorded disposition. Neither harness file (`scripts/fsr-bench-matrix.sh`, `scripts/fsr_bench_report.py`) has a commit since `d9a201c71`.

## Findings

### Eroded guards

None.

### New issues

### PERF-D1-07: Flee, UseWeapon and the UseWeapon runtime resolve a FormID target with an O(all `FormIdComponent`) scan every frame, per actor
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/flee_package.rs:108-120` (threat and refuge resolve in the per-actor, per-frame pass; `position_of(behavior.refuge_form_id)` is an eager argument to `flight_goal`); `byroredux/src/systems/use_weapon_package.rs:57-83` (target resolve per un-started actor per frame); `byroredux/src/systems/script_requests.rs:54-62` (`use_weapon_system` resolves `run.request.target` per live volley per frame, and builds a fresh `runs: Vec` per frame).
- **Status**: NEW (`b540b104a` Flee, `10f8cbb91` UseWeapon; the `script_requests.rs` site predates them as a one-shot script command but became permanent once a package raises its request). Found independently by the Dim 1 and Dim 7 reviews (the latter filed it as PERF-D7-02); merged here.
- **Description**: `resolve_entity_by_global_form_id` (`crates/scripting/src/condition/eval.rs:179-203`) walks every `FormIdComponent` entity and calls `FormIdPool::resolve` on each; its own doc says it is "fine for the occasional resolution ... not a per-frame hot path" (`0x14` short-circuits before the scan). Follow and Escort resolve once into `FollowState`; #5724 made Travel, Escort-lead and Guard do the same; `find_package` caches `FindProgress.target`. The three sites above resolve every frame:
  - `flee_package_system_inner` resolves the threat and the refuge for every eligible actor, including one merely holding at a safe distance, and the refuge scan runs even when a live threat decides the goal.
  - `use_weapon_package_system` collects not-yet-started actors into a fresh `Vec` and resolves each target every frame; the module doc says "a package whose target never loads simply never fires", so an actor whose target lives in another cell pays a **full miss scan every frame for as long as the package is active**.
  - `use_weapon_system` resolves the target of every live volley each frame. A package volley with Repeat Fire never finishes, so this is a permanent per-frame cost per active shooter.
- **Evidence**: scan body is `q.iter().find(|(_, fid)| pool.resolve(fid.0).is_some_and(|p| p.local.0 == form_id))`. Measured per call (two independent probes, see the CPU table): about 4.4 µs at 2k, 22–32 µs at 10–15k, 65–131 µs at 30–60k entities. The number of simultaneous such actors in real content is not measured (vanilla FNV authors about 15 Flee packages; the smoke gate has two shooters in ULKitchen).
- **Impact**: derived. Zero unless an actor carries one of these packages. In a 2,000-ref interior, single-digit µs per actor-frame. In a 10–60 k-entity exterior working set, 11–130 µs per call, so four actors resolving 1–2 targets each is roughly 0.1–0.7 ms of serial main-thread time per frame (the systems are exclusive). Linear in actor count and in loaded refs, no cap, and it grows with exactly the entity count the streaming work raises.
- **Related**: #5724 (the once-and-cache fix for the older movers), #2561 (shared resolve helper), #6313 (other per-frame whole-structure work), PERF-D1-08.
- **Suggested Fix**: cache the resolved `EntityId` in the behavior's state the way `FollowState` does (revalidate with `world.has::<GlobalTransform>`) and retry an unresolved target on a coarse clock (1 s). Better: one `FormId -> EntityId` index keyed on `FormIdComponent`'s existing structural generation (`TRACK_CHANGES = true`), which also serves `use_weapon_system`, `faction_hostility::ambush_gates` and `find_package::search`. Compute the refuge only when `flight_goal` needs it; keep `pending`/`runs` as persistent scratch like `FleePackageScratch`.

### PERF-D1-08: `find_package_system` runs a whole-world candidate scan per target-less actor each second (SipHash lookup per ref for Kind searches), and re-derives the held target's class with allocations every frame
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/find_package.rs:115-170` (`search`; the `item_kind` lookup at `:77-85,144`), `:174-182` (`behaviors` Vec), `:213-227` (search clock), `:230-245` (`is_actor` / `actor_would_attack` / `is_pickup_target` per frame), `:96-103,176-180` (scratch maps).
- **Status**: NEW (`fcd1cce4a`, reshaped by `ce22fcbcf`)
- **Description**: three separable costs.
  1. *The search.* For a Base or Kind target the actor walks **every** `SceneAliasCandidate` row once per second until it finds something. A Kind search evaluates `item_kind(index, base)` per row, i.e. `index.items.get(&base)` on a std (SipHash) `HashMap<u32, ItemRecord>` (`crates/plugin/src/esm/records/index.rs:25,280`), and the radius test comes after the kind filter. Actors installed in the same frame keep aligned clocks (`entry starts at 0`), so K such actors search on the same tick.
  2. *Per-frame allocation and re-derivation.* `behaviors` is a fresh `Vec` every frame any Find actor exists (sibling `flee_package.rs` keeps persistent scratch, #2033). For a held actor target `actor_would_attack` runs every frame: it clones `FactionRelations`, takes an `Arc` of the catalog and builds two `Vec<FormRef>` per call (`faction_hostility.rs:202-228`). `is_pickup_target` makes four sparse lookups plus a catalog probe per frame for an item target. An actor that has arrived at a friendly NPC (`Arrival::Stand`, never completes) also re-writes `FindProgress` through a `query_mut` every frame.
  3. *Scratch growth.* `search_clock` and `stuck` are cleared only when **no** actor carries `FindPackageBehavior`; entries for actors that streamed out or changed package are never pruned (unlike `EatSleepScratch`, #5707). 12–16 bytes per Find actor ever seen per session.
- **Evidence**: measured shape probe (CPU table). The per-frame items in point 2 are derived: about 0.3–1 µs per held actor-frame.
- **Impact**: derived. A Kind-searching actor with no target costs a 0.2 ms spike (20 k refs) to 0.6 ms (60 k refs) once a second, multiplied by the number of aligned searchers; about 3–10 µs per searcher per frame averaged at 60 fps. No sustained cost beyond that.
- **Related**: PERF-D1-07; `interaction.rs:1259-1325` already notes that item placements are a small share of the `SceneAliasCandidate` set.
- **Suggested Fix**: precompute `base -> Option<FindItemKind>` once per `LoadedCellIndex` swap (an `FxHashMap`, or fold into `InventoryCatalog`, which `is_pickup_base` already consults) and do the cheap `GlobalTransform` radius test before the kind classification. Stagger each actor's first `search_clock` (for example `entity % period`). Make `behaviors` persistent scratch, evaluate the arrival class only when `progress.target` changes, and prune `search_clock`/`stuck` against the live behavior set as `EatSleepScratch.live` does.

### PERF-D1-09: `jail_bed_system` gathers every bed into a fresh `Vec` every frame while the player serves a sentence, before checking for an activation
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/crime.rs:321-335` (`bed_entities`), `:338-355` (`jail_bed_system`)
- **Status**: NEW (`0c6cd46dc`)
- **Description**: the system returns early unless `player_in_jail`, the right outer gate. Once jailed it calls `bed_entities` (an `iter` over every `Furniture` row with a `markers.iter().any(...)` and a `collect()` into a fresh `Vec`) on **every** frame, and only then asks whether an `ActivateEvent` from the player targets one of them. `ActivateEvent` is a transient marker empty on nearly every frame.
- **Evidence**: code read; no probe, derived at about 0.5–3 µs per frame for tens to a few hundred furniture rows.
- **Impact**: negligible, and limited to the time a sentence is being served. Filed because it is a per-frame `Vec` built ahead of a rare event in a system added this window.
- **Suggested Fix**: read the `ActivateEvent` storage first and return when no event has the player as activator; test `world.get::<Furniture>(target)` per matching event, or build the bed set only on that frame.

### PERF-D5-03: the composite pass fetches four caustic-accumulator texels per geometry pixel, and the water-side accumulator is cleared every frame, whether or not any caustic source exists; neither is in the pass inventory or any timer
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/composite.frag:646-675` (the reads); `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:1524` (`wca.clear_pre_render_pass`) and `:1266` (`water_caustic_active: self.post.water_caustic_accum.is_some()`); `crates/renderer/src/vulkan/water_caustic.rs:185`
- **Status**: NEW (not found in open or closed issues; related and closed: #2789, #3844, #3646)
- **Description**: two fixed per-frame costs scale with render pixels and do not depend on whether the scene has water or refractive glass.
  1. Every geometry pixel in `composite.frag` does three `texelFetch` on the glass accumulator (`causticTex`, 3 layers of R32_UINT) and one on the water accumulator (`waterCausticTex`). The only gate is `params.caustic_flags.x`, which the host sets from `water_caustic_accum.is_some()`, true for the whole session. There is no "no caustic content this frame" signal. When no source exists, `CausticPipeline::clear_for_skip` zeroes the glass accumulator once per skip streak, so the composite reads zeros.
  2. `WaterCausticAccum::clear_pre_render_pass` runs on every frame the accumulator exists (its own comment: "runs unconditionally every frame"): one render-extent R32_UINT clear plus two barriers before the main pass. The glass side has the skip-streak latch (`skip_clear_decision`); the water side has no equivalent.
  No pass-inventory row exists for either, and no timer brackets the clear, so a bench cannot see it.
- **Evidence**: `composite.frag:651-664` reads layers 0–2 and the water texture behind an always-set uniform flag; `memory-budget.md:375-390` (Glass + Water Caustics) records both accumulators at full render resolution, 4 B/px per layer (`CAUSTIC_FORMAT` R32_UINT).
- **Impact** (derived, not measured): reads 16 B/px and the water clear writes 4 B/px per frame: about 33 MB read plus 8 MB written at 1920×1080, about 166 MB at 3840×2160. L2 residency of the zero-filled reads is unknown, so the real cost may be much smaller. Treat as LOW until a `composite_ms` A/B on a no-water, no-glass cell shows at least 0.1 ms.
- **Suggested Fix**: derive one `caustic_content_live` flag (glass dispatched this frame or its decay history non-empty, or a water draw recorded for this slot) into `caustic_flags` and let `composite.frag` skip all four fetches when clear; gate the water clear with a once-per-streak latch like `skip_clear_decision`; add a pass-inventory row. **Speculative-Vulkan caveat**: the barrier change is invisible to `cargo test`, so it needs RenderDoc evidence or a revert plan before landing. Confidence: medium on existence, low on magnitude.

### PERF-D7-01: #6097's worker skips a bake's refs when its `_oc.nif` merely exists; the main thread only treats a bake that spawned geometry as absorbing, so a present-but-empty bake sends its refs to a synchronous main-thread parse
- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/streaming/pre_parse.rs:677-690` (`bake_absorbed_refs`), `:760-765` (the `has_mesh` predicate), `:776-781` (the skip); versus `byroredux/src/cell_loader/precombined.rs:545-547` (`if count > 0 { self.baked_hashes.push(hash) }`) feeding `effective_absorbed_refs` in `exterior/apply_job.rs`.
- **Status**: NEW (follow-up to the closed #6097; not covered by #6114/#6116). The orchestrator re-read the three code sites and confirms the predicate mismatch; the `bake_absorbed_refs` doc admits only the decode-failure case.
- **Description**: `88149ae26` makes the worker skip STAT/SCOL refs of every bake whose `_oc.nif` is present in the archives (`tex_provider.has_mesh(key)`, an archive-table `contains`). The main thread skips a ref only if its bake's hash is in `baked_hashes`, and `PrecombinedSpawnJob` pushes a hash only when the bake spawned `count > 0` entities (#5484: a bake that drew nothing — zero-mesh `_oc.nif`, CSG-deferred fallback — is not a carrier of its refs' geometry). The worker's predicate is a strict superset. For a bake that exists but draws nothing (zero-mesh file, CSG routing deferred, parse/CSG decode failure, or every upload refused by a closed geometry pool, #6106), the refs are rendered per-REFR on the main thread, but the worker never parsed their models, so each unique model takes an inline `extract + parse_and_import_nif` on the main thread, the cost the worker exists to remove.
- **Evidence**: code read (above). Those refs are about 95% of an FO4 exterior cell's placements (the #6097 premise).
- **Impact**: bounded to cells whose bake exists but draws nothing. Vanilla `Fallout4.esm` with its CSG present should be clean; the exposure is an install without the shared-geometry `.csg` (every bake then defers, so every cell takes the synchronous fallback) and the already-degraded #6106 pool-refusal case. Not measured (needs FO4 data and an engine run); no cost claim.
- **Related**: #6097 (closed), #5484, #6106, #6114.
- **Suggested Fix**: derive the worker's absorbed set from bakes the worker actually parsed with at least one mesh (order the ref-model collection after the `_oc.nif` pre-parse, or gate on the same `count > 0` signal), or at least log the worker's `skipped_absorbed` against the main thread's `absorbed_skipped` so a mismatch shows in the cell log. Secondary, doc rot in the same hunk: the `pre_parse_cell` doc block (`pre_parse.rs:652-664`) now sits above `bake_absorbed_refs`' doc, leaving `pre_parse_cell` undocumented and `bake_absorbed_refs` with two stacked blocks.

### Existing / still present

- **#6248 (MEDIUM)**: every grass-blade fragment recomputes the blade's constant root ground colour, up to ~18 bindless fetches. `groundcover_blade.frag` is byte-unchanged. Still deferred pending a `groundcover_blade_draw_ms` A/B.
- **#6313 (LOW)**: `encumbrance_system` perk-list snapshot, `due` clone and `applied` `Vec` per frame. `byroredux/src/encumbrance.rs` has no commit since the baseline. Keep at LOW or close with the "needs a change signal on `Inventory`/`ActorValues`/perks" note.
- **#6315 (LOW)**: `fold_svgf_scene_inputs` still copies and masks 23 lanes per interned material per frame (98 µs @ 4,000, 194 µs @ 8,000). Its proposed fix (reuse the table's stored hashes) is invalid after #6247 because those hashes include the animated lanes; the issue body needs correcting. Workable shape: share one `hash_material_slice` result between the SVGF key and `upload_materials`, and cache the folded key while that slice hash is unchanged.
- **#6316**: the bracket exists and reads 0.033 ms inside ~0.22 ms `sky_cube_ms` (the author's figures, not re-measured). The remaining cost-motivated work is at most ~0.03 ms, so the issue could close on the numbers.
- **#5959**, **R6a-regress-23**, **#5365**: open; a static pass cannot attribute them.
- **Carried forward, code untouched in the window**: #6114 (its FO4 census of 9,674 unique shared models **predates #6097**, which removes absorbed refs' models from that working set; re-run it before the cap decision), #6116, #6106, #5764.
- **Residual by design (not filed)**: the geometry-admission latch is global, so after a refusal even uploads that would fit stay refused until a genuine hole is compacted (#6054's contract, the same shape as #6106). #6332 makes this honest; the old unwind masked it by reopening.

### Checked, no finding

- **Dim 3**: #6332 verified against PERF-D3-02 (8-step trace; no other partial-upload unwind path remains: terrain and tree LOD upload one mesh per attempt, `upload_scene_meshes_batched` is all-or-nothing with rollback, the remaining `drop_mesh(es)` callers are genuine frees). #6305 (`apply_texture_swap`) and the crime installers in `asset_provider/script.rs` are cost-neutral. BLAS budget, admission, scratch floors, staging budget and deferred-destroy constants all match the skill text; no new screen-sized allocation landed.
- **Dim 7**: #6097 is correct (it reuses the main thread's own type gate `precombine_can_replace_record` and `effective_absorbed_refs`; 4 `absorbed_ref_tests` pass). #6098 costs at most two `has_mesh` probes per candidate cell inside the existing ring budget and caches a miss as a sentinel. #6135 and #5495 are cost-neutral. The AI-package install side adds only match arms and component inserts; every new runtime system early-outs when no actor carries its component. The Ambush gate (`faction_hostility::ambush_gates`) is std-hashed but runs at 0.5 s; the guard-rush `waiting` map is std-hashed but is empty almost always. Crime reporting paths are event-driven. `populate_scene_runtime`'s ≈40 whole-index installers per interior load have an unmeasured aggregate cost (needs a loaded index) and were not part of the baseline either.
- **Dim 1**: the waypoint clone in `cached_walk_step` is the accepted disposition of #3269; `archetype_idle_system` still clones the (usually empty) `AnimArchetypeOverrides` map each frame ahead of its early-out, a sub-µs residue of #6334.
- **Dim 2**: ~10–14 sparse-set reads per resident mesh entity in `collect_static_mesh_draws` before the frustum verdict is the in-code #501 deferral; no new site is larger than a sparse get.
- **Dim 4**: hash-gate cost is about 0.06 ms of CPU together at a MedTek-like 7.4 k draws and skips a 1.12 MiB copy in steady state; `upload_bone_worlds` scans at most `MAX_SKIN_SLOTS` (1,364) slots then copies O(dirty); `classify_pbr_keyword` has no render-path caller.
- **Dim 5**: the shader-side `inverse(` hits are a mat3 in `volumetrics_inject.comp` (small next to the six neighbour fetches before it, and #5950 already moved the lead to the quadrature) and the gated inverse-transpose in `normal_transform.glsl`; `ENABLE_LEGACY_WRS` ships 0 (`check-shader-artifacts.sh`: 38 shaders + the opaque early-test variant match glslang, tree left clean); the selected-light ray probe read and its per-frame HOST barrier show no demonstrated cost.
- **Dim 6**: the CPU pose-hash over each visible skinned mesh's palette is about 0.1 ms for 100 actors × 60 bones (derived); `GlobalTransform` has no change signal, so it is rebuilt per frame. Below any measured threshold; overlaps Dim 1.
- **Dim 8**: #6337's `bench_gpu_samples` is evaluated once when the `bench:` line prints, and the printed `gpu_<key>=<ms>` token shape is unchanged, so the sweep extractors still match. One residual gap: the order test checks each row's key literal, not that the row's `s.gpu_*_ms`/`_active` fields belong to it.

## Prioritized Fix Order

Quick wins first (scratch reuse, gate restoration, caching), architectural last. None is urgent; all are LOW.

1. **PERF-D1-07**: cache the resolved target in the Flee/UseWeapon behavior state (or add the `FormId -> EntityId` index). The index also fixes `find_package` and the Ambush gate, and it is the only item here that scales with the streamed entity count.
2. **PERF-D1-08**: move `behaviors` to persistent scratch, stagger the first search clock, prune the two maps, and precompute `base -> kind`.
3. **PERF-D1-09**: read `ActivateEvent` before building the bed `Vec`.
4. **PERF-D7-01**: align the worker's absorbed set with the main thread's `count > 0` rule, or log the mismatch; re-attach the `pre_parse_cell` doc. Re-run #6114's census after #6097.
5. **PERF-D5-03**: needs a `composite_ms` A/B on a no-water, no-glass scene first; only then add the `caustic_content_live` flag and the water-clear latch, with RenderDoc evidence for the barrier change.
6. **Housekeeping**: correct the #6315 body (the stored-hash fix is invalid), consider closing #6316 on the numbers, and decide #6313.

## Stale skill premises

For `/audit-sync` to pick up (not edited here):
- **Dim 2**: `Paths`/`First step` omit `crates/renderer/src/vulkan/context/geometry_pass.rs` (the indirect gather loop and the `cmd_set_depth_bias` gating live there). The renderer-side guards need `cargo test -p byroredux-renderer --lib <filter>`, while `draw_sort_key` is a bin test (`cargo test -p byroredux --bin byroredux draw_sort_key`). The Guard line omits the `frame_params.rs` modules `group_state_tests`, `needs_two_sided_blend_split_tests` and `should_use_indirect_draws_tests`. A doc comment in `draw_sort_key_tests.rs` still says "11-tuple" for the 12-wide key.
- **Dim 4**: the "`fold_svgf_scene_inputs` … unmeasured; quantify before filing" sentence is stale (measured 98 / 194 µs; the hash is `svgf_material_hash`). The `bone_world_slot_needs_copy` tests live in `bone_world_slot_state_tests`.
- **Dim 5**: the pass inventory lacks a row for the water-caustic accumulator clear; the "Main geometry pass" row omits terrain parallax (#6197) and "Ground-cover model tier" omits the wind lean (`GROUNDCOVER_MODEL_RECORD_FLAG_SWAYS`).
- **Dim 7**: `load_order_parallel_tests` is a file name; the module is `cell_loader::load_order::parallel_tests`, so a `-- load_order_parallel_tests` filter matches 0 tests with exit 0. Filter on `parallel_walk_matches_the_inline_walk_on_a_localized_master_chain` and `digest_ignores_hash_order_but_not_sequence_order_or_values`. The Dim 7 text should also note #6097 as landed and the #6114 census caveat above.
- **Dim 8**: the "`BENCH_GPU_KEYS` must follow the bracket order" clause predates #6337: the rows of `bench_gpu_samples` now carry the keys and the test pins all 24.
- **Dim 3**: the #6332 / `abandon_scene_geometry_group` contract is already in the skill text; add `an_abandoned_group_leaves_no_hole_and_admission_stays_closed` to the Guard line.
