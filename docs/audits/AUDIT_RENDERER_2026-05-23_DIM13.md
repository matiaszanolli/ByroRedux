# Renderer Audit — 2026-05-23, Dim 13 focus

**Focus**: `--focus 13` (Caustic Splat — #321 Option A).
**Depth**: deep.
**Trigger**: first dedicated DIM13-focused audit. The dimension has accumulated 12 historical fixes (#321 / #415 / #417 / #473 / #640 / #649 / #670 / #738 / #922 / #1088 / #1098 / #1099 / #1100 / #1111 / #1149) but no holistic walk; this audit closes that gap and surfaces a single LOW lockstep tightening.
**Prior base**: `AUDIT_RENDERER_2026-05-19.md` (24 findings, no Dim 13 entries — caustics has been quiet since #1099/#1100/#1111 closed on 2026-05-09).
**Open Dim 13 issues at audit start**: none directly — #1210 (REN-DIM17-01) tracks the deferred water-side caustics, belongs to Dim 17; #1230 (REN-D14-NEW-03) tracks the deferred `avg_albedo` migration from `GpuInstance` to `GpuMaterial`, cross-cutting with Dim 14.

## Executive Summary

| Severity | Count | Status |
|----------|-------|--------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 0 | — |
| LOW      | 1 | 1 NEW (shader magic literal) |
| **Total** | **1** | 1 NEW |

**Pipeline areas affected**: caustic compute shader (1-line lockstep tightening).

**Headline**: The Dim 13 pipeline is in good health. All previously-flagged bugs (#640 missing ray flags, #670 wrong tMin/origin bias, #738 missing instance-id bounds, #922 over-broad CPU gate, #1099 unanchored fixed-point clamp, #1100 deprecated TOP_OF_PIPE, #1098 / #1111 R1 deferrals, #1149 inline subresource literals) are confirmed closed and stable. The CPU-side caustic-source gate (post-#922) is covered by 7 unit tests; the descriptor layout is validated against the SPIR-V module at construction; the barrier sequence between dispatch / clear / composite is correct; tone-map ordering at composite preserves "caustic added before ACES, separate from indirect" intent. Single remaining finding is a low-severity shader-side lockstep gap — `caustic_splat.comp` reads `flags & 4u` (magic literal) instead of `flags & INSTANCE_FLAG_CAUSTIC_SOURCE` (the macro the file already `#include`s).

## RT Pipeline Assessment (Dim 13 slice)

- **TLAS dependency** — caustic dispatch is gated on `accel_manager.tlas_handle(frame).is_some()` ([draw.rs:2434-2438](../../crates/renderer/src/vulkan/context/draw.rs#L2434-L2438)). Shader-side mirror at [caustic_splat.comp:159](../../crates/renderer/shaders/caustic_splat.comp#L159) — `if (sceneFlags.x < 0.5) return;`. Both gates in lockstep (#640 closeout).
- **Ray-query flags** — `gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT` ([caustic_splat.comp:288-292](../../crates/renderer/shaders/caustic_splat.comp#L288-L292)). Correct for visibility-only queries; matches the audit-skill checklist.
- **Origin bias** — `G - ns * 0.1`, `tMin = 0.05` (post-#670 / SH-4 fix). `ns` is the light-facing normal (with back-face flip from line 268), preventing self-intersection on grazing-incidence refraction.
- **Per-frame fence wait** — `draw_frame`'s two-slot fence wait at [draw.rs:197-209](../../crates/renderer/src/vulkan/context/draw.rs#L197-L209) covers the cross-frame WAR on `caustic_accum[frame]`. Per-dispatch barriers cover same-frame cache flushing.
- **Barrier chain** (HOST→COMPUTE on UBO, COMPUTE|FRAGMENT→TRANSFER pre-clear, TRANSFER→COMPUTE post-clear, COMPUTE→FRAGMENT post-dispatch) is symmetric and complete.

## Rasterization Assessment (Dim 13 slice)

- **Composite consumption** — [composite.frag:335-339](../../crates/renderer/shaders/composite.frag#L335-L339): `causticRaw → causticLum / CAUSTIC_FIXED_SCALE → albedo * causticLum`, added to `direct + indirect * albedo + caustic` as a separate term. Done in HDR pre-ACES. Caustic is NOT routed through SVGF's input (SVGF reads `raw_indirect` G-buffer only), so the audit-skill criterion "never doubled into the indirect path that SVGF already denoised" is met.
- **Sampler type** — composite reads via `usampler2D` ([composite.frag:56](../../crates/renderer/shaders/composite.frag#L56)) — correct for R32_UINT.
- **Source-pixel selection** — `is_caustic_source` ([draw.rs:52-63](../../crates/renderer/src/vulkan/context/draw.rs#L52-L63)) restricts to MATERIAL_KIND_GLASS + MultiLayerParallax(refraction>0). 7 unit tests pin the narrowing (hair / foliage / particles / skin / effect-shader explicitly rejected).

## Findings

### LOW

#### REN-D13-NEW-09: `caustic_splat.comp` gate uses magic literal `4u` instead of the `INSTANCE_FLAG_CAUSTIC_SOURCE` macro

- **Severity**: LOW
- **Dimension**: Caustic Splat / Shader-Constants lockstep
- **Location**: [crates/renderer/shaders/caustic_splat.comp:164](../../crates/renderer/shaders/caustic_splat.comp#L164)
- **Status**: NEW
- **Description**: The caustic-source gate reads `if ((flags & 4u) == 0u) return;` with the bit value inlined. The file already `#include`s `shader_constants.glsl` ([line 7](../../crates/renderer/shaders/caustic_splat.comp#L7)), which exposes `#define INSTANCE_FLAG_CAUSTIC_SOURCE 4u` (auto-generated from [`shader_constants_data.rs:86`](../../crates/renderer/src/shader_constants_data.rs#L86) via `build.rs`). Sibling shaders use the symbolic form — [`triangle.vert:174`](../../crates/renderer/shaders/triangle.vert#L174) reads `(inst.flags & INSTANCE_FLAG_NON_UNIFORM_SCALE) != 0u`, [`triangle.frag:809`](../../crates/renderer/shaders/triangle.frag#L809) reads `(inst.flags & INSTANCE_FLAG_FLAT_SHADING) != 0u`, [`triangle.frag:872`](../../crates/renderer/shaders/triangle.frag#L872) reads `(inst.flags & INSTANCE_FLAG_TERRAIN_SPLAT) != 0u`.
- **Evidence**:
  ```glsl
  // caustic_splat.comp:164
  uint instIdx = meshId - 1u;
  uint flags = instances[instIdx].flags;
  if ((flags & 4u) == 0u) return;   // ← magic literal
  ```
  vs the canonical pattern in triangle.frag:
  ```glsl
  if ((inst.flags & INSTANCE_FLAG_ALPHA_BLEND) != 0u && ...) // line 917
  if ((inst.flags & INSTANCE_FLAG_FLAT_SHADING) != 0u)       // line 809
  if ((inst.flags & INSTANCE_FLAG_TERRAIN_SPLAT) != 0u)      // line 872
  ```
- **Impact**: Lockstep drift potential. If `INSTANCE_FLAG_CAUSTIC_SOURCE` ever moves to a different bit position (e.g., a new flag claims bit 2 and CAUSTIC_SOURCE shifts to bit 6), the Rust-side test `assert_eq!(INSTANCE_FLAG_CAUSTIC_SOURCE, SB_CAUSTIC_SOURCE)` ([shader_constants.rs:313-320](../../crates/renderer/src/shader_constants.rs#L313-L320)) catches the Rust ↔ generated-define drift, but the bare `4u` literal in `caustic_splat.comp` keeps reading bit 2 — silently severing the gate. The caustic dispatch would then run for whatever instances happen to have bit 2 set under the new scheme (or never fire at all if no instance sets that bit).
- **Related**: #1099 (closed — same "magic literal unanchored from named constant" class of finding, applied to `CAUSTIC_FIXED_SCALE`). #1162 (closed — `DBG_*` redeclaration prevention, related lockstep tightening).
- **Suggested Fix**: One-line replacement:
  ```glsl
  if ((flags & INSTANCE_FLAG_CAUSTIC_SOURCE) == 0u) return;
  ```
  Recompile shader. Optional defense-in-depth: extend the `shader_constants::tests` module with a positive-side check that grep's `caustic_splat.comp` for the symbolic form, mirroring the existing `triangle_frag_dbg_bits_not_redeclared` negative-side check.

## Did-not-find (negative coverage)

- **Per-FIF image creation** — `R32_UINT` + `STORAGE | SAMPLED | TRANSFER_DST`, `MAX_FRAMES_IN_FLIGHT` count, single mip. Confirmed at [caustic.rs:420-436](../../crates/renderer/src/vulkan/caustic.rs#L420-L436).
- **Layout walk** — `initialize_layouts` ([caustic.rs:630-663](../../crates/renderer/src/vulkan/caustic.rs#L630-L663)) uses `srcStageMask = NONE` (post-#1100, correctly avoiding deprecated TOP_OF_PIPE). Layout stays GENERAL across the slot lifetime.
- **Barrier chain** — HOST→COMPUTE (UBO), COMPUTE|FRAGMENT→TRANSFER (pre-clear), TRANSFER→COMPUTE (post-clear), COMPUTE→FRAGMENT (post-dispatch). All four present and access-mask-correct.
- **Atomic accumulator** — `imageAtomicAdd` on `r32ui` storage view; per-contribution clamp anchored to `0xFFFFFFFFu / scale` (post-#1099 fix).
- **`CAUSTIC_FIXED_SCALE`** — single source of truth at [`shader_constants_data.rs:31`](../../crates/renderer/src/shader_constants_data.rs#L31), emitted into [`include/shader_constants.glsl:31`](../../crates/renderer/shaders/include/shader_constants.glsl#L31), consumed by both `caustic_splat.comp` and `composite.frag`.
- **9-binding descriptor set** validated against SPIR-V module at construction via `validate_set_layout` ([caustic.rs:302-312](../../crates/renderer/src/vulkan/caustic.rs#L302-L312)) — descriptor-layout drift fails the build, not the runtime.
- **Pool sizes derived from bindings** via `DescriptorPoolBuilder::from_layout_bindings` (post-#1030 — no hard-coded counts).
- **`color_subresource_single_mip()` helper** consistently used across all 4 barrier sites + the clear range (post-#1149).
- **TLAS gate** — both CPU-side `tlas_handle(frame).is_some()` ([draw.rs:2434-2438](../../crates/renderer/src/vulkan/context/draw.rs#L2434-L2438)) AND shader-side `sceneFlags.x < 0.5` ([caustic_splat.comp:159](../../crates/renderer/shaders/caustic_splat.comp#L159)) — both in place, both correct (#640 closeout).
- **Ray-query flags** — `gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT` ([caustic_splat.comp:288-292](../../crates/renderer/shaders/caustic_splat.comp#L288-L292)).
- **Origin bias** — `G - ns * 0.1`, `tMin = 0.05` (post-#670 / #649 / SH-4 fix).
- **`is_caustic_source` CPU gate** — 7 unit tests confirm Glass + MultiLayerParallax(refraction>0) splat; hair / foliage / particles / skin / effect-shader do NOT (#922 closeout intact).
- **`avg_albedo` source** — still per-instance on `GpuInstance` (not yet migrated to `GpuMaterial`). Documented as deferred at [caustic_splat.comp:75-81](../../crates/renderer/shaders/caustic_splat.comp#L75-L81), tracked by open #1230. Not a new finding.
- **Single-eta single-bounce transport model** — documented as deliberate budget choice at [caustic_splat.comp:199-220](../../crates/renderer/shaders/caustic_splat.comp#L199-L220), tracked by REN-D13-NEW-04 (audit 2026-05-09; defer indefinitely). Not a new finding.
- **Composite tone-map order** — caustic added BEFORE ACES, separate from SVGF-denoised indirect ([composite.frag:339](../../crates/renderer/shaders/composite.frag#L339)).
- **TINT_LUM_FLOOR named constant** — post-REN-D13-NEW-05 (audit 2026-05-09); no drift.

## Prioritized Fix Order

1. **REN-D13-NEW-09** (LOW, NEW) — one-line shader edit at `caustic_splat.comp:164`. Recompile SPIR-V. Defends against a class of regression that already cost #1099 a real fix. Trivial.

## Methodology notes

- Walked the full pipeline end-to-end: pipeline construction → descriptor layout → image creation → layout init → per-frame dispatch sequence → shader body → composite consumption.
- Cross-checked against 12 historically-closed Dim 13 issues — all confirmed fixed and stable in current code.
- 282 renderer tests + 7 `is_caustic_source` CPU-gate tests passing.
- Did not run the engine to capture caustic output — read-only static audit. The render-side correctness is covered by `validate_set_layout` + the unit tests + the lockstep tests against `CAUSTIC_FIXED_SCALE`; no live-frame divergence suspected.
