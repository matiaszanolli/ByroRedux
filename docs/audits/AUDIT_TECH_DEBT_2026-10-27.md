# Tech-Debt Audit — 2026-10-27

**HEAD**: fa6be4a6b · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d; 160 commits since) · **Audited**: Dims 1, 2, 3, 4, 7, 8 (delta-scoped to c7463c04d..fa6be4a6b) · **Unchanged since baseline (skimmed)**: Dims 5, 6, 9 (recipes re-run, no new instances)

Part of `/audit-suite --preset comprehensive`: run synchronously with no sub-agents. Builds used the repo `target/` only. Nothing was committed, filed or edited outside this report and `/tmp/audit/tech-debt/`.

In one sentence: the 10-25 backlog was worked down hard. The 10-27 consolidation wave (#6362–#6373), the #6142/#6143 function splits and #6140/#6141 file splits removed every production function over 1000 lines and three of the five files over 2000 LOC. The clippy gate is green. This window's debt comes from the geometry-pool tier work, the underwater god-ray shader and new guard tooling whose blind spots are already visible.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 8 |
| **Total** | **8** |

By dimension: Dim 1 ×1 · Dim 2 ×2 · Dim 3 ×2 · Dim 4 ×1 · Dim 7 ×1 · Dim 8 ×1 · Dims 5, 6, 9 ×0.

- **Gates**: the blocking `cargo clippy --workspace --keep-going -- -D warnings` gate is **green** on the pinned 1.96.0 toolchain (0 warnings, every crate including the bin). `cargo machete --with-metadata` is clean. All 26 `workspace_hygiene_tests` pass. `_audit-validate.sh` is OK (3804 refs across 124 skill files). The non-blocking `--all-targets` clippy step was not re-run: REG-2026-10-27-05 already reports it red with 6 lints.
- **Cross-audit dedup**: this window's doc-rot, test-gap and magic-number items are mostly already filed by sibling audits: REN-D8-03, NIFAL-D4-01, ESM-04, PHYS-D3-02, AUD-D1-02, UI-D4-01, TOOL-D2-01, EXT-D2-01/-02, EXT-D3-01, EXT-D6-04, NIF-D4-02, REN-D1-01, SAVE-02, PAR-D4-01/-02, GAME-D1-01, NIF-D4-01, LC-D1-01, plus #6414 and #6418–#6420. They are cited below rather than re-filed.
- **Issue housekeeping (recommendations, not actions)**:
  - **#6355**: premise gone. `obscript_effects.rs` is 1842 prod LOC (was 2034) after #6365.
  - **#5586**: premise still dead (10-25 note).
  - **#6353**: none of its eleven sites appears in today's closed-issue advisory, because the eae8fd740 sync rewrote them. Re-check, then close.
  - **#6356 / #6357**: both shrank slightly (inventory 1956 → 1944, npc_spawn 1911 → 1901).
  - **#6359**: unchanged (982 / 853).

### Delta vs the 2026-10-25 baseline

| Metric | 10-25 (c7463c04d) | 10-27 (fa6be4a6b) | Note |
|---|---:|---:|---|
| markers (TODO/FIXME/…) | 26 | 26 | no marker token added in 160 commits; 0 shader hits; triangle.frag MIT/Burley block intact |
| `allow(dead_code)` | 41 | 39 | #6398 retired two `cfg_attr(not(test), …)`; 12 are `lock_tracker.rs` cfg(test) markers |
| `unimplemented!` / `todo!()` | 0 | 0 | |
| `#[ignore]` tests | 373 | 373 | zero added or removed |
| production files >2000 LOC | 5 | **2** | `shader_constants_data.rs` 3150 (data table, +178 from #6390–#6392) and `mq101_conformance.rs` 2157 (#6360) |
| production fns >200 / >300 / >500 / >1000 | 206 / 83 / 28 / 3 | **206 / 79 / 25 / 0** | scanner reproduces the 10-25 figures exactly at c7463c04d; every >1000 fn split (#6142, #6143) |
| test-heavy files >2000 total LOC | 93 | 93 | none escalates |
| blocking clippy gate | green | green | |
| `cargo machete` | clean | clean | |

New production functions over 200 lines are all extraction products of the #6143 splits. `build_static_draw_command` (631) is pinned at 640 in `scheduler_access_tests.rs`. The others are `emit_bench_summary` 292, `refresh_frame_telemetry` 211, `upload_frame_parameters` 283, `import_embedded_controller` 300 and `advance_actor_synth` 209. `spawn_lod_block` grew 334 → 404; it belongs to the exterior audit's CRITICAL (EXT-D6-01).

## Baseline Snapshot (Phase 1, measured at fa6be4a6b)

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 26
allow(dead_code):      39
unimplemented!/todo!(): 0
#[ignore] tests:        373
files >2000 production LOC: 2
test files >2000 total LOC (lower priority, separate bucket): 93
```
`prod_loc_self_test`: ok. Watch list (prod LOC 10-25 → 10-27):

| File | 10-25 | 10-27 | Note |
|---|---:|---:|---|
| `byroredux/src/inventory.rs` | 1956 | 1944 | #6356 |
| `crates/debug-ui/src/panels.rs` | 1938 | 1938 | |
| `crates/renderer/src/vulkan/gpu_timers.rs` | 1929 | 1929 | #6358 |
| `byroredux/src/npc_spawn.rs` | 1911 | 1901 | #6357 |
| `crates/plugin/src/esm/records/misc/world.rs` | 1912 | 1870 | |
| `crates/renderer/src/mesh.rs` | 1687 | **1864** | TD1-01 |
| `byroredux/src/asset_provider/material/merge.rs` | 1860 | 1860 | |
| `crates/scripting/src/obscript_effects.rs` | 2034 | 1842 | |
| `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` | 1647 | **1818** | #6250 + #6143 extraction; REN-D2-01 / PERF-D4-01 own its logic |

## Top Quick Wins (trivial / small)

1. **TD3-02** (trivial): rewrite memory-budget.md's admission-tier paragraph from mesh.rs's `GeometryAdmissionClass` docs, and fix the test doc and the TSD-4 cap figures.
2. **TD8-01** (trivial): either wire `actor_geometry_admission_open` into NPC spawn or drop the accessor. Do it together with SPT-D4-01.
3. **TD3-01** (small): name the symbol at the 10 dead-file line refs, and make `stale_line_references.rs` fail on a zero-match basename.
4. **TD4-01** (small): teach the #6383 closed-issue advisory the "fixed …: #A, #B" list idiom and per-clause cues. Today it is 11/11 noise.
5. **TD2-02** (small): add one `production_scan_roots()` helper covering `tools/` and route the five hygiene scans through it.
6. **TD2-01** (small, alongside SAFE-01): build `destroy` from one array of per-frame families, or add a field-coverage scan.
7. **TD7-01** (small): name the god-ray step and tap budgets in `shader_constants_data.rs` and fold #6418's IOR into the same pass.

## Top Medium Investments

1. **TD1-01**: split `crates/renderer/src/mesh.rs` into `mesh/pool_limits.rs` and `mesh/primitives.rs` before the next geometry-pool feature crosses 2000.
2. Carry-overs still open from 10-25: #6358 (gpu_timers table), #6359 (registration hubs), #6371 (compute-pass teardown, the sibling of TD2-01) and #6223 (WTHR decoders; partial ac9fa19a8).

## Findings

### LOW — Dim 1: Complexity

#### TD1-2026-10-27-01: `crates/renderer/src/mesh.rs` grew 1687 → 1864 prod LOC (+177) in four geometry-pool commits over two days
- **Severity**: LOW · **Dimension**: 1 · **Effort**: small · **Status**: NEW
- **Location**: `crates/renderer/src/mesh.rs` (pool-limit block ~:20-280, primitives :1528-1851)
- **Age**: 3929a406c, ab9d46d18 (10-26), bb97be77c, 22d6e1a80 (10-27). These raised the pool caps 5M → 7M, added reserve headroom and added the three admission tiers.
- **Description**: The file is now 136 lines from the threshold. The growth sits in two blocks that have nothing to do with `MeshRegistry`'s upload and residency job:
  - **Pool-limit and admission-tier policy**: `VERTEX_POOL_*`/`INDEX_POOL_*`, `DEFAULT_GROUND/ACTOR_RESERVE_PERCENT`, `GeometryAdmissionClass`, `PoolReserve`, `PoolLimits`, `check_pool_growth`, `geometry_rebuild_needs_idle`.
  - **Procedural primitive generators**: `cube_vertices`, `triangle_vertices`, `quad_vertices`, `box_vertices_colored`, `uv_sphere`, `fullscreen_quad_ui_vertices`. This is pure CPU data, about 320 lines.
- **Suggested Fix**: Move the first block to `mesh/pool_limits.rs` and the second to `mesh/primitives.rs`, re-exported from `mesh.rs`. The `mesh/` directory already holds `geometry_ssbo`, `geometry_residency` and `geometry_sharing`. That leaves `MeshRegistry` at about 1250. Move the matching `pool_growth_cap_tests` / `primitive_winding_tests` modules with their code.

### LOW — Dim 2: Duplication

#### TD2-2026-10-27-01: SceneBuffers per-frame buffer families are spelled at six sites with no coverage check; #6250 missed the destroy site
- **Severity**: LOW · **Dimension**: 2 · **Effort**: small · **Status**: NEW (structural root of SAFE-2026-10-27-01; sibling of open #6371)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/buffers.rs:39-237` (fields), `:486-516` (bundle struct), `:575`/`:663` (per-family alloc loop), `:735` (bundle literal), `:1059-1084` (move into `Self`); `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:361-426` (`destroy`)
- **Description**: Every `Vec<GpuBuffer>` family must be written at six places. `destroy` tears them down with eleven hand-unrolled `for buf in &mut self.X { buf.destroy(..) } self.X.clear();` loops. #6250 added `moved_surface_buffers` at five of the six sites and missed `destroy`. That miss is SAFE-01's allocator-`Arc` leak, which fails the teardown `unwrap` and has kept the lavapipe lane red since c7463c04d. Nothing checks that each field reaches `destroy`.
- **Suggested Fix**: Fix it with SAFE-01. Either give `SceneBuffers` a `fn per_frame_families_mut(&mut self) -> [&mut Vec<GpuBuffer>; N]` that `destroy` iterates, so a new family becomes a compile-visible edit, or add a source-scan test that every `Vec<GpuBuffer>` field named in buffers.rs appears in `destroy`.

#### TD2-2026-10-27-02: Five workspace-hygiene scans re-spell the "bin src + every crates/*" root walk (four added this window); none covers `tools/`
- **Severity**: LOW · **Dimension**: 2 · **Effort**: small · **Status**: NEW
- **Location**: `byroredux/src/workspace_hygiene_tests.rs:988-993` (#6351), `:1057-1061` (#6366), `:1114-1117` (#6373), `:1172-1176` (#6372), `:1500-1506` (zup)
- **Description**: Each test builds its own roots with a private `read_dir(manifest.join("../crates"))` loop, and the loops disagree:
  - **Roots**: some push `<crate>/src` and some push the whole crate dir. Some check `is_dir()` and some do not.
  - **Test-file skip**: most re-spell the `_tests.rs` / `tests.rs` / `tests/` skip.
  - **Coverage**: only the scan at `:524-530` walks `../tools`. The spliced-doc, `init_resource`, pool-to-local and `weapon_stub` guards therefore never see byro-dbg, byro-launcher, byro-detect or texture-upscale, although #6373's own message says it covers "the bin and every crate".
- **Suggested Fix**: Add one `production_scan_roots()` (bin `src` + `crates/*/src` + `tools/*/src`) and a test-file-skipping visitor beside the existing `hygiene_scan_roots()` / `visit_workspace_rs_files`, and route the five scans through it.

### LOW — Dim 3: Stale documentation

#### TD3-2026-10-27-01: 10 `file.rs:LINE` references name in-tree `.rs` files that splits deleted; #6167's gate skips them
- **Severity**: LOW · **Dimension**: 3 · **Effort**: small · **Status**: NEW (blind spot of closed #6167's guard)
- **Location**: guard `crates/core/tests/stale_line_references.rs:93`. Sites:
  - **`cell_loader/references.rs` → now `references/`**: `byroredux/src/cell_loader/transition.rs:678`, `byroredux/src/cell_loader/spawn.rs:1151`, `byroredux/src/scene/nif_loader.rs:378`, `byroredux/src/asset_provider/tests/material_path.rs:67`, `docs/engine/m47-0-design.md:26`
  - **nif `blocks/tri_shape.rs` → now `tri_shape/`**: `crates/nif/src/blocks/particle.rs:1110`, `particle_tests.rs:462`, `tri_shape_skin_vertex_tests.rs:1212`
  - **renderer `scene_buffer.rs` → now `scene_buffer/`**: `crates/renderer/src/mesh.rs:28`, `crates/nif/src/import/material/shader_data.rs:123`
- **Description**: The gate does `let Some([target]) = sources.get(base)… else { continue }`, so a basename that resolves to no file is skipped. A reference into a file that a split deleted is the case where the line number now means nothing, and it is exactly the one the gate cannot see.
  - **Excluded as legitimate**: five rapier upstream `control/character_controller.rs` refs (in `systems/character.rs`, `locomotion.rs` and physics `world/queries.rs`), the upstream paths in `dependency-update-followups-plan.md`, and the test's own fixture literals.
- **Suggested Fix**: Replace the 10 line numbers with symbol + module. Then make the gate fail on a zero-match basename unless the reference has an upstream-crate prefix or sits on a short allowlist.

#### TD3-2026-10-27-02: The geometry-pool tier rewrite left the reference docs describing the deleted two-tier scheme
- **Severity**: LOW · **Dimension**: 3 · **Effort**: trivial · **Status**: NEW
- **Location**: `docs/engine/memory-budget.md:897-910`; `crates/renderer/src/mesh.rs:2324` (test doc); `docs/engine/terrain-displacement.md:36,139`
- **Age**: 22d6e1a80 (admission tiers, 10-27), bb97be77c (7M / 28M caps, 10-27)
- **Description**:
  - **memory-budget.md** describes the deleted scheme: "`GeometryAdmissionClass`, `PROP_POOL_RESERVE_PERCENT` = 5", props at 95 %, and "terrain, water, terrain LOD **and actors** are essential … may use the full cap".
  - **The code** (`mesh.rs:37-127`, `:997-1031`) now has:
    - three tiers: Essential, Actor and Prop;
    - `DEFAULT_GROUND_RESERVE_PERCENT` = 3 and `DEFAULT_ACTOR_RESERVE_PERCENT` = 2;
    - actors capped below the ground reserve;
    - a third latch, `actor_geometry_admission_open`;
    - a reserve that is re-sized per exterior radius through `MeshRegistry::set_pool_reserve`.
  - **`PROP_POOL_RESERVE_PERCENT` no longer exists.** Its only surviving mention is the `mesh.rs:2324` test doc. That mention keeps the name in the validator's symbol corpus, which is why `_audit-validate.sh` does not flag memory-budget.md (a self-satisfying corpus).
  - **terrain-displacement.md** still says "cap 4 M verts = 416 MB, 16 M indices" and "2.5 % of the vertex pool". The 7 M cap makes that about 1.4 %.
  - memory-budget.md is one of the authoritative docs audits are told to trust.
- **Related**: SPT-2026-10-27-D4-01 (code defect in the same tier work), TD8-01.
- **Suggested Fix**: Rewrite the tier paragraph from `GeometryAdmissionClass`'s rustdoc, update the test doc, and refresh the TSD-4 figures.

### LOW — Dim 4: Audit-finding rot

#### TD4-2026-10-27-01: The #6383 closed-issue advisory fires 11 times at HEAD, and all 11 are false positives
- **Severity**: LOW · **Dimension**: 4 · **Effort**: small · **Status**: NEW (follow-on to #6383, 1f6594b18)
- **Location**: `.claude/commands/_audit-validate.sh:741-781`
- **Description**: The advisory lists 11 hits: `audit-esm:84` #5492, `audit-exterior:90` #5520, `audit-fo3:29` #5429, `audit-fo4:91` #3638, `audit-gameplay:17` #5367, `audit-oblivion:28` #6258, `audit-performance:26` #5128 and `:81` #5447, `audit-physics:24` #5538, `audit-scripting:100` #6365, `audit-ui:46` #5967. Every one is a correct past-tense citation, for example "all since fixed, #5493 `136048a95` / #5492 `dae08000d`" and "Fixed since 2026-10-08 — verify, don't re-file: #6098 (…), #6258 (…)". Four separate causes:
  1. **List items are missed**: the `resolved` look-back is `[^#]{0,80}$`, so it stops at the previous `#`. The second and later numbers in a "fixed …: #A (…), #B (…)" list are never treated as resolved.
  2. **"don't re-file" is a cue**: the house idiom "fixed — verify, don't re-file" marks a closed fix, but only "regression guard" is exempted.
  3. **The 160-char window spans clauses**: it catches "stays open" about a different subject in the same sentence (`exterior:90`, `physics:24`).
  4. **"closed" after the number is not read**: e.g. `gameplay:17` "#5367 (…) is closed".
- **Impact**: A day-one advisory that is 100 % noise teaches auditors to scroll past it, which is the failure `_audit-common.md` warns against. The next real stale "known open" (the class tracked in #6353, #6173, #5742, #5606 and #4369) will be buried in it.
- **Suggested Fix**: Treat fixed/closed/resolved anywhere earlier in the same sentence as resolving every `#N` that follows. Drop "don't re-file" as a cue when the sentence also says fixed or closed. Evaluate "open" cues per clause (split on `;`, `.` and `—`). Add a self-test over these 11 lines plus two true positives.

### LOW — Dim 7: Magic numbers

#### TD7-2026-10-27-01: The underwater god-ray march keeps its loop bounds and ~15 tuning literals inline
- **Severity**: LOW · **Dimension**: 7 · **Effort**: small · **Status**: NEW (siblings filed: #6418 IOR `1.333`, PERF-2026-10-27-D5-01 cost, REN-D8-01/-02)
- **Location**: `crates/renderer/shaders/composite.frag:522-606` (`underwaterSurfaceFocus`, `underwaterSunVisibility`, `underwaterGodRays`)
- **Age**: 067ce38ef, 296d21912, 99475136b (10-26)
- **Description**: This is the one new shader block that this window's #6390–#6392 constant sweep did not reach. Those commits moved the Fresnel, parallax-pass and radial-blur-tap ranges into `shader_constants_data.rs` and pinned them against inline literals. The god-ray block bypasses that file entirely.
  - **Budget literals**: `const int steps = 24` (the march) and `for (int i = 0; i < 6; ++i)` (occluder taps). Together they are PERF-D5-01's 144 depth taps per submerged pixel, and neither has a name.
  - **Tuning literals**:
    - Henyey-Greenstein `g = 0.6`;
    - reach `900.0`;
    - occluder slab `6.0 + 0.01*w` / `160.0`;
    - tap fractions 0.15 / 0.4 / 0.75 and 60 / 180 / 420 BU;
    - noise scales `/70`, `/37`;
    - four drift speeds;
    - `pow(ridge, 6.0) * 2.4`;
    - `span * 1.5`;
    - `k = 2.0 / span`;
    - sun-elevation gates `0.05`.
  - **Impact**: a perf retune of the step count, or a future CPU mirror of the shaft occlusion, has no single home to change.
- **Suggested Fix**:
  - Add these names to `shader_constants_data.rs`: `UNDERWATER_GODRAY_STEPS`, `UNDERWATER_GODRAY_OCCLUDER_TAPS`, `UNDERWATER_GODRAY_PHASE_G` and the reach/slab bounds.
  - Keep the noise-shaping numbers as documented locals.
  - Fold #6418's IOR into the same change.

### LOW — Dim 8: Dead code

#### TD8-2026-10-27-01: `MeshRegistry::actor_geometry_admission_open` (22d6e1a80) has no production reader
- **Severity**: LOW · **Dimension**: 8 · **Effort**: trivial · **Status**: NEW
- **Location**: `crates/renderer/src/mesh.rs:997-1001`
- **Description**: The two sibling latches both have readers:
  - `prop_geometry_admission_open` gates the tree, placement and object LOD pre-checks and the LOD re-arm edge (`byroredux/src/app_step.rs:143-146`).
  - `scene_geometry_admission_open` gates terrain LOD.

  The actor latch is read only by tests. NPC and player spawn (`spawn_nif_mesh`, `byroredux/src/scene/nif_loader.rs:1226-1237`) attempts the upload and lets it fail. Nothing re-arms on a closed actor tier or reports it. This was found by a sweep of the 130 `pub fn`s added this window; the other five with only test callers are documented test or tooling helpers kept public on purpose (#6398's decision, #6372 fixtures).
- **Related**: SPT-2026-10-27-D4-01, TD3-02.
- **Suggested Fix**: Either add an NPC-spawn pre-check plus a reopen log edge, matching the LOD one, or remove the public accessor until it has a consumer.

## Dimensions with no new findings

- **Dim 5 (markers)**: 26 hits, the same composition as 10-25. The added-line grep over the window is empty. The two real TODOs in `byroredux/src/loading_screen.rs` cite #6194, which is still open.
- **Dim 6 (stubs)**: 0 `unimplemented!`/`todo!()`. The one `panic!("not ` hit is test-only (`crates/scripting/src/fragment/vm/tests.rs:4951`). The production-unreachable sweep is covered under TD8-01.
- **Dim 9 (tests)**:
  - **Ignores and lanes**: zero ignore attributes added or removed. No feature or CI-lane changes. The pins file grew by 11 rows via #6354 and its guard is green. The window's only closed-HIGH, #6406, is pinned by plain tests.
  - **New tests**: none is assertion-free. Seven new `include_str!` scans read GLSL (no test module to leak). The eighth is NIF-D4-02, already filed.
  - **Still open**: #6402, #6403.

## Deferred

- The TD1-01 split is not gated on anything, but it should land after SAFE-01, REN-D2-01 and SPT-D4-01, which all touch the same pool and upload paths. That avoids rebasing a move over three live fixes.

---
Suggested next step: `/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-10-27.md`
