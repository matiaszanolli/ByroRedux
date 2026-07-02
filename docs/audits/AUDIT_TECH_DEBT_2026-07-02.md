# Tech-Debt Audit — 2026-07-02

**Scope**: All 9 dimensions, `--depth deep`. Full codebase (`crates/` 21 crates + `byroredux/`).
**Method**: Discovery recipes per `.claude/commands/audit-tech-debt/SKILL.md`; every finding
re-verified against live source. Dedup baseline = `tech-debt`-labelled GitHub issues
(188 all-state) + prior `AUDIT_TECH_DEBT_*.md` reports (last: 2026-06-26).

## Executive Summary

The codebase is in **very clean shape**. The 2026-05-13 → 2026-06-26 tech-debt
sweeps closed nearly every prior finding, and this pass confirms the fixes held:

- **Path gate GREEN** — `_audit-validate.sh` reports 980/980 refs valid across 26 skill files.
- **Zero genuine markers** — all TODO/FIXME/HACK/XXX hits are protocol tags (`XXXX`
  ESM extended-size) or documentation of upstream/reference-impl FIXMEs. `unimplemented!/todo!()` = 0.
- **GPU-struct doc rot fixed** — `renderer.md`, `bindings.glsl`, `shader-pipeline.md`
  all now consistent at GpuInstance 112 B / GpuCamera 336 B / GpuMaterial 300 B / Vertex 100 B.
- **feature-matrix rot fixed** — the Save/load (M45) and M47.2 rows corrected;
  Terrain-LOD and Ragdoll rows now read "~ Partial/Classic".
- **Prior dead-`pub fn` deleted** — `mesh.rs::oriented_quad` / `fullscreen_quad_vertices` gone.
- **Bookkeeping issues closed** — #1627 / #1704 / #1709 now CLOSED on GitHub.

Findings this pass: **4 total** — 0 CRITICAL, 0 HIGH, 0 MEDIUM, 4 LOW. Three are
oversized-file split candidates (Dim 1), one is a redundant `#[allow(dead_code)]`
annotation that the code outgrew (Dim 8).

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 4 |

### Delta vs baseline (2026-06-26)

| Metric | 06-26 | 07-02 | Note |
|--------|-------|-------|------|
| Marker total | ~17 | 17 | all false positives (XXXX / ref-impl FIXME) |
| `allow(dead_code)` | ~20 | 20 | 1 now redundant (start_mip); rest justified |
| `unimplemented!/todo!()` | 0 | 0 | codebase prefers explicit fallbacks |
| files >2000 LOC | 6 | 6 | membership stable; see Dim 1 |
| path gate | GREEN | GREEN | 980 refs |

## Baseline Snapshot (for next audit to diff)

```
TODO/FIXME/HACK/XXX:    17   (all false positives — protocol / ref-impl docs)
allow(dead_code):       20   (19 justified, 1 redundant — TD8-001)
unimplemented!/todo!(): 0
#[ignore] tests:        267  (all GPU/game-data/audio-device gated — not debt)
files >2000 LOC:        6
```

Oversized set (unchanged from 06-26):
```
4265  crates/renderer/src/vulkan/context/draw.rs
3335  crates/renderer/src/vulkan/context/mod.rs
2846  byroredux/src/main.rs
2370  crates/nif/src/import/collision.rs   (~1085 prod / ~1285 test)
2166  crates/nif/src/blocks/particle.rs    (~1378 prod / ~788 test)
2065  crates/plugin/src/esm/records/actor.rs (~1154 prod / ~911 test)
```

## Top Quick Wins

1. **Drop the redundant `#[allow(dead_code)]` on `Dx10Chunk::start_mip`** (TD8-001,
   trivial) — the field is now read by the monotonic-mip validation added `885532b8`.

## Top Medium Investments

1. **Split `context/draw.rs` (4265 LOC)** by extracting `draw_frame` (~1844 LOC) and the
   two record helpers into per-pass modules (TD1-001).
2. **Extract `context/mod.rs::new()` (986 LOC)** — the Vulkan init chain — into an
   ordered builder (TD1-002).
3. **Slim `byroredux/src/main.rs` (2846 LOC)** — `about_to_wait` (379 LOC) + `main`
   (347 LOC) + `render_one_frame` (307 LOC) into a boot/config vs event-loop split (TD1-003).

---

## Findings

### TD1-001: `context/draw.rs` is 4265 LOC with a 1844-LOC `draw_frame`
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1992-3836` (`draw_frame`),
  `:784-1404` (`record_geometry_pass`, ~620 LOC), `:1404-1992` (`record_skinned_blas_refit`, ~588 LOC)
- **Status**: NEW (largest file; grew after the Session-34/35 splits closed the original set)
- **Description**: The largest file in the tree. Production code runs to line ~3836
  (tests below); the single `draw_frame` function is ~1844 LOC and two record helpers
  add ~1200 more. Every per-frame renderer edit, review, and merge pays a tax here.
- **Evidence**: `awk` fn-boundary scan — `draw_frame` @1992→3836; `record_geometry_pass`
  @784; `record_skinned_blas_refit` @1404. First `#[cfg(test)]` at 3852.
- **Impact**: Highest-leverage complexity debt; not a correctness bug.
- **Effort**: large (decompose first). **This is Vulkan command-recording code** —
  per `feedback_speculative_vulkan_fixes.md`, split by *extracting cohesive recording
  blocks into `&self` helpers*, NOT by reordering barriers/passes. Suggested axis:
  acquire/sync → geometry-pass record → RT/BLAS refit → post-passes → submit.
- **Suggested Fix**: Extract the post-pass sequence (already isolated as `record_post_passes`
  @415) further, and lift the geometry-record and skinned-refit blocks that `draw_frame`
  inlines into named `record_*` helpers so `draw_frame` becomes an orchestrator.

### TD1-002: `context/mod.rs` is 3335 LOC with a 986-LOC `new()` init chain
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1526-2512` (`new`, ~986 LOC),
  `:903-1438` (`parse_render_debug_flags_env`, 535 LOC), `:2831-3146` (`Drop`, ~315 LOC)
- **Status**: NEW
- **Description**: The `VulkanContext::new()` init chain is a single 986-LOC function
  (entry → instance → debug → surface → device → allocator → swapchain → render pass →
  pipeline → framebuffers → pool → sync). `parse_render_debug_flags_env` is 535 LOC of
  env-var → flag mapping. Both are legitimate but oversized single units.
- **Evidence**: fn-boundary scan; `#[cfg(test)]` starts at 3136 (prod ≈ 3136 LOC).
- **Impact**: Init-order changes and debug-flag additions touch giant functions.
- **Effort**: medium. Init-chain ordering is invariant #6 in CLAUDE.md — preserve the
  full ordered chain; extract into a private `struct VulkanInit { … }` staged builder so
  each stage is a named method, not a split that reorders. `parse_render_debug_flags_env`
  is a pure string→u32 table — extract to a lookup-driven helper.
- **Suggested Fix**: Staged builder for `new()`; table-driven flag parse. No behavior change.

### TD1-003: `byroredux/src/main.rs` is 2846 LOC (App + event loop + world build)
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `byroredux/src/main.rs:2228-2607` (`about_to_wait`, 379 LOC),
  `:110-457` (`main`, 347 LOC), `:1609-1916` (`render_one_frame`, 307 LOC),
  `:753-1007` (test helper `mg07_on_activate_dispatch`, 254 LOC), `:1183` (`step_streaming`, 204 LOC)
- **Status**: NEW (crossed 2000 since the Session-34 split; SKILL notes `byroredux/` files grew)
- **Description**: The binary's `main.rs` carries the winit `ApplicationHandler` impl,
  boot/config, world construction, and the streaming/transition stepping in one file.
  The SKILL's suggested axis: App/ApplicationHandler event loop vs system registration
  vs boot/config.
- **Evidence**: fn-boundary scan (above).
- **Impact**: Boot config, event handling, and frame stepping all co-located.
- **Effort**: medium. Split boot/config (`main`, `build_world`) into a `boot.rs` module
  and keep the `ApplicationHandler` impl in `main.rs`; the streaming/transition steppers
  (`step_streaming`, `step_cell_transition`) can move to an `app_step.rs`.
- **Suggested Fix**: Extract boot + step logic; leave the event-loop impl in `main.rs`.

### TD8-001: Redundant `#[allow(dead_code)]` on `Dx10Chunk::start_mip` (field is now read)
- **Severity**: LOW
- **Dimension**: 8 (Dead Code)
- **Location**: `crates/bsa/src/ba2.rs:148-149`
- **Status**: NEW (predicted as TD8-004 in the 2026-06-26 report; the read landed, the annotation stayed)
- **Description**: `start_mip` carries `#[allow(dead_code)]`, but it *is* read by the
  BA2 DX10 monotonic-mip validation at `ba2.rs:621` (`w[0].start_mip <= w[1].start_mip`,
  added in `885532b8`, 2026-05-18). The annotation is now dead and masks nothing.
  (The sibling `end_mip` on line 151 is still only assigned, never read — its
  `#[allow(dead_code)]` remains justified pending the M40 streaming consumer, #1049.)
- **Evidence**:
  ```
  148:    #[allow(dead_code)]
  149:    start_mip: u16,
  ...
  621:    let monotonic = chunks.windows(2).all(|w| w[0].start_mip <= w[1].start_mip);
  ```
- **Impact**: None at runtime; a stale annotation that suppresses a warning that no
  longer fires — pure lint hygiene.
- **Effort**: trivial (delete line 148). Keep the `end_mip` annotation.
- **Suggested Fix**: Remove the `#[allow(dead_code)]` above `start_mip` only.

---

## Verified-Clean (no finding — recorded so the next audit does not re-litigate)

- **Dim 2 (Duplication)**: All Z-up→Y-up flips route through the canonical
  `byroredux_core::math::coord` / `crates/nif/src/import/coord.rs` / `anim/coord.rs`
  helpers (`zup_to_yup_quat`, `zup_matrix_to_yup_quat`, `zup_point_to_yup`) — no leaked
  re-implementations. No divergent-bug-fix duplication surfaced.
- **Dim 3 (Doc rot)**: Path gate GREEN (980 refs). GPU-struct byte sizes consistent
  across `renderer.md` (112/336/300), `shader-pipeline.md` (112/336/300),
  `bindings.glsl` (`gpu_material_size_is_300_bytes` cite). `material.rs` `classify_pbr`
  doc comments all correctly frame it as *deleted/historical*; surviving symbols are
  `classify_pbr_keyword` (free fn) + `Material::resolve_pbr`. feature-matrix M45/M47.2/
  Terrain-LOD/Ragdoll rows corrected (TD3-* from 06-26 held).
- **Dim 4 (Audit rot)**: Gate clean across all 26 skill files.
- **Dim 5 (Markers)**: 0 genuine. All hits are `XXXX` (ESM protocol, `reader.rs`/
  `magic.rs`), ref-impl FIXME docs (`bgem.rs`, `bs_geometry.rs`), or resolved-context
  mentions (`scene.rs:775` closes #1055). MIT attribution block atop `triangle.frag` intact.
- **Dim 6 (Stubs)**: `unimplemented!/todo!()` = 0. All "stub"/"placeholder" comments
  document intentional best-effort fallbacks (NIF opaque-block skip, SpeedTree billboard,
  ESM stub-shape captures) with issue refs — not unfinished work. `condition.rs` header
  now correctly reads "13 functions" (GetFactionRank/HasPerk implemented).
- **Dim 7 (Magic numbers)**: GPU sizes pinned by `gpu_instance_layout_tests.rs`
  (112/336) + `gpu_material_size_is_300_bytes`; shader defines sourced from
  `shader_constants_data.rs`. No stray literals surfaced.
- **Dim 8 (Dead code)**: 19 of 20 `#[allow(dead_code)]` justified (NIF schema-completeness
  `VF_*` constants under #336/#358; RAII-Drop `debug_server`; std430-consumed `count`;
  future-hook `texture_indices` #1199; deserialize-schema `RawDependency.name`;
  `end_mip` #1049). `_unused` bindings in ESM parsers are intentional stream-byte skips.
  Only TD8-001 (start_mip) is stale. Prior dead `pub fn`s (`mesh.rs`) confirmed deleted.
- **Dim 9 (Test hygiene)**: All 267 `#[ignore]` tests gate on GPU / on-disk game data /
  audio device with explicit opt-in reasons (`--ignored`). None guard a closed
  CRITICAL/HIGH fix while suppressed. Not debt.

## Deferred

None. No finding is gated on an in-progress milestone.
