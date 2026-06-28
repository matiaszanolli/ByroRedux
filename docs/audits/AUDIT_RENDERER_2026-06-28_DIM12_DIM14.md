# Renderer Audit — Dimensions 12 (Command Buffer Recording) + 14 (Caustic Splat)

- **Date**: 2026-06-28
- **Command**: `/audit-renderer 12 14` → `--focus 12,14 --depth deep`
- **Branch**: main
- **Method**: Orchestrator + 2 dimension agents (renderer-specialist), adversarial per-finding disproof, symbol-anchored verification against the live tree. Reference docs (`docs/engine/shader-pipeline.md`, `docs/engine/memory-budget.md`) treated as authoritative. Dedup baseline: `gh issue list` (35 open) + prior `docs/audits/` reports (notably the comprehensive sweeps `AUDIT_RENDERER_2026-06-26.md` and `AUDIT_RENDERER_2026-06-14.md`). Orchestrator independently re-verified every actionable item.

> **Numbering note**: the older `*_DIM12.md` / `*_DIM14.md` reports in `docs/audits/`
> predate a dimension renumbering — under the *old* scheme Dim 12 = GPU skinning and
> Dim 14 = Material Table. This report uses the **current** scheme: Dim 12 = command
> buffer recording, Dim 14 = caustic splat. The relevant prior coverage of *these*
> topics lives in the comprehensive sweeps, not the same-numbered focused reports.

---

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 1 | D14-LOW-01 |
| INFO | 2 | D14-INFO-01, X-INFO-01 |

Both dimensions are in good shape. **Dimension 12 (command buffer recording) is
zero-finding** — every lifecycle / ordering / per-draw / counter-independence
invariant verified to hold, extending the clean result of the prior sweeps.
**Dimension 14 (caustic splat)** surfaced exactly one actionable item, a **LOW**
test-coverage gap (a shader missing from the constants-header lockstep test); the
prior u32-wrap hazard (REN-D14-NEW-01) is confirmed **RESOLVED** (#1575) and
recast as a regression guard. One cross-cutting **INFO** records a Dim-14
*skill-checklist* wording error (not a code defect).

No release-blocking issues. No speculative Vulkan changes proposed (per standing
guidance); no barrier/layout inconsistency found.

---

## RT Pipeline Assessment

**Caustic accumulation path (Dim 14)** — the glass/refractive `caustic.rs` (#321
Option A) path is correct end-to-end: per-FIF `R32_UINT` accumulator with
`STORAGE | SAMPLED | TRANSFER_DST` usage, kept in `GENERAL` for the whole frame
(the only `UNDEFINED → GENERAL` transition is the one-time init/resize walk, never
per-frame — no discard); `imageAtomicAdd` fixed-point deposits with the
`CAUSTIC_FIXED_SCALE = 65536.0` constant matching on both the Rust (`shader_constants_data.rs`)
and shader (`composite.frag`) sides; RT-gated on `sceneFlags.x` (shader early-out
**and** CPU dispatch-skip when no TLAS handle); per-FIF descriptor + image so no
cross-frame WAR; output added to **direct** only (double-count guard against the
SVGF-denoised indirect holds).

**Command-buffer ordering (Dim 12)** — recorded pass order matches
`docs/engine/shader-pipeline.md` "Per-Frame Submission Order" item-for-item: skin
compute → AS build (BLAS/TLAS, **outside** any render pass) → main render pass →
G-buffer barrier → post-geometry compute (SVGF / caustic / volumetrics / TAA /
SSAO / bloom) → composite (last raster) → egui → optional screenshot copy → submit
→ present. The single `cmd_begin_render_pass` for the main pass begins only after
`build_tlas`, so AS builds are never recorded inside a render pass.

## GPU-Struct & Memory Assessment

No layout or lifecycle findings in scope. The caustic accumulator lifecycle
(create / per-frame clear / barrier chain / resize-recreate / destroy) is balanced;
the source-flag plumbing rides `GpuInstance.flags` (bit pinned by
`instance_flag_bits_match_scene_buffer_consts`), not the material table, so no
`GpuMaterial` interaction is in play. Command-buffer recording leaks nothing on
the error/unwind paths — the #956/#992 `debug_assert!→warn+clamp` downgrade (to
avoid leaking the in-flight cmd buffer on unwind) and the #910 acquire-signal
recovery on all fallible sites are both intact.

---

## Findings

### LOW

#### D14-LOW-01: `caustic_splat.comp` (and `water.frag`) missing from the constants-header lockstep test

- **Severity**: LOW
- **Dimension**: Caustics
- **Location**: `crates/renderer/src/shader_constants.rs` :: `affected_shaders_include_constants_header`
- **Status**: NEW
- **Description**: `caustic_splat.comp` `#include`s the generated `include/shader_constants.glsl` (`caustic_splat.comp:7`) and depends on it for `INSTANCE_FLAG_CAUSTIC_SOURCE` (used at the source-gate, `caustic_splat.comp:200`) and `WORKGROUP_X` / `WORKGROUP_Y`. The `affected_shaders_include_constants_header` test enumerates the shaders that MUST retain that `#include` so the single-source-of-truth flag/constant contract can't silently drift — but its list omits `caustic_splat.comp`. Orchestrator-confirmed the enumerated set is exactly: `cluster_cull.comp`, `triangle.frag`, `triangle.vert`, `skin_vertices.comp`, `skin_palette.comp`, `composite.frag`, `bloom_downsample.comp`, `bloom_upsample.comp`, `volumetrics_inject.comp`, `volumetrics_integrate.comp` — neither `caustic_splat.comp` nor `water.frag` appears. If the include line were dropped in a refactor, `INSTANCE_FLAG_CAUSTIC_SOURCE` would become an undefined identifier and no `cargo test` would catch it (the SPIR-V is pre-compiled).
- **Evidence**: `grep 'shader_constants.glsl' caustic_splat.comp` → line 7; `awk '/fn affected_shaders_include_constants_header/,/^}/'` over `shader_constants.rs` → 10 shaders, `caustic_splat.comp` absent. `water.frag` also absent (it has only an indirect `water_frag_motion_enum_matches` guard).
- **Impact**: Missing regression guard, not a runtime bug — the include is present today. Blast radius is a future refactor that strips the include and isn't caught until the `.spv` is hand-regenerated.
- **Suggested Fix**: Add `("caustic_splat.comp", include_str!("../shaders/caustic_splat.comp"))` to the `affected_shaders_include_constants_header` tuple list (and, while there, `water.frag`).

### INFO

#### D14-INFO-01: Combined-caustic u32-wrap fix is in place (regression guard for REN-D14-NEW-01)

- **Severity**: INFO (resolved — regression guard)
- **Dimension**: Caustics
- **Location**: `crates/renderer/shaders/composite.frag` :: fragment `main` (geometry-pixel branch)
- **Status**: Resolved by #1575 (`9c10f14e`, 2026-06-15)
- **Description**: The 2026-06-14 sweep raised REN-D14-NEW-01: summing the glass and water accumulators as u32 before the float divide (`float(causticRaw + waterCausticRaw) / CAUSTIC_FIXED_SCALE`) could wrap modulo 2³² to ~0 at extreme overlapping concentration, producing a black pixel the post-divide `min(…,16.0)` firefly cap could not recover. The current code promotes each accumulator to float **before** the add: `float causticLum = (float(causticRaw) + float(waterCausticRaw)) / CAUSTIC_FIXED_SCALE;` (`composite.frag:382`, orchestrator-confirmed), with the `CAUSTIC_FIREFLY_MAX = 16.0` cap correctly applied AFTER the divide (`composite.frag:391-392`).
- **Impact**: None (resolved). Recorded so a future edit re-collapsing the sum to u32 is recognized as a regression of #1575.
- **Suggested Fix**: None required. Optionally add a `composite.frag` source-assertion (analogous to `water_frag_motion_enum_matches`) pinning the two `float(...)` casts.

#### X-INFO-01: Dim-14 skill checklist mis-describes the caustic-source flag location (instance flags, not the material table)

- **Severity**: INFO (skill doc-rot — no code change)
- **Dimension**: Caustics / audit-skill maintenance
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 14 checklist); live code at `crates/renderer/shaders/caustic_splat.comp` :: source-gate, `crates/renderer/src/vulkan/context/draw.rs` :: `is_caustic_source`.
- **Status**: NEW (analogous to REN-2026-06-26-I01 for Dim 12)
- **Description**: The Dim-14 checklist states source-pixel selection "reads the material flag from `materials[material_id]` (post-R1), using `INSTANCE_FLAG_CAUSTIC_SOURCE`." That conflates two things. The caustic-source bit is an **instance** flag on `GpuInstance.flags`, not a material-table field: the shader reads `uint flags = instances[instIdx].flags; if ((flags & INSTANCE_FLAG_CAUSTIC_SOURCE) == 0u) return;` (`caustic_splat.comp:199-200`), and the CPU sets it via `is_caustic_source(draw_cmd)` → `f |= INSTANCE_FLAG_CAUSTIC_SOURCE` (`draw.rs:133`, `:2336-2337`), pinned by `is_caustic_source_tests` (`draw.rs:3938+`). The only `materials[inst.materialId]` mention in `caustic_splat.comp` is a *deferred-work* comment (saving 16 B of per-instance `avgAlbedo*` by moving it to the material table, #1098) — not the live source-selection path. The underlying code is correct and macro-driven (not a hex literal, #1234 holds); only the checklist wording is wrong.
- **Impact**: None at runtime. A future auditor following the checklist literally would hunt for a non-existent `materials[]` read and could mis-flag the correct instance-flag path.
- **Suggested Fix**: Reword the Dim-14 checklist bullet to: "source-pixel selection reads `INSTANCE_FLAG_CAUSTIC_SOURCE` from `GpuInstance.flags` (set CPU-side by `is_caustic_source`, #1234) — NOT a hex literal; the `materials[material_id]` reference in `caustic_splat.comp` is a deferred-work comment (#1098), not the live path."

---

## Coverage Record — invariants verified to HOLD (no finding)

### Dimension 12 — Command buffer recording (zero findings)

1. **Record lifecycle**: `reset_command_buffer` → `begin_command_buffer` (ONE_TIME_SUBMIT) → `end_command_buffer`, single/balanced. Reset-error path returns before any `cmd_begin_render_pass`. The lone main-pass `cmd_begin_render_pass` ↔ `cmd_end_render_pass` body contains no `?`/`return`/`bail!` (adversarially scanned) — the pass cannot be left open on an error path. Composite (`composite.rs`) and egui (`egui_pass.rs`) open/close their own passes in single blocks; egui captures `draw_result` *without* `?`, ends the pass, *then* propagates (#1637/#1491). Every `return Ok(false)` (#1211 empty-framebuffers guard, pinned by `framebuffers_empty_guard_tests`), `Ok(true)` (`ERROR_OUT_OF_DATE_KHR`), `bail!`, and the 5 `Err` recovery sites land before `begin` or after `end` — never inside an open pass/buffer.
2. **Pass ordering**: matches `shader-pipeline.md` "Per-Frame Submission Order" item-for-item (skin → AS build outside RP → main RP → barrier → SVGF/caustic/vol/TAA/SSAO/bloom → composite last raster → egui → screenshot → submit → present). Composite confirmed last raster; egui preserves `PRESENT_SRC_KHR` so the screenshot `PRESENT_SRC → TRANSFER_SRC` barrier is valid.
3. **Per-draw recording**: decal depth bias via `cmd_set_depth_bias` per-layer-change with a `None` sentinel forcing the first explicit set (Decal anchor never left at pipeline default); set 0 (bindless) + set 1 (scene) bound once/frame; per-batch pipeline rebind only on `pipeline_key` change; push constants + `cmd_draw_indexed` / `cmd_draw_indexed_indirect`; batch-coalescing merge loop folds consecutive `DrawCommand`s sharing the full state key + SSBO contiguity, dynamic state (`cmd_set_cull_mode` / depth) fired only on change.
4. **Counter-independence regression guards**: #1258 (`DrawCallStats.batch_count` vs `indirect_call_count` vs app-side input count — three independent metrics); #1259 (`all_cached` short-circuit + pipeline rebind only on key change); #1260 (off-frustum draws zero `flags`, frustum-border *visible* draws keep full assembly, SSBO slot still populated off-frustum for the #516 RT-hit contract); #1235 (`world.insert(placement_root, SceneFlags::from_nif(cached.root_flags))` in `cell_loader/spawn.rs`, `cached.root_flags: u32` in `cell_loader/nif_import_registry.rs`, `SceneFlags::from_nif` in `crates/core/src/ecs/components/scene_flags.rs`).
5. **Adjacent recording-safety**: #910 acquire-signal recovery on all 6 fallible sites; #952 `reset_fences` immediately before `queue_submit`; `copy_depth_to_history` recorded outside any pass with paired READ_ONLY↔TRANSFER barriers; `resize.rs` records no command buffers (covered by #1211).

### Dimension 14 — Caustic splat (1 LOW, 2 INFO)

1. **Accumulator lifecycle**: `CAUSTIC_FORMAT = R32_UINT`, usage `STORAGE | SAMPLED | TRANSFER_DST`; moving-camera clear via `cmd_clear_color_image` with `GENERAL→GENERAL` pre-clear + `TRANSFER→COMPUTE` post-clear barriers; HOST→COMPUTE UBO barrier before dispatch; COMPUTE→FRAGMENT barrier before composite sample; layout stays `GENERAL` all frame.
2. **Atomic accumulation**: `imageAtomicAdd` u32 fixed-point; `CAUSTIC_FIXED_SCALE = 65536.0` matches Rust (`shader_constants_data.rs`) → generated header → `composite.frag:382` divide.
3. **Source + output**: source gate uses `INSTANCE_FLAG_CAUSTIC_SOURCE` macro (#1234, no hex literal), read from `GpuInstance.flags` (see X-INFO-01 re: checklist wording); output added to **direct** only (`composite.frag:395` `direct + indirect*albedo + caustic`), never into SVGF indirect; sampled via `usampler2D` + divide.
4. **Boundary comment**: `caustic_splat.comp` "water-side caustic is the water shader's responsibility" matches the live `water.frag` (`imageAtomicAdd(waterCausticAccum, …)`, same scale), not a stub.
5. **RT gating**: `if (sceneFlags.x < 0.5) return;` early-out (`caustic_splat.comp:188`) + CPU dispatch-skip unless `tlas_handle(frame)` is `Some`.
6. **Dispatch coverage**: `div_ceil(8)` matches `WORKGROUP_X/Y = 8`; in-shader bounds guard; per-FIF descriptor + image (no cross-frame WAR, gated by per-frame fence).

---

## Prioritized Fix Order

1. **D14-LOW-01** (LOW, test) — add `caustic_splat.comp` (and `water.frag`) to `affected_shaders_include_constants_header`. One-line test edit; pins the single-source-of-truth flag contract for the caustic source gate. Low effort, do first.
2. **X-INFO-01** (INFO, skill maintenance) — reword the Dim-14 caustic-source checklist bullet to reference `GpuInstance.flags` instead of `materials[material_id]`. No code change; prevents a recurring auditor false-trail (sibling of the already-applied REN-2026-06-26-I01 for Dim 12).
3. **D14-INFO-01** (INFO) — no action; optionally pin the #1575 float-promotion with a `composite.frag` source-assertion.

## Needs-RenderDoc

None. No sync/barrier finding in either dimension required capture-based
verification; all barrier chains were read for consistency only and found
consistent.
