# Renderer Audit — 2026-05-26 (Dimension 11 only: TAA / M37.5)

**Scope**: single-dimension run via `/audit-renderer 11` — focused re-audit of TAA (Halton (2,3) jitter + YCoCg variance clamp resolve, M37.5). The other 19 renderer dimensions were not run today; see the 2026-05-24 sweeps (Dim 6+14, Dim 21, Dim 15+16) for the most recent state of the broader pipeline.

## Executive Summary

Delta-from-2026-05-18 verifies clean. The jitter generator, un-jittered camera UBO, per-frame-in-flight history ring, motion-vector reprojection, and the YCoCg variance-clamp resolve pass remain consistent with the 2026-05-18 baseline. **Zero NEW findings.** One pre-existing CARRYOVER (Medium) — implicit history-validity on swapchain recreate — is still open and not regressed; tracked here for completeness so the next sweep can compare against it.

| Severity | NEW | CARRYOVER | FIXED-VERIFIED | STALE-ON-PREMISE | INFO |
|----------|-----|-----------|----------------|------------------|------|
| Critical | 0   | 0         | 0              | 0                | 0    |
| High     | 0   | 0         | 0              | 0                | 0    |
| Medium   | 0   | 0         | 0              | 1                | 0    |
| Low      | 0   | 0         | 0              | 0                | 1    |
| **Total**| **0** | **0**   | **0**          | **1**            | **1**|

**Audit-publish pass (2026-05-26)**: F-11.01 verified STALE — `signal_history_reset` + `recreate_on_resize` + `should_force_history_reset` already implement the proposed fix. Not filed. See finding section for the per-site verification.

## RT Pipeline Assessment

Not in scope for Dim 11. The TAA path consumes the post-geometry rasterised output and is downstream of all RT consumers (shadow/reflection/GI ray queries). No interaction with BLAS/TLAS lifecycle. See the most recent renderer sweep for RT correctness.

## Rasterization Assessment (TAA-specific)

- **Halton (2,3) sequence**: standard radical-inverse generator at the jitter computation site in [draw.rs](crates/renderer/src/vulkan/context/draw.rs). Modulus-by-N indexing via the frame counter. Output bounded to `[-0.5, 0.5]` pixel offsets and divided by render extent before projection-matrix injection.
- **Un-jittered projection** preserved in the camera UBO at [scene_buffer/upload.rs](crates/renderer/src/vulkan/scene_buffer/upload.rs) — RT rays + motion-vector reconstruction consume this, not the jittered raster projection.
- **Per-frame-in-flight history slots**: history colour + depth images allocated per slot, indexed by current frame index — see resource arrays in [context/mod.rs](crates/renderer/src/vulkan/context/mod.rs) and the binding writes inside [taa.comp](crates/renderer/shaders/taa.comp).
- **YCoCg neighborhood clamp**: `taa.comp` converts current + neighborhood to YCoCg, builds 3×3 min/max + mean/variance, and clamps the reprojected history before blending — the canonical M37.5 path.
- **History layout transitions**: GENERAL ↔ SHADER_READ_ONLY around the TAA dispatch, barriers present in `draw.rs`. No UNDEFINED transitions per frame.
- **Composite ordering**: composite (ACES tone map) consumes the TAA-resolved colour, not raw geometry output — verified in `draw.rs` command-recording order. The composite descriptor binding 0 is rebound to the TAA output when TAA-on; no shader-side branching needed.

## Checklist Status

| # | Item | Status | Evidence |
|---|------|--------|----------|
| 1 | Halton (2,3) sequence advance + NDC pixel-unit jitter | **PASS** | `crates/renderer/src/vulkan/context/draw.rs` jitter site |
| 2 | Camera UBO carries un-jittered projection | **PASS** | `crates/renderer/src/vulkan/scene_buffer/upload.rs` |
| 3 | Per-frame-in-flight history slots, no aliasing | **PASS** | `context/mod.rs` resource arrays + `taa.comp` bindings |
| 4 | Motion-vector reprojection (linear filter / dilation) | **PASS** | `triangle.vert/frag` produces, `taa.comp` consumes |
| 5 | YCoCg 3×3 neighborhood clamp | **PASS** | `taa.comp` min/max + clamp before blend |
| 6 | Disocclusion / first-frame fallback to current pixel | **PASS** | OOB / invalid-history path in `taa.comp` |
| 7 | Sub-pixel jitter magnitude bounded `[-0.5, 0.5]` px | **PASS** | divided by render extent before projection injection |
| 8 | TAA → Composite ordering | **PASS** | `context/draw.rs` command-recording order |
| 9 | History image layout transitions (GENERAL ↔ SHADER_READ_ONLY) | **PASS** | barriers in `draw.rs` |
| 10 | Shader Struct Sync (TAA uses no `GpuInstance`) | **PASS / N/A** | `taa.comp` does not redeclare shared structs |
| 11 | Swapchain recreate invalidates history | **CARRYOVER (Medium)** | Implicit via zero-init; see F-11.01 below |
| 12 | No per-game branching in TAA path | **PASS** | game-agnostic; no `GameKind` reads in `taa.comp` |
| 13 | Validation-layer-clean in debug | **PASS (INFO)** | No new validation messages observed |

## Findings

### F-11.01 — History validity flag is implicit on swapchain recreate

- **Severity**: MEDIUM
- **Status**: ~~CARRYOVER from 2026-05-18 audit~~ → **STALE on premise** (verified 2026-05-26 during `/audit-publish`)
- **Location**: [crates/renderer/src/vulkan/context/resize.rs](crates/renderer/src/vulkan/context/resize.rs) (`recreate_swapchain`) + [crates/renderer/shaders/taa.comp](crates/renderer/shaders/taa.comp) (history sample path)
- **Related issue**: none — not filed (stale)

> **Audit-publish verification (2026-05-26)**: This finding's premise — "no explicit history-invalid flag" — is FALSE in current source. The mechanism the proposed fix asks for already exists, just under a different name:
> - [`taa.rs:113-132`](crates/renderer/src/vulkan/taa.rs#L113-L132) documents the invariant: `signal_history_reset` zeroes both slots together; `recreate_on_resize` drops every history image; `should_force_history_reset(c) := c < MAX_FRAMES_IN_FLIGHT` is the per-frame gate.
> - [`resize.rs:588-616`](crates/renderer/src/vulkan/context/resize.rs#L588-L616) calls `taa.recreate_on_resize` and explicitly comments at lines 670-673 that the force-history-reset gate runs after resize against "freshly-recreated (effectively undefined) history images."
> - The 2026-05-18 sweep at line 43 already verified this path clean.
>
> Audit agent missed these three sites. Per `feedback_audit_findings.md`, this is the recurring "verify premise against current code BEFORE proposing fix" failure mode. Not filed.

**Symptom**

After `recreate_swapchain`, TAA history images are freshly allocated. Correctness today depends on:
- Zero-initialised contents reading as "weight 0 history" via the motion-vector OOB / luminance-out-of-range fallback, AND
- The next frame's reprojection rejecting the stale-looking history.

There is **no explicit per-slot "history invalid" boolean** — the invariant is implicit.

**Impact**

Low in practice — the first post-resize frame produces a near-history-free blend, visually indistinguishable from a clean start. But the invariant is undocumented and brittle: any future change to history sampling (removing the OOB guard, switching to clamp-to-edge, adjusting the luminance reject threshold) could leak undefined memory into the resolve.

**Suggested fix**

Add `history_valid: [bool; MAX_FRAMES_IN_FLIGHT]` to `VulkanContext`. Set all to `false` in `recreate_swapchain`. Pass via push constant or small UBO to `taa.comp`. On `false`, skip the history blend and copy current → history (force α = 1.0). Clear the flag for that slot after the resolve writes a valid history.

**Effort**: ~30 LOC across `context/mod.rs`, `resize.rs`, `draw.rs`, `taa.comp`.

### INFO-11.A — Halton index wrap

- **Severity**: LOW (INFO)
- **Status**: INFO

`taa.comp`'s jitter index is `frame_counter % HALTON_LEN` (with `HALTON_LEN = 8` per M37.5 spec). Wrap is correct; noted for context that increasing `HALTON_LEN` requires no host change beyond the modulus — the sequence is generated on the fly, not tabled. No action required.

## Cross-cutting notes

- **Format Translation Layer**: no interaction. TAA is downstream of all material/shader translation per `feedback_format_translation.md`.
- **VRAM cost**: 2 × MAX_FRAMES_IN_FLIGHT × swapchain-extent × (RGBA16F history + R32F depth history). At 1920×1080 with FIF=2 that's ~40 MB total — well within the 4 GB renderer budget per `feedback_vram_baseline.md`.
- **Shader Struct Sync hazard**: `taa.comp` does not redeclare any of the shared `Gpu*` structs, so the lockstep contract in `feedback_shader_struct_sync.md` doesn't gate the TAA pipeline.

## Methodology

1. Read the most recent prior Dim 11 audit (`docs/audits/AUDIT_RENDERER_2026-05-18_DIM11_DIM14.md`) for baseline + delta tracking.
2. Walked TAA entry points: `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/shaders/taa.comp`, `crates/renderer/src/vulkan/context/draw.rs` (jitter assembly + dispatch), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (camera UBO contract), `crates/renderer/src/vulkan/composite.rs` + `composite.frag` (TAA-on/off branch).
3. Diffed git log against the 2026-05-18 sweep for TAA / composite touches — only 2 commits in scope, both validated against the checklist.
4. Cross-checked dedup baseline at `/tmp/audit/renderer/issues.json` — no existing open issue covers F-11.01.

---

Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-26_DIM11.md`
