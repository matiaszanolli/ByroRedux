# Renderer Audit — Dimension 18: Volumetric Lighting (M55)

**Date:** 2026-06-04
**Scope:** Single-dimension focus (`--focus 18`, depth `deep`). Froxel grid
inject + integrate compute passes, composite consumption, the
`VOLUMETRIC_OUTPUT_CONSUMED` gate (#928), per-FIF buffer lifecycle, HG phase
math, barrier scopes, and resize rebind (#905). No prior dedicated DIM18 audit
existed — Dim 18 was explicitly **deferred** in `AUDIT_RENDERER_2026-05-28.md`,
so this is first coverage.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 3 |
| INFO (pass-through) | 14 |

The M55 volumetric subsystem is in **good health**. The primary concern this
audit was opened against — "volumetric contribution is gated OFF in composite
but the inject+integrate dispatch still runs every frame, wasting ~28 MiB of
image writes + ~1.84M per-froxel TLAS shadow rays" — **does not exist**. The
host const `volumetrics::VOLUMETRIC_OUTPUT_CONSUMED = false`
(`crates/renderer/src/vulkan/volumetrics.rs:124`) is the single source of truth
and gates three sites in lockstep:

1. `crates/renderer/src/vulkan/context/draw.rs:2850` — gates the entire
   `vol.dispatch()`; when false, inject + integrate are **not dispatched**.
2. `crates/renderer/src/vulkan/context/draw.rs:1965` — mirrors the const into
   the composite push-constant `depth_params.z`.
3. `crates/renderer/shaders/composite.frag:438` — consumes `vol.a`/`vol.rgb`
   only when `depth_params.z > 0.5`, else falls through to a `vol.rgb * 0.0`
   SPIR-V keep-alive (so binding 6 stays referenced and reflection-valid).

So the "gated off" state is the **correct skip-dispatch behaviour** the #928
invariant requires, not a dispatched-then-ignored waste. (Independently verified
during this audit: const `false` → dispatch skipped, gate zeroed, composite read
off — all three sites read the same const.)

The 3 findings are all **LOW** and **latent** — they manifest only if the const
is flipped to `true` (for M-LIGHT v2) without addressing them first. They should
ride the same changeset that flips the const.

---

## RT Pipeline Assessment

The volumetric inject pass issues one TLAS shadow ray per froxel with
`gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT` (visibility-only, no
closest-hit walk) — cost-bounded and correct. The TLAS-freshness latch
(`tlas_written[frame]` debug-asserted before dispatch, reset each frame) prevents
stale/undefined AS reads (#1105). With the const `false` the entire RT cost is
**zero** (not dispatched). No AS-correctness or barrier defects.

## Rasterization / Compute-Pipeline Assessment

Froxel dims `160 × 90 × 128` match the shader `local_size` (inject `8/8/8`,
integrate `8/8/1`); dispatch is ceiling-division with a correct shader bounds
check on the over-covered Y rows. Per-FIF volumes (~14 MiB/slot RGBA16F) are not
shared across frames (no WAR on the volumes). All three barrier hops
(inject-write → integrate-read, prior-composite-read → integrate-write,
integrate-write → composite-read) have correct stage/access scopes. Resize
rebinds composite binding 6 from `integrated_views()`. No pipeline-state or
synchronization findings.

---

## Findings

### LOW

#### REN-DIM18-01: Inject slice-center vs integrate front-slab vs composite texel-edge depth mismatch (latent)
- **Severity:** LOW
- **Dimension:** Volumetrics (M55)
- **Location:** `crates/renderer/shaders/volumetrics_inject.comp:100-104`,
  `crates/renderer/shaders/volumetrics_integrate.comp:55-71`,
  `crates/renderer/shaders/composite.frag:396-397`
- **Status:** NEW
- **Description:** Three slightly different depth conventions. Inject samples each
  froxel at its **center** (`t = ((z+0.5)/128) * VOLUME_FAR`). Integrate treats
  slice `i` as a **front-aligned** uniform slab (`inscatter[i]*T_cum*dt`, then
  `T_cum *= exp(-extinction[i]*dt)`). Composite reconstructs the slice by
  **texel-edge** sample (`slice = clamp(worldDist/VOLUME_FAR, 0, 0.9999)`,
  CLAMP_TO_EDGE + bilinear). Under the documented linear distribution this is a
  ~half-slab (~0.78 m at 200 m / 128 slices) bias in the transmittance/inscatter
  curve.
- **Impact:** Sub-froxel fog-depth bias when output is consumed; no crash, no NaN.
  **Harmless today** (`VOLUMETRIC_OUTPUT_CONSUMED == false`, composite read dead).
  Becomes a subtle fog-depth offset the moment the const flips.
- **Suggested Fix:** When M-LIGHT v2 flips the const, reconcile the three
  conventions (shift composite's `slice` by +0.5 texel to match the center
  sample, or document the half-slab bias as accepted under the Phase-2 linear
  model). Track on the existing #928 flip checklist.

#### REN-DIM18-02: Integration param UBO is single-buffered, not per-FIF (safe today, fragile if dt goes dynamic)
- **Severity:** LOW
- **Dimension:** Volumetrics (M55)
- **Location:** `crates/renderer/src/vulkan/volumetrics.rs:405-418`
  (`integration_param_buffer`), bound at `:533-538`
- **Status:** NEW
- **Description:** The injection param UBO is correctly per-FIF (written each
  frame). The **integration** param UBO is a single buffer written **once at
  construction** with the constant `dt = VOLUME_FAR / FROXEL_DEPTH`. Correct
  today because `dt` is immutable, so all FIF slots can alias one read-only UBO.
  But the shader doc + Phase-5 plan (`volumetrics_integrate.comp:14-15`,
  `volumetrics.rs:126-128`) call for a per-slice/per-frame exponential `dt`. If a
  future contributor starts writing this buffer per-frame without making it
  per-FIF, frame N+1's host write races frame N's in-flight integrate read — the
  exact WAR hazard the per-FIF injection buffer already avoids.
- **Impact:** None currently. Latent WAR hazard if Phase 5 makes `dt` dynamic
  without converting to per-FIF.
- **Suggested Fix:** Add a comment at `volumetrics.rs:411` ("single-buffered
  because dt is immutable; convert to per-FIF `Vec` before making dt dynamic —
  Phase 5") so the constraint is visible at the edit site.

#### REN-DIM18-03: Inject Y-dispatch over-covers (90 → 96 invocations) — informational, correctly bounds-checked
- **Severity:** LOW (informational; correctly handled)
- **Dimension:** Volumetrics (M55)
- **Location:** `crates/renderer/src/vulkan/volumetrics.rs:827-830`, guard at
  `crates/renderer/shaders/volumetrics_inject.comp:93-97`
- **Status:** NEW (informational)
- **Description:** `FROXEL_HEIGHT=90` is not a multiple of `WORKGROUP_Y=8`, so the
  inject dispatch launches `ceil(90/8)=12` groups → 96 rows, 6 over the grid.
  **Correctly handled** by the `any(greaterThanEqual(coord, size))` early-return.
  Width (160) and depth (128) are exact multiples. Noted only to confirm no
  over-write past `imageSize`.
- **Impact:** None — over-dispatched groups early-return.
- **Suggested Fix:** No action needed; the bounds check is correct.

---

## INFO — Verification Pass-Throughs (checklist items confirmed correct)

- **Froxel dims match shader `local_size`** — 160/90/128; inject `8/8/8`,
  integrate `8/8/1`; ceiling-division dispatch. PASS.
- **Integrate dispatch z=1** — 2D column dispatch matching `local_size_z=1` and
  the internal Z-march loop. PASS.
- **Per-FIF volumes** — `lighting_volumes` + `integrated_volumes` are
  `MAX_FRAMES_IN_FLIGHT`-length, one ~14 MiB RGBA16F per slot, no cross-frame
  sharing. PASS.
- **Inject shadow ray bounded** — `OpaqueEXT | TerminateOnFirstHitEXT`, single
  proceed, no closest-hit. PASS.
- **Transmittance multiplied** — `trans_cumulative *= exp(-extinction*dt)`, init
  1.0. Not added. PASS.
- **HG `g` clamp** — `clamp(g, -0.999, 0.999)` + `max(denom, 1e-4)`. g=±1 div-by-0
  impossible. PASS (#1021).
- **#928 dispatch-skip invariant** — `vol.dispatch()` and composite read gated by
  the same const, host/shader lockstep. NOT regressed. PASS (#928, #1013).
- **Resize rebind** — composite binding 6 re-written from `integrated_views()`;
  froxel is resolution-independent so the volume isn't reallocated. PASS (#905).
- **Interior neutral output** — interior cells get `scatter_coef = 0.0` + zeroed
  sun color → integration no-op, composite reduces to `scene*1 + 0`. No
  through-wall god-rays, no NaN. PASS (#1084).
- **First-frame / disabled init** — `initialize_layouts` clears both volumes to
  `(0,0,0,T=1)` ("no fog" sentinel). PASS (#1082).
- **Barriers** — inject-write→integrate-read, prior-composite-read→integrate-write,
  integrate-write→composite-read, HOST→COMPUTE UBO flush; all correct scopes,
  GENERAL layout. PASS.
- **TLAS freshness latch** — `tlas_written[frame]` asserted before dispatch, reset
  each frame. PASS (#1105).
- **Depth distribution consistency** — inject/integrate/composite share the
  `VOLUME_FAR = 200 m` linear model (modulo the half-slab note in REN-DIM18-01).
  PASS.
- **FogVolume ECS driver (#1277)** — ECS component only, not yet wired to dispatch,
  does not bypass the consume gate. PASS.
- **Performance** — with const `false`, inject+integrate cost is zero (not
  dispatched); the <2 ms budget is moot until the const flips. PASS.

---

## Prioritized Fix Order

All three findings are LOW and latent — none affects the currently-shipping
(disabled) state. **No action is required now.** When M-LIGHT v2 flips
`VOLUMETRIC_OUTPUT_CONSUMED = true`, fold the following into that changeset:

1. **REN-DIM18-01** — reconcile the inject-center / integrate-front-slab /
   composite-texel-edge depth conventions (or document the accepted half-slab
   bias).
2. **REN-DIM18-02** — convert `integration_param_buffer` to per-FIF *before*
   making `dt` dynamic (or add the constraint comment now).
3. **REN-DIM18-03** — no action (informational; bounds check correct).
