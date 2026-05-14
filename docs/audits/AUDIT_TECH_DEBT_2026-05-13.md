# Tech-Debt Audit — ByroRedux

**Date:** 2026-05-13
**Repo HEAD:** `e3cac0f` (branch `main`)
**Scope:** All 10 dimensions, deep depth
**Auditor:** `/audit-tech-debt` orchestrator + 10 dimension agents

This is the first dedicated tech-debt audit run on ByroRedux. No prior `AUDIT_TECH_DEBT_*` reports exist; no open issues carry the `tech-debt` label. Therefore every finding is **NEW** unless explicitly cross-referenced to a closed GitHub issue.

---

## Executive Summary

| Severity | Count | Comment |
|----------|------:|---------|
| **HIGH** | 1     | TD4-002 — duplicated `MAX_FRAMES_IN_FLIGHT` constant with documented bump path |
| **MEDIUM** | 19  | Shader↔Rust constant drift (5), publish-but-don't-consume parsers (4), CLAUDE.md doc rot (5), audit-skill rot (3), logic duplication (4) |
| **LOW**  | 100+ | Hygiene findings across all 10 dimensions |
| **INFO** | 3   | Already-fixed historical patterns or verification-only |
| **Total** | ~132 | Across 10 dimensions |

**Dominant themes:**

1. **Shader ↔ Rust constant drift.** 5 MEDIUM findings (TD4-002 through TD4-006) all share the same root cause: GLSL `#define`/`const uint` literals that must stay in lockstep with Rust constants, with the only safety net being convention. The right architectural fix is a `build.rs` step that emits a generated `shader_constants.glsl` from a single Rust source-of-truth. **TD4-002 is HIGH** because the documented "bump `MAX_FRAMES_IN_FLIGHT` to 3" upgrade path will silently produce a use-after-free on freed texture descriptors.

2. **Doc rot post-Session-34.** CLAUDE.md, `_audit-common.md`, `audit-renderer.md`, `audit-safety.md`, and the `feedback_shader_struct_sync.md` memory note all carry pre-Session-34 (or pre-R1, or pre-#992) anchors. 3 audit reports in the last 10 days (`AUDIT_RENDERER_2026-05-11_DIM4`, `AUDIT_SAFETY_2026-05-03`, `AUDIT_RENDERER_2026-05-07`) quoted stale baselines verbatim. TD10-009's recommendation — bare line numbers → symbol-based anchors — would prevent ~70 % of this class.

3. **Publish-but-don't-consume parser boundaries.** NIF importer captures `StencilState`, BSSky/Water shader flags, BSShaderTextureSet slot-6 inner texture; ESM parser captures IMGS/ACTI/TERM raw payloads. None reach the renderer/runtime today. Healthy debt (silent drop moved from parser to consumer boundary), but every MEDIUM here is a `grep` target waiting for the consumer.

**Delta vs baseline** (`/tmp/audit/tech-debt/baseline.txt`):
- TODO/FIXME/HACK/XXX: 4 → expected to drop to 2 after Dim 1 fixes (2 doc-comment hits aren't actionable markers).
- `#[allow(dead_code)]`: 42 → target ~29 after Dim 2 cleanup (~13 mutes to delete or `#[cfg(test)]`).
- `unimplemented!()`/`todo!()`: 1 (only a doc-comment mention; codebase enforces "no live `todo!()`" well).
- `#[ignore]` tests: 104 raw grep hits → 73 actual `#[test] #[ignore]` annotations (the rest are doc-comment mentions). Every justified — no stale-by-closed-issue.
- Files > 2000 LOC: 9 — 7 splittable proposals; 2 test files skipped.

---

## Baseline Snapshot

So the next audit can diff:

```
TODO/FIXME/HACK/XXX:      4
allow(dead_code):         42
unimplemented!/todo!():   1   (doc-comment only, not live)
#[ignore] tests:          104 raw grep / 73 actual annotations
files >2000 LOC:          9

Top 9 oversized files:
   4200 crates/renderer/src/vulkan/acceleration.rs
   3667 crates/nif/src/blocks/dispatch_tests.rs    (test scaffolding, skipped)
   3329 crates/plugin/src/esm/cell/tests.rs        (test scaffolding, skipped)
   2554 crates/renderer/src/vulkan/context/draw.rs
   2367 crates/renderer/src/vulkan/scene_buffer.rs
   2348 crates/renderer/src/vulkan/context/mod.rs
   2212 crates/nif/src/import/mesh.rs
   2162 crates/nif/src/blocks/collision.rs
   2101 crates/nif/src/anim.rs
```

---

## Top 10 Quick Wins

Trivial / small effort, immediate readability or correctness payoff:

| # | Finding | Effort | Why now |
|---|---------|--------|---------|
| 1 | **TD4-002** — delete duplicate `MAX_FRAMES_IN_FLIGHT` in `texture_registry.rs:28`, import from `sync.rs` | trivial | **HIGH severity** — silent use-after-free on `MAX_FRAMES_IN_FLIGHT` bump |
| 2 | **TD7-017** — fix `volumetric_inject.comp` → `volumetrics_inject.comp` (and `_integrate`, `bloom_down`, `bloom_up`) in `_audit-common.md:51` and `audit-renderer.md:303,305` | trivial | Audit grep against these paths hard-fails today |
| 3 | **TD7-018** — delete `effect_lit.frag` from `_audit-common.md:51` (file doesn't exist) | trivial | Same audit-grep hard-fail |
| 4 | **TD7-019** + **TD7-001** — drop deleted `legacy/ TES3-FO4 stubs` from `_audit-common.md:52` and `CLAUDE.md:73-77` | trivial | Submodules were deleted under #390; docs lie |
| 5 | **TD10-001** — `audit-renderer.md:79` says R16_UINT/32767 cap; actual is R32_UINT/0x7FFFFFFF (#992) | trivial | Leaked into `AUDIT_RENDERER_2026-05-11_DIM4` verbatim |
| 6 | **TD10-003** — `audit-safety.md:68` points at `scene_buffer.rs:~975`; actual line is 1270 | trivial | Leaked into `AUDIT_SAFETY_2026-05-03` and `2026-05-05` |
| 7 | **TD7-020 / TD7-023 / TD10-002** — bump "3 shaders in lockstep" to "6 shaders" (`triangle.vert/frag`, `ui.vert`, `water.vert`, `caustic_splat.comp`, `skin_vertices.comp`) in `audit-renderer.md:241,252`, `audit-safety.md:69`, and the `feedback_shader_struct_sync.md` memory note | trivial | Half the lockstep surface area is currently invisible to audits |
| 8 | **TD1-001 / TD1-002** — delete two `TODO: thread StagingPool through ... (#242)` markers in `byroredux/src/scene.rs:477` and `byroredux/src/main.rs:1144`; #242 closed | trivial | Stale markers point at closed issue |
| 9 | **TD2-001 / TD2-002 / TD2-003 / TD8-004** — delete 4 stale `#[allow(dead_code)]` annotations where the symbol IS read (`unload_cell`, `OneCellLoadInfo.cell_root`, `CellLoadResult`, `embedded_clip`) | trivial | Lints lie + hide future real warnings |
| 10 | **TD2-008** — delete `fn _uses_ni_texturing_property() { panic!() }` in `texture_slot_3_4_5_tests.rs:521`; replace with `#[allow(unused_imports)]` on the anchored `use` | trivial | Directly violates CLAUDE.md "delete completely" rule |

Net effect of all 10: ~10 lines deleted, 4 audit reports stop drifting, 1 HIGH-severity latent UAF closed.

---

## Top 5 Medium Investments

File/function splits, duplication consolidations. Each lists effort + payoff.

| # | Investment | Effort | Findings consolidated |
|---|------------|--------|------------------------|
| 1 | **`build.rs`-generated `shader_constants.glsl`** — single source of truth for shader↔Rust constants (`MAX_FRAMES_IN_FLIGHT`, `MAX_LIGHTS_PER_CLUSTER`, `MAX_BONES_PER_MESH`, `VERTEX_STRIDE_FLOATS`, `CAUSTIC_FIXED_SCALE`, `GLASS_RAY_BUDGET`, cluster grid). Eliminates the entire shader-Rust drift class. | medium (~1 day) | TD4-003, TD4-004, TD4-005, TD4-006, TD4-013, TD4-015 |
| 2 | **Flesh out `crates/renderer/src/vulkan/descriptors.rs`** with `write_image_sampler`, `write_storage_buffer`, `image_barrier_undef_to_general`, and a `DescriptorPoolBuilder`. The file is currently a 5-line placeholder; 113 inline builder chains across `composite.rs`/`svgf.rs`/`scene_buffer.rs`/`caustic.rs`/`bloom.rs` are waiting. | medium | TD3-008, TD3-009, TD3-014 |
| 3 | **Generic `KeyGroup<K: KeyValue>`** in `crates/nif/src/blocks/interpolator.rs` — collapses 4 near-identical scaffolds (`FloatKey`/`Vec3Key`/`Color4Key` + quaternion path in `NiTransformData`) into one. `#230` already had to fix the quat path separately because of this; the next equivalent fix will need 4 sites again. | medium | TD3-001 |
| 4 | **`CommonNamedFields::from_subs` helper** + `read_lstring_sub` in `crates/plugin/src/esm/records/common.rs` — every record parser should funnel `EDID`/`FULL`/`MODL`/`ICON`/`SCRI`/`VMAD` through one place. `#816` had to fix SCOL's missing FULL handling specifically because there was no shared site; the next record-parser without lstring routing is one #348 bug away. | small–medium | TD3-006, TD3-007 |
| 5 | **Split `acceleration.rs` (4200 LOC)** into `mod.rs` façade + `blas.rs` + `skinned_blas.rs` + `tlas.rs` + `scratch.rs` + `telemetry.rs` + `tests.rs`. Already has section banners aligned with proposed module boundaries. Largest single file in the renderer; review burden is the actual cost. | medium | TD9-001, TD9-011, TD9-012 |

A sixth deferred-but-worth-mentioning: **anim.rs Z-up→Y-up helper** (TD3-002). One-line fix, but a real divergent-bug risk against `#333`'s normalize fix.

---

## Findings by Severity

The full per-dimension reports live at `/tmp/audit/tech-debt/dim_<N>.md` during the audit and are summarised below. Cross-dimension duplicates are noted with `(see also)` rather than re-listed.

### HIGH (1)

#### TD4-002 — `MAX_FRAMES_IN_FLIGHT` defined in two places
- **Severity**: HIGH
- **Dimension**: Magic Numbers
- **Location**: `crates/renderer/src/vulkan/sync.rs:6` (canonical) and `crates/renderer/src/texture_registry.rs:28` (private duplicate)
- **Status**: NEW
- **Effort**: trivial
- **Description**: Both read in the same descriptor-update / fence-wait code paths. The codebase already anticipates bumping `sync.rs`'s value to 3 (warning at `acceleration.rs:3048`). If bumped without updating the private copy, freed texture descriptors will be reused while a frame is still in flight — a use-after-free with no `cargo test` coverage.
- **Suggested Fix**: Delete the `texture_registry.rs` copy, `use super::vulkan::sync::MAX_FRAMES_IN_FLIGHT;`.

### MEDIUM (19)

Grouped by dimension. Each entry: ID — short title — location — effort.

**Logic Duplication (Dim 3) — 4**
- **TD3-001** — `KeyGroup<K>::parse` triplicated keyframe scaffold + 4th hand-rolled quat path — `crates/nif/src/blocks/interpolator.rs:92-251, 308-329` — medium. Divergent-fix evidence: `#230`, `#408`, `#8353092`.
- **TD3-002** — `crates/nif/src/anim.rs:42-56` carries its own `zup_to_yup_quat` that missed `#333`'s Shepperd-normalize fix — small. Animation rotation keys silently propagate shear that the static-mesh path fixed last month.
- **TD3-006** — ESM record parsers re-roll EDID/FULL/MODL/ICON/SCRI/VMAD match arms across 10+ record files; `CommonItemFields::from_subs` exists but is only consumed by `items.rs` — small–medium. Divergent: `#816` had to fix SCOL FULL specifically.
- **TD3-008** — 113 inline `vk::WriteDescriptorSet` + `vk::ImageMemoryBarrier` builder chains; `descriptors.rs` is a 5-line placeholder. — medium. See Top 5 #2.

**Magic Numbers (Dim 4) — 5 (shader↔Rust drift)**
- **TD4-001** — 113 bare `NifVersion(0x...)` literals; only 6 named constants; existing `V10_1_0_0` has 23 call sites that all spell the hex longhand instead — small, mechanical sweep.
- **TD4-003** — `TILES_X=16, TILES_Y=9, SLICES_Z=24, NEAR=0.1, FAR_*` cluster constants duplicated across `compute.rs`, `cluster_cull.comp`, `triangle.frag`.
- **TD4-004** — `MAX_LIGHTS_PER_CLUSTER = 32` Rust ↔ shader.
- **TD4-005** — `MAX_BONES_PER_MESH = 128` Rust ↔ `triangle.vert` ↔ `skin_vertices.comp`. Already documented as lockstep risk; bumping past 128 (planned for #29.5) without all three sites = silent visual corruption.
- **TD4-006** — `VERTEX_STRIDE_FLOATS = 25` Rust ↔ `triangle.frag` ↔ `skin_vertices.comp`. `#783` confirms a real 21→25 drift happened; `VERTEX_UV_OFFSET_FLOATS = 9` has no Rust mirror at all.

**Stub Implementations (Dim 5) — 4 (publish-but-don't-consume)**
- **TD5-001** — SpeedTree importer reachable via `--tree` CLI flag returns a single placeholder billboard, never real geometry. SpeedTree Phase 1.6 contracted, but `--tree` CLI advertises something it doesn't deliver. — large.
- **TD5-002** — `StencilState` parsed (7 sub-fields) into `MaterialInfo`; renderer hardcodes `stencil_test_enable(false)` everywhere. (`#337`.) — medium.
- **TD5-003** — `BSSkyShaderProperty` / `BSWaterShaderProperty` flags captured into `MaterialInfo`; zero renderer consumers (`grep -RnE 'is_sky_object|water_shader_flags' crates/renderer/` → 0 hits). (`#977`.) — medium.
- **TD5-006** — M55 volumetrics is a clear-only skeleton: 3D froxel images cleared each frame, composite shader multiplies by 0.0. — large, planned.

**Stale Documentation (Dim 7) — 5 (CLAUDE.md as worst offender)**
- **TD7-001** — `CLAUDE.md:72-77` lists deleted `legacy/{tes3,tes4,tes5,fo4}.rs` submodules (gone post-`#390`). See also TD7-019 in `_audit-common.md:52`.
- **TD7-003** — `CLAUDE.md:98` says `Vertex (position + color + normal + UV), 4 attribute descriptions`. Actual: 8 fields, 9 attribute descriptions, 100 B / 25 floats.
- **TD7-004** — `CLAUDE.md:317-318` `Next:` lists M33 and M29 as upcoming; both `~~Closed~~` per ROADMAP.
- **TD7-006** — `CLAUDE.md:217` `1000+ tests / 14 crates / 200 files / ~94K LOC`. Actual: 1979 / 17 / 385 / ~170K. Every number wrong by ≥45%.
- **TD7-008** — `CLAUDE.md:301-302` claims `192-byte GpuInstance`. Actual `size_of::<GpuInstance>() == 112` (post R1 Phase 6, pinned by test).
- **TD7-012** — `README.md:10-13, 64-66` highlighted FPS numbers are from `6a6950a` (172.6 / 253.3 / 92.5 FPS); ROADMAP refreshed to `220e8e1` (133.5 / 217.3 / 68.5 FPS) on 2026-05-11.

**Audit-Skill Manifest Rot (Dim 7 + Dim 10) — 5**
- **TD7-017** — `_audit-common.md:51` and `audit-renderer.md:303,305` carry shader names `volumetric_inject.comp` (actual `volumetrics_inject.comp`), `volumetric_integrate.comp` (actual `volumetrics_integrate.comp`), `bloom_down.comp` (actual `bloom_downsample.comp`), `bloom_up.comp` (actual `bloom_upsample.comp`).
- **TD7-018** — `_audit-common.md:51` lists non-existent `effect_lit.frag`.
- **TD7-020** — `audit-renderer.md:241,252` says GpuInstance lives in "3 shaders". TD10-002 sharpens this further: actual count is **6** (`triangle.vert/frag` + `ui.vert` + `water.vert` + `caustic_splat.comp` + `skin_vertices.comp`). The "3 in lockstep" mantra under-covers half the audit surface.
- **TD7-023** — The `~/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_shader_struct_sync.md` memory note carries the same wrong "3 shaders" count. This is what fix-issue / audit-renderer reads FIRST when GpuInstance changes.
- **TD10-001** — `audit-renderer.md:79` Dim 4 checklist says mesh ID is R16_UINT / 32767 instance cap; actual since `#992` is R32_UINT / 0x7FFFFFFF. Quoted verbatim in `AUDIT_RENDERER_2026-05-11_DIM4.md:61`.
- **TD10-003** — `audit-safety.md:68` MaterialTable upload at `scene_buffer.rs:~975`; actual line is **1270**. Quoted in `AUDIT_SAFETY_2026-05-03.md` and `2026-05-05.md`.
- **TD10-005** — `audit-nif.md:42`, `audit-renderer.md:277`, `audit-fo4.md:33` cite `tri_shape.rs:695` for FO4 inline tangent decode; actual span is **665–730**. Cited verbatim in `AUDIT_RENDERER_2026-05-07.md:152`.

### LOW (100+)

Listed by dimension, abbreviated for the index. Full text per finding lives in `/tmp/audit/tech-debt/dim_<N>.md` (re-attached for posterity below).

**Stale Markers (Dim 1) — 2**
- TD1-001 / TD1-002 — Two `TODO: thread StagingPool through ... (#242)` markers; issue #242 closed. (See also TD5-005.)

**Dead Code (Dim 2) — 16**
- Stale mutes (consumer landed): TD2-001 `unload_cell`, TD2-002 `OneCellLoadInfo.cell_root`, TD2-003 `CellLoadResult` struct-level (see also TD8-004).
- Should be `#[cfg(test)]` not `#[allow(dead_code)]`: TD2-004 `scene_import_cache` 4 accessors, TD2-005 `parsed_nif_cache::is_empty`/`clear_entries`, TD2-006 `DeferredDestroyQueue::is_empty`.
- Truly unused renderer telemetry pub fns: TD2-007 `Volumetrics::integrated_view`, TD2-009 `SceneBuffers::instance_buffer_mapped_mut`, TD2-010 five `AccelerationManager` `*_bytes/_telemetry` getters (commit `3314ee08` shipped 4 getters without consumers — pattern flag).
- Policy violation: TD2-008 `fn _uses_ni_texturing_property() { panic!() }`.
- Honest mutes worth re-gating: TD2-011 `bsa::archive::genhash_folder/file` (debug-only, should be `cfg(any(debug_assertions, test))`).
- Acceptable / future-tracked: TD2-012 `RefrTextureOverlay::inner`, TD2-013 staged-rollout XCLL fields + RawDependency, TD2-014 MSWP `peek_path_filter`, TD2-015 NIF schema-completeness mutes, TD2-016 BA2 `Dx10Chunk` `start_mip/end_mip` (LOD streaming deferred).

**Logic Duplication (Dim 3) — 8 LOW + 2 INFO**
- TD3-003 — `byroredux/src/cell_loader/euler.rs` is a third coord-flip helper.
- TD3-004 — `[x, z, -y]_zup→yup` literal inlined in `spt/src/import/mod.rs:167,453` and `byroredux/src/cell_loader/exterior.rs:324`.
- TD3-005 — 117× `impl NiObject for X { block_type_name + as_any }` boilerplate (~585 LOC); declarative macro proposed.
- TD3-007 — `read_lstring_sub` helper missing; `tree.rs` and `pkin.rs` open-code the pattern.
- TD3-009 — Descriptor-pool creation + size-array boilerplate per pipeline.
- TD3-010 — `from_rgba` and `record_dds_upload` repeat staging+image+2× barrier dance.
- TD3-011 — Exterior-grid origin formula `gx * 4096` repeated 4+ times.
- TD3-012 — Particle-modifier per-block trailers hand-roll past `parse_particle_modifier_base`.
- TD3-013 — INFO — `allocate_vec` sweep (`#408`) already landed; no helper prevents recurrence.
- TD3-014 — INFO — `GraphicsPipelineBuilder` missing; per-pipeline `vk::GraphicsPipelineCreateInfo` chain.

**Magic Numbers (Dim 4) — 14**
- TD4-007 — `MATERIAL_KIND_GLASS=100` / `MATERIAL_KIND_EFFECT_SHADER=101` Rust↔shader.
- TD4-008 — BSA/BA2 version allowlists (`version == 103/104/105`, `match version { 1|7|8 => ... }`).
- TD4-009 — 106 bare `bsver` literal compares; `NifVariant` predicates exist for some but not the BSVER threshold form.
- TD4-010 — 107 `data.len() >= N` / `== N` subrecord checks; `scol.rs:57` already has the right pattern (`pub const WIRE_SIZE: usize = 28;`).
- TD4-011 — Allocator block sizes are bare `64 * 1024 * 1024` etc.; duplicate `2 * 1024 * 1024 * 1024` at two sites that must agree.
- TD4-012 — Two function-local `const SLACK: vk::DeviceSize = ...;` with different values in `acceleration.rs:453,475`.
- TD4-013 — `GLASS_RAY_BUDGET = 8192u` lives only in shader; `scene_buffer.rs` doc references it but doesn't own.
- TD4-014 — `deferred_destroy.rs:30` countdown is `2` literal instead of `MAX_FRAMES_IN_FLIGHT as u32`; silent lag on bump.
- TD4-015 — `CAUSTIC_FIXED_SCALE = 65536.0` mirrored Rust ↔ shader; existing test pattern-matches the literal as a fragile drift guard.
- TD4-016 — `NifVersion(0x14010001)` (NIF v20.1.0.1, string table boundary) hardcoded twice with "must match" comment.
- TD4-017 — `BINDLESS_CEILING = 65535` correctly named; the predecessor `1024` leaked into 4 unrelated sites.
- TD4-018 — `MAX_INSTANCES = MAX_INDIRECT_DRAWS = 0x40000` expressed as independent literals; should be `MAX_INDIRECT_DRAWS = MAX_INSTANCES`.
- TD4-019 — `user_version_2 > 130 / < 131` partition in `header.rs:127-130`; should be `bsver::FALLOUT4`.
- TD4-020 — Bloom + volumetrics workgroup sizes (`WORKGROUP_X = 8`) duplicate Rust ↔ shader `layout(local_size_x = 8)`.

**Stub Implementations (Dim 5) — 5**
- TD5-004 — Stale-docs follow-up: `CLAUDE.md` and audit prompt both reference deleted `legacy/{tes3,tes4,tes5,fo4}.rs` per-game stubs (see also TD7-001).
- TD5-005 — `StagingPool` not threaded through frame loop / scene load; `None` literals at 2 call sites (see also TD1-001/002).
- TD5-007 — `audio::SoundCache` is a dormant API with zero engine call sites; `len() == 0` steady-state. First consumer is M44 Phase 3.5b.
- TD5-008 — `IMGS` / `ACTI` / `TERM` records captured as raw payloads; no consumer until M47.0 / M48.
- TD5-009 — `FootstepConfig` hardcoded to dirt-walk WAV; M44 Phase 3.5b FOOT-record→material lookup pending.

**Test Hygiene (Dim 6) — 10**
- TD6-001 — `bsa::archive::extract_and_parse_nif` is print-only with no parse call; name overpromises.
- TD6-002 — `bench_draw_sort_serial_vs_parallel` is a Criterion-shaped bench in `#[test]` clothing.
- TD6-003 — `audit-performance.md` "must not regress" claims 8 baselines; 5 have no dedicated regression test (#824, #828, #830, #832, #833 dispatch).
- TD6-004 — `mtidle_motion_diagnostic.rs` named "diagnostic", asserts only `> 0.0` (very loose).
- TD6-005 — `BYROREDUX_FNV_DATA` env-var honored inconsistently; some `#[ignore]`'d tests hard-code Steam install path.
- TD6-006 — Golden-frame coverage is one scene (cube demo); TAA / GPU skin / composite have no per-pass golden.
- TD6-007 — `per_block_baselines.rs` per-game parser floors are all `#[ignore]`; CI has no game data → no regression signal.
- TD6-008 — `m41-equip.sh` smoke-tests README doesn't reference the parallel `skinning_e2e.rs` test.
- TD6-009 — `nif/tests/common/mod.rs` test-helper only reachable via `mod common;` in each test binary.
- TD6-010 — `cargo test` reports green count but not skipped-by-category; suite shrinkage by deletion would look like a win.

**Documentation (Dim 7) — 17**
- TD7-002 — `CLAUDE.md:82` says VulkanContext has 54 fields; actual 71.
- TD7-005 — `CLAUDE.md:316` says "888 tests passing"; actual 1979.
- TD7-007 — `CLAUDE.md` shaders list missing 9 files (TAA, bloom, water, caustic, volumetrics, skin compute).
- TD7-009 — `CLAUDE.md:229` says 186 NIF type names; README says 291; actual 206. Three docs disagree.
- TD7-010 — `ROADMAP.md:235` references `render.rs:204` for `MAX_TOTAL_BONES` cap; post-Session-34 the cap check is at `:358`.
- TD7-011 — `ROADMAP.md` compat-matrix vs project-stats vs CLAUDE.md disagree on per-game clean rates and sweep date.
- TD7-013 / TD7-014 — README Skyrim Bannered Mare (`1932 entities`) and FO4 MedTek (`7434 entities`) drifted to 3209 / 10809 after M41 NPC spawn lands `Inventory`/`EquipmentSlots`.
- TD7-015 — README Phase-2 paragraph implies FO4 NPCs have armor meshes; ROADMAP is explicit they don't until `.hkx` skeleton stub lands.
- TD7-016 — README "291 dispatch arms" claim disagrees with CLAUDE and actual code (206).
- TD7-021 — `audit-renderer.md:241` cap `MAX_MATERIALS = 4096` matches code; HISTORY.md:128 carries stale `MAX_MATERIALS = 1024` from `#797 SAFE-22`.
- TD7-022 — `HISTORY.md:181` `#779 prepass dance` entry sits in "shipped" section despite being net-zero (reverted twice).
- TD7-024 — INFO — `session34_layout.md` memory note mostly accurate; minor drift on a few LOC counts.

**Backwards-Compat Cruft (Dim 8) — 10**
- TD8-001 — `TextureRegistry::new` carries unused `_swapchain_image_count`; single caller, no API concern.
- TD8-002 — `debug_server::start` takes unused `_world: &mut World`; single caller.
- TD8-003 — `SkinnedMesh::new` is a legacy shim duplicating `new_with_global`; zero production callers.
- TD8-004 — Stale `#[allow(dead_code)]` on 4 items now actively read (overlaps TD2-001/002/003 + `embedded_clip`).
- TD8-005 — Genuinely-dead helpers gated by `allow(dead_code)` (overlaps TD2-005).
- TD8-006 — `RefrTextureOverlay::inner` preserved "for parity" with no consumer.
- TD8-007 — ~30 unused `pub use` re-exports across renderer / bsa / bgsm / audio / facegen / spt / scripting / physics crate roots.
- TD8-008 — `humanoid_skeleton_path` + 2 siblings carry unused `_gender: Gender` "for a future mod-aware lookup."

**File / Function Complexity (Dim 9) — 17**
- File splits (>2000 LOC): TD9-001 `acceleration.rs` (4200, see Top 5 #5), TD9-002 `context/draw.rs` (2554, blocked on TD9-008), TD9-003 `scene_buffer.rs` (2367), TD9-004 `context/mod.rs` (2348), TD9-005 `nif/import/mesh.rs` (2212), TD9-006 `nif/blocks/collision.rs` (2162), TD9-007 `nif/anim.rs` (2101).
- Function extractions (>200 LOC): TD9-008 `draw_frame()` (2322), TD9-009 `SceneBuffers::new()` (356), TD9-010 `VulkanContext::new()` (745), TD9-011 `build_blas_batched()` (1269), TD9-012 `build_tlas()` (684), TD9-013 `extract_local_bound()` (562), TD9-014 `walk_controller_chain()` (356), TD9-015 `parse_render_debug_flags_env()` (370).
- Match arm explosions: TD9-016 NIF block dispatcher (272 arms), TD9-017 `RecordType::Display` (101 arms; table replacement).

**Audit-Finding Rot (Dim 10) — 6**
- TD10-002 — "all 3 shaders" → actually 6 (refined version of TD7-020/TD7-023).
- TD10-004 — `audit-performance.md:70` cites `streaming.rs:286` for `pre_parse_cell`'s `into_par_iter`; actual line 474.
- TD10-006 — `audit-performance.md:88` references `gpu_instance_size_*` test prefix; actual names are `gpu_instance_is_112_bytes_std430_compatible` + siblings.
- TD10-007 — `audit-performance.md:61` + `audit-ecs.md:59` describe transform_propagation cache key fields imprecisely.
- TD10-008 — `audit-fo4.md:69` cites `crates/plugin/src/esm/cell.rs:211`; `cell.rs` was split into `cell/` in Session 34.
- TD10-009 — Pattern observation: 8 of 10 Dim 10 findings are bare line-number anchors; switch to symbol-based anchors (`file.rs::function`).
- TD10-010 — `.claude/issues/<N>/ISSUE.md` files have no `Status:` line — design choice, but the audit-tech-debt spec expects LOCAL-vs-GitHub desync detection here; either add the field or update the spec.

---

## Deferred

Findings that are honest debt but gated on milestones still in progress. Track these on the milestone, not as standalone tech-debt issues:

| Finding | Gating Milestone |
|---------|------------------|
| TD5-001 SpeedTree placeholder billboard → real geometry | SpeedTree Phase 2 |
| TD5-002 StencilState consumer | `#337` (no milestone, renderer follow-up) |
| TD5-003 BSSky/Water shader-flag dispatch | `#977` (M38 follow-up) |
| TD5-006 M55 volumetrics density/integration | **M55** (planned, not started) |
| TD5-007 SoundCache first consumer | **M44 Phase 3.5b** |
| TD5-008 IMGS/ACTI/TERM consumers | **M47.0 / M48** |
| TD5-009 FootstepConfig FOOT lookup | **M44 Phase 3.5b** |
| TD2-016 BA2 partial-mip-range reads | M40 streaming |
| TD9-002 / TD9-008 `draw_frame` split + file split | Needs RenderDoc baselines per `feedback_speculative_vulkan_fixes`; not blockable until then |

---

## Methodology Notes

- Baseline counts (`/tmp/audit/tech-debt/baseline.txt`) captured at audit start so the next run can diff.
- Each dimension ran as a Task agent with the dim-specific checklist; results landed in `/tmp/audit/tech-debt/dim_<N>.md`.
- Severity floor: LOW unless a tech-debt finding qualifies under one of the promotion triggers in `_audit-severity.md` (`/audit-tech-debt` skill body, "Severity Guidance for Tech Debt"). Only **TD4-002** qualified for HIGH (silent overflow/underflow under a documented use case).
- Cross-dimension dedup: TD7-020 / TD7-023 / TD10-002 (shader struct sync "3 vs N"), TD1-001-002 / TD5-005 (StagingPool TODOs ↔ stub), TD2-001/002/003 / TD8-004 (stale dead-code mutes), TD2-005 / TD8-005 (cache dead helpers), TD5-004 / TD7-001 / TD7-019 (deleted legacy/ stubs). Each is reported once with `(see also)` cross-references.

---

## Next Step

```
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-05-13.md
```

`/audit-publish` will apply the `tech-debt` label (in addition to severity + domain labels) when the publish phase reads `Audit: TECH_DEBT` from this report header.
