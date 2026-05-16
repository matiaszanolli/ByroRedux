# Tech-Debt Audit — 2026-05-16

**Scope**: 10 dimensions, deep depth.
**Prior baselines**: `AUDIT_TECH_DEBT_2026-05-13.md`, `AUDIT_TECH_DEBT_2026-05-14.md`.

---

## Executive Summary

**58 findings total** across 10 dimensions.

| Severity | Count | Dimension Distribution |
|----------|-------|------------------------|
| MEDIUM   | 6     | D1 (2), D3 (2), D7 (1), D9 (1), D10 (2) |
| LOW      | 52    | D1 (0), D2 (4), D3 (4), D4 (9), D5 (4), D6 (3), D7 (5), D8 (7), D9 (8), D10 (6) |

**Delta vs 2026-05-14 audit**: TODO/FIXME burndown holding (6 hits, of which only 2 are live code-side markers). `#[allow(dead_code)]` dropped from 42 → 25 → 23 today, confirming the burndown plan is on track. Files > 2000 LOC: just 2 remain (both Vulkan-recording hot paths intentionally deferred per `feedback_speculative_vulkan_fixes.md`).

**The pattern that won't die**: Audit-skill path-drift after Session 34/35 module splits. `#1039` swept `_audit-common.md` two days ago, but per-game audit skills weren't touched and now carry 7 fresh stale paths (TD7-045..049). Symbol-anchor pattern (`#1040`) or a CI grep gate is the structural fix.

---

## Baseline Snapshot

```
Date: 2026-05-16
TODO/FIXME/HACK/XXX: 6
allow(dead_code): 25
unimplemented!/todo!(): 1
#[ignore] tests: 105
files >2000 LOC: 2
```

---

## Top 10 Quick Wins (trivial effort, immediate payoff)

1. **TD2-201** — Strip unused doc comment on `crates/renderer/src/shader_constants.rs:1` (cargo-check warning).
2. **TD2-202** — Remove unused `VERTEX_STRIDE_FLOATS` import in `skin_compute.rs:25`.
3. **TD2-203** — Add `#[allow(dead_code)]` + comment to `LightHeader.count` (byte-copied to GPU SSBO; Rust can't see).
4. **TD1-003 / TD1-004** — Repoint the two close-with-marker orphan TODOs in `water.frag:221` and `caustic_splat.comp:71` to "tracked under closed #NNN" framing.
5. **TD4-209** — Replace `data.len() == 4` localized-string sentinel with `size_of::<u32>()` in `records/common.rs:167`.
6. **TD7-045..049** — Sweep 5 stale paths across per-game audit skill files (audit-skyrim/fo4/starfield/oblivion/fnv/fo3/speedtree).
7. **TD10-001..004** — Update `audit-audio.md` + `audit-renderer.md` line numbers (drift after Session 34/35).
8. **TD8-016** — Drop 8 blanket `#[cfg(test)] #[allow(unused_imports)]` test-module attrs in `cell_loader.rs` + `scene.rs` post-Session-35.
9. **TD8-018** — Strip 3× `_gender: Gender` placeholder args on `humanoid_*` lookups (gender is forwarded only to be dropped).
10. **TD3-206** — Replace inline `ImageSubresourceRange { COLOR, 0, 1, 0, 1 }` literals with the existing `color_subresource_single_mip()` helper.

Total quick-win effort: ~3 hours, ~15 LOC delta.

---

## Top 5 Medium Investments (file/function splits, consolidations)

1. **TD3-202 (MEDIUM)** — Introduce `EXTERIOR_CELL_UNITS = 4096.0` const + `cell_grid_to_world_yup` helper. Already has divergent bug-fix history (TD3-110 Z-flip sign disagreement in `exterior.rs`). Effort: small.

2. **TD3-203 (MEDIUM)** — Drive adoption of `CommonItemFields::from_subs` across the 13+ ESM parsers still re-rolling the EDID/FULL/MODL walk. Mechanical, no logic change. Effort: medium.

3. **TD3-201** — Extract `compute_storage_image_barrier()` helper in `descriptors.rs`. 8+ duplicate emit sites across svgf/taa/caustic/bloom/volumetrics. Effort: small.

4. **TD4-203..208** — Extend `shader_constants_data.rs` + `build.rs` codegen (#1038 hub) to cover BLOOM_INTENSITY, VOLUME_FAR, WATER_KIND_*, DBG_*, workgroup sizes, and cluster constants. Closes 6 LOW findings in one PR. Effort: medium.

5. **TD7-050 (MEDIUM)** — Pivot audit-skill maintenance from one-shot sed sweeps to either a CI grep-gate on audit-skill paths or the `#1040` symbol-anchor pattern. The drift class has recurred two days after `#1039` closeout. Effort: small.

---

## HIGH Severity

None. No magic-number overflow / silent-truncation risk surfaced (TD4 highest was LOW).

---

## MEDIUM Severity (6 findings)

### TD1-003: water.frag traceWaterRay TODO references CLOSED #1070

- **Severity**: MEDIUM
- **Dimension**: Stale Markers
- **Location**: `crates/renderer/shaders/water.frag:221`
- **Age**: 51281f3db (2026-05-15)
- **Description**: `TODO(M38-Phase2 / #1070)` planted in the same commit that closed #1070. The marker is orphaned — no successor open issue. The reader can't tell what the actual gating milestone is.
- **Suggested Fix**: Repoint the marker to a long-tail tracking issue (or open one), or change framing to "Tracked under closed #1070 — landed Phase 1; Phase 2 deferred to <milestone>".
- **Effort**: trivial

### TD1-004: caustic_splat.comp avgAlbedo TODO references CLOSED #1098

- **Severity**: MEDIUM
- **Dimension**: Stale Markers
- **Location**: `crates/renderer/shaders/caustic_splat.comp:71`
- **Age**: f3fcc2985 (2026-05-16, today)
- **Description**: `TODO(#1098 / REN-D13-001)` planted today by the same commit that closed #1098. Same orphan-TODO pattern as TD1-003.
- **Suggested Fix**: Same as TD1-003 — repoint to milestone framing.
- **Effort**: trivial

### TD3-202: `4096.0` exterior-cell unit literal scattered across 6 files (no canonical const)

- **Severity**: MEDIUM
- **Dimension**: Logic Duplication
- **Locations**: `byroredux/src/cell_loader/exterior.rs`, plus 5 more callers per Dim 3 sweep.
- **Description**: The Bethesda exterior-cell unit value (4096.0) appears as a bare literal in 6 files. Already has divergent bug-fix history (TD3-110 Z-flip sign disagreement). A consolidation point exists in `crates/nif/src/import/coord.rs`.
- **Proposed Consolidation**: `EXTERIOR_CELL_UNITS: f32 = 4096.0` const + `cell_grid_to_world_yup(grid_x, grid_y) -> Vec3` helper in `crates/core/src/math/cell.rs` (or `byroredux_core::math`).
- **Effort**: small

### TD3-203: `CommonItemFields::from_subs` adoption stalled at 1 file

- **Severity**: MEDIUM (refresh of TD3-102, status changed)
- **Dimension**: Logic Duplication
- **Locations**: Currently only `crates/plugin/src/esm/records/items.rs` uses the helper; 13+ other parsers still re-roll EDID/FULL/MODL walk.
- **Description**: The helper was introduced specifically to deduplicate subrecord parse loops, but adoption stopped after one file. The remaining 13+ parsers are exactly the pattern the helper was built for.
- **Proposed Consolidation**: Migrate `actor.rs`, `weather.rs`, `tree.rs`, `pkin.rs`, etc. to `CommonItemFields::from_subs`. Mechanical.
- **Effort**: medium

### TD7-050: Audit-skill path-drift class still recurring 2 days after `#1039` closeout

- **Severity**: MEDIUM
- **Dimension**: Stale Documentation
- **Description**: `#1039` swept `_audit-common.md` paths after the Session 34/35 splits. Two days later, the per-game audit skills carry 7 fresh stale paths (TD7-045..049). The recurrence rate argues for either a CI grep-gate on audit-skill paths or migration to `#1040`-style symbol-anchor references instead of a third sed sweep.
- **Suggested Fix**: Either (a) add a script in `.claude/commands/_audit-validate.sh` that greps audit-skill file paths against the live tree and fails CI on missing refs, or (b) replace numeric line references with symbol names (e.g., `streaming.rs:streaming_state_machine` instead of `streaming.rs:286`).
- **Effort**: small

### TD9-001: `byroredux/src/render.rs` carries a 1306-LOC `build_render_data` god-function

- **Severity**: MEDIUM
- **Dimension**: File / Function Complexity
- **Location**: `byroredux/src/render.rs::build_render_data` (1306 LOC)
- **Description**: Per-frame hot path. Mixes 8 ECS query families (static meshes, skinned, lights, particles, water, terrain, sort, mod). The function size is intimidating but the perf-sensitivity gates a naive split.
- **Proposed Split**: 8-sibling submodule structure under `byroredux/src/render/` — `static_meshes.rs`, `skinned.rs`, `lights.rs`, `particles.rs`, `water.rs`, `terrain.rs`, `sort.rs`, `mod.rs`. Must preserve the single `World` query pass and the consolidated entity-iteration cost.
- **Effort**: large (perf-sensitive, requires bench before/after)

### TD10-001: `audit-audio.md:81` cites stale line numbers for drain cap warn

- **Severity**: MEDIUM (audit baseline drift — would point an audit at wrong code)
- **Dimension**: Audit-Finding Rot
- **Location**: `.claude/commands/audit-audio.md:81` cites `lib.rs:600-602`; actual is `lib.rs:760`.
- **Description**: An audio audit run against this baseline would scan the wrong block of code, miss the actual drain-cap-warn site.
- **Suggested Fix**: Update line numbers, OR (preferred) switch to symbol-anchor reference (`lib.rs::drain_cap_warn` style).
- **Effort**: trivial

### TD10-002: `audit-audio.md:77` cites stale line numbers for listener_id early-return

- **Severity**: MEDIUM
- **Dimension**: Audit-Finding Rot
- **Location**: `.claude/commands/audit-audio.md:77` cites `lib.rs:603-606`; actual sites are `lib.rs:740` and `lib.rs:827`.
- **Description**: Same class as TD10-001 — audit baseline drift.
- **Suggested Fix**: Update line numbers / migrate to symbol anchors.
- **Effort**: trivial

---

## LOW Severity (52 findings)

### Dim 2: Dead Code & Unused Surface (4)

- **TD2-201** — Unused doc comment on `crates/renderer/src/shader_constants.rs:1` (binds to `include!()` which can't carry docs).
- **TD2-202** — Unused import `VERTEX_STRIDE_FLOATS` in `skin_compute.rs:25` (3 textual references are doc/format-string only).
- **TD2-203** — `LightHeader.count` field never read in Rust source. False-positive but real signal — needs explicit `#[allow(dead_code)]` + safety comment ("byte-copied to GPU SSBO via raw pointer cast in upload.rs:42").
- **TD2-204** — `VF_FULL_PRECISION` constant duplicated under `#[allow(dead_code)]` in `sse_recon.rs:56` when identical live constant exists in `blocks/tri_shape.rs:474`.

### Dim 3: Logic Duplication (4 — beyond the 2 MEDIUMs)

- **TD3-201** — Compute-pass GENERAL→GENERAL storage-image barrier boilerplate, 8+ sites. Helper: `compute_storage_image_barrier` in `descriptors.rs:139`.
- **TD3-204** — `impl_ni_object!` macro adopted partially (hand-rolled impls dropped 174 → 33). Migration unfinished.
- **TD3-205** — Vec field destroy-and-clear loop pattern unchanged in `scene_buffer/descriptors.rs::destroy`.
- **TD3-206** — `ImageSubresourceRange { COLOR, 0, 1, 0, 1 }` inlined at multiple compute-pass sites despite `color_subresource_single_mip()` existing.

### Dim 4: Magic Numbers (9)

- **TD4-201** — 32 bare `bsver()` integer compares survive in `crates/nif/src/blocks/`. Should use named constants from `crates/nif/src/version.rs`.
- **TD4-202** — 142 `data.len() >= N` subrecord-size gates in ESM records. `records/misc/water.rs` accounts for 22 in a single file.
- **TD4-203** — `BLOOM_INTENSITY` literal mirrored Rust ↔ shader without drift test.
- **TD4-204** — `VOLUME_FAR` literal mirrored Rust ↔ shader without drift test.
- **TD4-205** — Water motion enum (`WATER_CALM`/`RIVER`/`RAPIDS`/`WATERFALL`) lives only in shader.
- **TD4-206** — `DBG_*` debug-viz bit flags shader-only at `triangle.frag:743-784`.
- **TD4-207** — `local_size_x = 8` workgroup size duplicated across 4 compute shaders, no Rust mirror.
- **TD4-208** — Cluster shading constants (`NEAR` / `FAR_FLOOR` / `FAR_FALLBACK` / `THREADS_PER_CLUSTER`) shader-only.
- **TD4-209** — `data.len() == 4` localized-string sentinel in `records/common.rs:167`. Use `size_of::<u32>()` or a named const.

### Dim 5: Stub & Placeholder Implementations (4 — all carryover)

- **TD5-001** — SpeedTree `--tree` CLI renders placeholder billboard. Shipped-CLI reachable. Gated on SpeedTree Phase 2.
- **TD5-002** — `StencilState` parsed but pipeline hardcodes `stencil_test_enable(false)`. Gated on `#337`.
- **TD5-003** — `BSSkyShaderProperty` / `BSWaterShaderProperty` flags captured, zero renderer consumers.
- **TD5-008** — IMGS/ACTI/TERM record parsers test-only. Gated on Tier 4 (interactivity).

### Dim 6: Test Hygiene (3)

- **TD6-201** — Prior TD6-101..105 (CRITICAL/HIGH-regression-backing) RESOLVED by #1058. Audit confirms.
- **TD6-202** — `data_dir(env, default)` helper duplicated across 5 crates (`audio` has 6 inline copies). Candidate for a `byroredux-testutil` workspace crate (~50 LOC removed).
- **TD6-203** — Golden frame baseline `byroredux/tests/golden/cube_demo_60f.png` is 7 days old; today's shader edits (caustic_splat, composite, svgf_temporal, triangle.frag, ui.vert, volumetrics_integrate) won't trip auto-check.

### Dim 7: Stale Documentation (5 — beyond the 1 MEDIUM)

- **TD7-045** — `audit-skyrim.md:56` / `audit-fo4.md:50` / `audit-starfield.md:68` cite deleted `crates/nif/src/blocks/bs_tri_shape.rs` (folded into `blocks/tri_shape.rs`).
- **TD7-046** — `audit-oblivion.md:67` cites `crates/nif/src/import/material.rs` — now a directory.
- **TD7-047** — `audit-fnv.md:48` + `audit-fo3.md:60` cite `crates/plugin/src/esm/cell.rs` — split in Session 34.
- **TD7-048** — `audit-fo4.md:80` / `audit-oblivion.md:61` / `audit-skyrim.md:86` cite deleted `crates/plugin/src/legacy/{fo4,tes4,tes5}.rs` (removed under `#390`).
- **TD7-049** — `audit-speedtree.md:78` cites `crates/spt/src/import.rs` — now a module dir.

### Dim 8: Backwards-Compat Cruft (7)

- **TD8-016** — 8 blanket `#[cfg(test)] #[allow(unused_imports)]` test-module attrs survive in `cell_loader.rs:28-41` (5) and `scene.rs:27-59` (3) two weeks after the Session 35 split. Mask real unused-import lints.
- **TD8-017** — `watr_to_params(record, _game: GameKind)` placeholder param has no consumer divergence.
- **TD8-018** — 3× `_gender: Gender` placeholders on `humanoid_skeleton_path` / `humanoid_body_paths` / `humanoid_default_idle_kf_path` (`npc_spawn.rs:56,84,250`). Explicit "kept for future mod-aware flip" pattern but CLAUDE.md is explicit: delete instead.
- **TD8-019** — `crates/plugin/src/legacy/mod.rs:24-32` carries a "There used to be `tes3`/`tes4`/`tes5`/`fo4` submodules…" obituary for #390 deletions. Pure breadcrumb.
- **TD8-020** — (audit confirmation) Cargo features `inspect` and `parallel-scheduler` both wired — no dead-flag risk.
- **TD8-021** — (audit confirmation) Zero `#[deprecated]` attributes workspace-wide.
- **TD8-022** — (audit confirmation) cell_loader has one canonical `nif_import_registry` path; no legacy alias.

### Dim 9: File / Function Complexity (8 — beyond the 1 MEDIUM)

- **TD9-002** — `crates/nif/src/lib.rs` `parse_nif_with_options` is 580 LOC inside a 1673-LOC root.
- **TD9-003** — `crates/plugin/src/esm/records/mod.rs` `parse_esm_with_load_order` is 760 LOC.
- **TD9-004** — `crates/nif/src/import/walk.rs` at 1916 LOC mixes hierarchical + flat walk paths.
- **TD9-005** — `crates/nif/src/blocks/tri_shape.rs` (1875 LOC) co-locates 4 distinct shape variants.
- **TD9-006** — `crates/bsa/src/archive.rs` (1619 LOC) carries 3 BSA versions in one file.
- **TD9-007** — `crates/nif/src/import/tests.rs` at 1788 LOC.
- **TD9-008** — `crates/nif/src/anim/tests.rs` at 1596 LOC.
- **TD9-009** — `crates/nif/src/blocks/shader_tests.rs` at 1610 LOC.

### Dim 10: Audit-Finding Rot (6 — beyond the 2 MEDIUMs)

- **TD10-003** — `audit-renderer.md:281` cites `triangle.frag:863` for `DBG_BYPASS_NORMAL_MAP` — actual decl at line 763.
- **TD10-004** — `audit-renderer.md:282` line range `739-829` for DBG bit catalog is borderline (decls span 743-829).
- **TD10-005** — `audit-tech-debt.md:182` self-references `streaming.rs:286` as a sample baseline that is itself stale.
- **TD10-006** — (non-finding) Dimension counts in sync across all audit skills.
- **TD10-007** — (non-finding) `AUDIT_RENDERER_2026-05-15.md` findings all triaged into `.claude/issues/1081-1109`.
- **TD10-008** — (non-finding) "Existing: #NNN" callouts in `audit-renderer.md` reference CLOSED issues by design.

---

## Deferred

These findings depend on milestones still in progress. Document the gate; don't act yet.

| Finding | Gating Milestone |
|---------|------------------|
| TD5-001 | SpeedTree Phase 2 (no ROADMAP row yet) |
| TD5-002 | `#337` Stencil pipeline variants (Tier 5 renderer polish) |
| TD5-003 | M33+ Sky/Weather renderer consumer surface |
| TD5-008 | Tier 4 (interactivity) |
| TD9-001 | Bench-of-record refresh + RenderDoc validation (perf-sensitive) |

---

## Closed Since 2026-05-14 (verification)

- **TD5-010 / TD5-011 / TD5-013 / TD5-016** — closed by `#1062` (commit `8dbed20f`) "add parse-but-don't-consume gate markers"
- **TD1-001 / TD1-002 / TD5-005** — StagingPool TODOs closed by `#1055`
- **TD6-101..105** — hardcoded-Steam-path tests closed by `#1058` (centralized `test_paths.rs` + `data_dir(env, default)` helper)
- **`#[allow(dead_code)]` count** 42 → 23 confirms ongoing burndown

---

## Recommended Next Step

Run `/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-05-16.md` to file MEDIUM findings as GitHub issues. The 52 LOWs are mostly mechanical sweeps; the publish skill will dedupe against existing OPEN issues automatically.
