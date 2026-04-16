# Renderer Audit — 2026-04-14

**Auditor**: Renderer specialists (Claude Opus 4.6, 1M context) — 3 parallel agents
**Commit**: 03f96bd (main)
**Scope**: Full Vulkan renderer — 10 dimensions, depth=deep
**Prior baseline**: AUDIT_RENDERER_2026-04-12c.md (lighting-only) and earlier 2026-04-12 reports

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 4 |
| INFO     | 3 |
| **Total NEW** | **5 actionable** |

Three deltas verified clean: command-buffer/fence alignment (#259), gpu-allocator
block-size tune (#308), and the persistent transfer fence (#301/#302). Six
dimensions returned CLEAN (Dim 5/7/8/9/10 + the verified-clean half of Dim 1).

The single MEDIUM is a shader invariant break in the metal-reflection path that
double-modulates reflections by the local albedo after the #268 albedo-demodulation
work. Everything else is documentation drift, an empty-TLAS edge case, or an error
path in BLAS compaction.

### RT Pipeline Assessment

**Acceleration structures are sound.** BLAS compaction (M36) correctly threads the
compacted device address into TLAS instances; PREFER_FAST_TRACE on BLAS / PREFER_FAST_BUILD+ALLOW_UPDATE on TLAS matches design intent. Device-local TLAS instance buffer (#289) has a correct two-stage barrier chain (HOST_WRITE→TRANSFER_READ→AS_READ). Empty-TLAS-at-init contract is preserved with `padded_count` floored to 8192 instances. Ray queries use the correct TLAS binding (set 1, binding 2) across all four query sites (shadow, reflection, GI, window portal).

**Shaders are mostly consistent.** Window portal threshold change (b5fa09a, 0.95→0.5) and the NiAlphaProperty flag bit gate (ddd76ef) match the CPU-side encoding. Composite reassembly applies ACES after `direct + indirect * albedo`, preserving the #268 demodulation invariant for dielectrics. The metal/glossy reflection path (R6-02) is the lone violator.

### Rasterization Assessment

**Pipeline state, render pass, and command recording are clean.** Vertex input
matches the `Vertex` struct, 6 color attachments wired end-to-end, dynamic state
set per-frame, RESET_COMMAND_BUFFER pool flag in place, render-pass begin/end balanced. Subpass dependencies cover both COLOR_ATTACHMENT_OUTPUT and EARLY/LATE_FRAGMENT_TESTS for `discard`-using shaders. G-buffer images carry SAMPLED usage and are pre-transitioned for first-frame validity.

The four prior renderer findings re-checked are FIXED:
| Prior ID | Title | Status |
|----------|-------|--------|
| R-01 | cluster cull UBO missing prevViewProj | FIXED (cluster_cull.comp:32-40) |
| R-04 | CB-by-frame vs fence-by-frame fragility | FIXED via #259 |
| R-07 | "4 color attachments" stale comment | FIXED (pipeline.rs:192) |
| R-08 | depth DONT_CARE doc inconsistency | FIXED (helpers.rs:76-77) |

### Resource Lifecycle

CLEAN. The new device-local TLAS instance buffers (#289), BLAS compaction
artifacts (M36), and persistent `transfer_fence` (#302) are all destroyed in
correct reverse-creation order. Global geometry SSBO (#294) uses the existing
`deferred_destroy` two-frame countdown.

---

## Findings (grouped by severity)

### MEDIUM

#### R6-02: Metal reflection double-modulates by local albedo
- **Severity**: MEDIUM
- **Dimension**: Shader Correctness / Denoiser invariants
- **Location**: `crates/renderer/shaders/triangle.frag:503-518, 813, 839-841`
- **Status**: NEW
- **Description**: Post #268 the invariant is: `outRawIndirect` carries lighting
  with the local albedo factored out; composite computes
  `direct + indirect * localAlbedo`. For metal/glossy surfaces (metalness>0.3,
  roughness<0.6), lines 513-517 fold `traceReflection().rgb` into `ambient` via
  `mix(ambient, envColor, metalness*F)`. `reflResult.rgb` already carries the
  HIT surface's albedo (line 244: `texture(textures[hitTexIdx], hitUV).rgb * distFade`),
  not the local surface's. That value flows through `indirectLight = (ambient + indirect) * ao`
  into `outRawIndirect`, then composite multiplies by the LOCAL albedo a second time.
- **Evidence**:
  ```glsl
  // triangle.frag:513-517
  vec3 envColor = mix(ambient, reflResult.rgb, reflClarity * reflResult.a);
  ambient = mix(ambient, envColor, metalness * F);
  // triangle.frag:813
  vec3 indirectLight = (ambient + indirect) * ao;
  // triangle.frag:840
  outRawIndirect = vec4(indirectLight, 1.0);
  // composite.frag:127
  vec3 combined = direct + indirect * albedo;
  ```
- **Impact**: Metal reflections darker than intended. Chrome at albedo (1,1,1)
  reads correctly; tinted/dirty metal (typical FNV/Skyrim values 0.5-0.8) loses
  30-50% reflection energy. Likely contributor to why RL-03/RL-06 hacks were
  needed in the prior lighting audit.
- **Related**: #268, RL-03/RL-06 in AUDIT_RENDERER_2026-04-12c.md.
- **Suggested Fix**: Route reflection through the direct path (add to `Lo`,
  metals already have kD≈0). Alternative: compute `envColor / max(albedo, 1e-4)`
  before folding into `ambient` so the composite multiply cancels — needs care
  at the 0-albedo edge.

### LOW

#### D1-02: Empty-TLAS barrier emits ambiguous `size = 0`
- **Severity**: LOW
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:1022-1068`
- **Status**: NEW
- **Description**: Two-stage barrier chain (HOST→TRANSFER, TRANSFER→AS_BUILD)
  is otherwise correct. When `instances.len() == 0` the empty-TLAS init path
  still emits both barriers with `.size(copy_size = 0)` — driver behavior on
  `size = 0` varies between "whole buffer" and "no-op". The build itself is
  valid with `primitive_count = 0`.
- **Suggested Fix**: Skip both barriers + the staging copy when
  `copy_size == 0`; the empty TLAS build is still well-formed.

#### D2-02: BLAS compaction phase-6 leaks on mid-loop allocation failure
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:609-641`
- **Status**: NEW
- **Description**: Phase 6 allocates the compact destination buffers in a loop.
  If `GpuBuffer::create_device_local_uninit` fails mid-loop, the `?` at line 627
  propagates the error without destroying the compact accels/buffers created in
  earlier iterations, and the source `prepared` originals also leak (their Drop
  emits a warning but doesn't free Vulkan handles).
- **Suggested Fix**: Wrap phase 6 in `(|| -> Result { ... })()` and on error
  destroy already-created `compact_accels` entries and `prepared` originals
  before returning. Mirrors the existing cleanup pattern at lines 582-594 / 665-682.

#### R6-01: ui.vert GpuInstance struct drifts from triangle.vert/frag
- **Severity**: LOW
- **Dimension**: Shader Struct Sync
- **Location**: `crates/renderer/shaders/ui.vert:11-31` vs `triangle.vert:14-40`,
  `triangle.frag:28-54`, `crates/renderer/src/vulkan/scene_buffer.rs:46-87`
- **Status**: NEW
- **Description**: Post #294 the Rust `GpuInstance` and both triangle shaders
  declare offset 152 as `uint flags` (bit 0 = non-uniform scale; bit 1 = alpha
  blend). `ui.vert` still names the slot `uint _pad0; uint _pad1;`. Total size
  is 160 B either way so layout isn't corrupted, but the
  [Shader Struct Sync](memory) invariant ("GpuInstance lives in 3 shaders; all
  must be updated in lockstep") is broken — any UI-relevant flag added later
  will be silently ignored.
- **Suggested Fix**: Rename `_pad0` to `flags` in ui.vert (or extract a shared
  `.glsl` include).

#### R34-01: Stale push-constant doc on Vertex
- **Severity**: LOW
- **Dimension**: Pipeline State (doc)
- **Location**: `crates/renderer/src/vertex.rs:11-13, 23-24`
- **Status**: NEW
- **Description**: Doc-comment claims the vertex shader "falls through to the
  push-constant `model` matrix" and references "the per-draw `bone_offset` push
  constant". No push constants exist (pipeline.rs:220-233 — set-layouts only);
  per-instance model/bone_offset live in the instance SSBO (set 1, binding 4).
- **Suggested Fix**: Update doc to "per-instance `model` matrix in the instance
  SSBO" and "instance SSBO's `bone_offset`".

#### R34-02: Stale render-pass attachment-format comment
- **Severity**: LOW
- **Dimension**: Render Pass (doc)
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:48-54`
- **Status**: NEW
- **Description**: Header comment lists normal as `RGBA16_SNORM` and implies
  full 32-bit mesh_id. Actual: `NORMAL_FORMAT = R16G16_SNORM` (octahedral, per
  #275) and `MESH_ID_FORMAT = R16_UINT` (effective 65534-instance ceiling).
- **Suggested Fix**: Update comment to `RG16_SNORM octahedral normal` and
  `R16_UINT mesh_id (up to 65534 instances)`.

### INFO (verified clean — no action)

- **D1-01** (#301/#302) — Persistent transfer fence reuse correctly serialized
  via Mutex held across reset→submit→wait→free. Leaf locking; `device_wait_idle`
  in Drop guarantees unsignaled fence at destroy.
- **D1-03** (#259) — Per-frame CB alignment trivially safe: CB indexed by `frame`
  identical to `in_flight[frame]`; wait-then-reset ordering correct.
- **D2-01** (#308) — gpu-allocator block-size reduction is reservation-only;
  block lifetime tracking unchanged. No new leak vector.

### Re-verified prior findings (Dedup vs 2026-04-12c)

| Prior ID | Status |
|----------|--------|
| RL-01 (sRGB linearization, parse-time) | UNCHANGED — out of renderer scope |
| RL-02 (duplicate lights) | UNCHANGED — CPU-side |
| RL-03 (missing per-light ambient) | UNCHANGED — still open in shader |
| RL-04 (NIF radius 2048) | UNCHANGED — CPU-side |
| RL-05 (no exposure control) | PARTIAL — `exposure = 0.85` hardcoded in composite.frag (no runtime uniform / auto-exposure); LOW severity stands |
| RL-06 (remove 2.5x ambient boost) | RESOLVED per a5f48f6 — `triangle.frag:772-776` now reads raw XCLL ambient with no floor |
| RL-07 (APPLY_REPLACE unlit) | UNCHANGED — parse-time |
| RL-08 (fxlight filter) | UNCHANGED — CPU-side |

---

## Prioritized Fix Order

1. **R6-02** (MEDIUM) — Metal reflection albedo double-modulation. Visual
   correctness regression in PBR pipeline; touches the same surfaces RL-03/RL-06
   were trying to compensate for.
2. **D2-02** (LOW) — BLAS compaction error-path leak. Doesn't trigger on the
   happy path but will fragment GPU memory on first OOM during compaction.
3. **D1-02** (LOW) — Empty-TLAS `size = 0` barrier ambiguity. Driver-portable
   safety net.
4. **R6-01, R34-01, R34-02** (LOW × 3) — Documentation/struct-name drift.
   Single-pass doc cleanup.

---

## Confirmed-Correct Patterns (this audit)

| Pattern | Status |
|---------|--------|
| BLAS PREFER_FAST_TRACE / TLAS PREFER_FAST_BUILD+ALLOW_UPDATE | Correct |
| BLAS compaction → device address → TLAS instance flow (M36) | Correct |
| TLAS device-local instance buffer two-stage barrier (#289) | Correct |
| Empty TLAS at init (8192-instance floor) | Correct |
| `instance_custom_index` encoding matches draw command index | Correct |
| Ray query TLAS binding (set 1 binding 2) across all 4 query sites | Correct |
| Window portal alpha threshold + NiAlphaProperty flag gate | Correct |
| Composite ACES applied AFTER `direct + indirect * albedo` | Correct |
| #268 demodulation invariant (dielectrics) | Correct |
| Per-frame CB alignment (#259) | Correct |
| Persistent transfer fence under Mutex (#302) | Correct |
| gpu-allocator block-size tune (#308) — no leak surface | Correct |
| Resource Drop ordering (#289 device-local + M36 + #302 + #294) | Correct |
| 6-attachment G-buffer wiring end-to-end | Correct |
| RESET_COMMAND_BUFFER pool flag + transient pool separation | Correct |
| Subpass dependency masks for `discard` shaders | Correct |

---

Suggested next step:

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-04-14.md
```
