# Fallout New Vegas Compatibility Audit — Dimension 4: RT Lighting Pipeline (2026-05-24)

Focused sweep — only Dimension 4 (RT Lighting Pipeline). FNV is the reference title, so this dimension is a regression guard. The 2026-05-22 FNV audit deferred Dim 4 to the renderer audit and reported no regressions; this sweep checks against the heavy renderer churn since (Disney BSDF port #1248-#1252, today's sun-sprite mip 0 fix, water-caustic accumulator #1257, skyTint interior gate #1125).

## Executive Summary

- **0 findings**. All Dim 4 baselines verified clean.
- Disney BSDF port (#1248-#1252) is **correctly gated** on `MAT_FLAG_BGSM_PBR` at all 4 fragment-shader sites — FNV legacy materials (no BGSM authoring) bypass the new lobe and use the existing Lambert path. No regression risk for FNV scenes.
- 8 RT-pipeline contracts verified live in current source (see Verifications).
- One **informational note** on audit-prompt drift: the prompt mentions "1 GB BLAS budget" but the actual budget is dynamic `device_local_bytes / 3` with a 256 MB floor. Not a code finding — note for the next prompt refresh.

| Severity | NEW | Carryover | Total |
|----------|-----|-----------|-------|
| HIGH     | 0   | 0         | 0     |
| MEDIUM   | 0   | 0         | 0     |
| LOW      | 0   | 0         | 0     |

## Method

The 2026-05-12 → 2026-05-24 renderer churn touched many surfaces this dimension cares about:

| Domain | Commits (since 2026-05-10) | Risk to FNV |
|---|---|---|
| Disney BSDF port | `#1248`, `#1249`, `#1250`, `#1252`, `#1253/#1254` | HIGH if gating fails — FNV materials are pre-PBR |
| TLAS lifecycle | `#1142`, `#1144`, `#1145`, `#1226`, `#1228` | MEDIUM — could regress BLAS budget / refit |
| Skin compute + BLAS refit gate | `#1195`, `#1196`, `#1197` | LOW — paired gate is conservative |
| Sky / clouds | `#1125`, `#1147`, today's `8b5d77c1` (sun-sprite mip 0) | LOW — gating preserves interior path |
| Shader debug / line anchors | `#1158`, `#1162`, `#1190` | None (cosmetic) |

Verified each high-risk site against current source.

## Verifications (8 RT-pipeline contracts, all CLEAN)

### V1 — M31.5 Streaming RIS reservoir count + W clamp

- **Site**: `crates/renderer/shaders/triangle.frag:2484-2488`
- **Expected**: `NUM_RESERVOIRS = 8`, `RESERVOIR_W_CLAMP = 64.0`
- **Found**: `const uint NUM_RESERVOIRS = 8;` + `const float RESERVOIR_W_CLAMP = 64.0;`
- ✓ Both constants exact-match the M31.5 baseline. Clamp engaged at `triangle.frag:2796` in the unbiased W estimator. **No regression.**

### V2 — Glass-ray budget cap

- **Site**: `crates/renderer/shaders/include/shader_constants.glsl:27`, enforcement at `triangle.frag:1901`
- **Expected**: `GLASS_RAY_BUDGET = 8192` with per-fragment atomic counter; over-budget fragments fall back to non-RT glass.
- **Found**: `#define GLASS_RAY_BUDGET 8192u` + `glassIORAllowed = (old + GLASS_RAY_COST <= GLASS_RAY_BUDGET);`
- ✓ Wired. **No regression.**

### V3 — Disney BSDF gating (highest-risk for FNV)

- **Sites** (3 gate locations in `triangle.frag`; original 2026-05-24 draft incorrectly counted 4 — corrected per `AUDIT_RENDERER_2026-05-24_DIM21.md` finding DIM21-NEW-02):
  - `:1652` — F0 derivation gate: authored specular for PBR vs metallic-workflow F0 for legacy
  - `:2447-2449` — `disneyDiffuseSplit(...)` consumer in the main `diffuseBrdf` branch
  - `:2669-2671` — second `disneyDiffuseSplit(...)` consumer in the deferred-specular path
- **Flag definition**: `MAT_FLAG_BGSM_PBR (1u << 5)` at `triangle.frag:182`
- **Comment at `:2436`**: *"Lambert for legacy NIF. Gated on MAT_FLAG_BGSM_PBR so legacy materials bypass the new lobe."*
- **FNV material authoring**: BGSM/BGEM is FO4+ format; FNV ships .nif material properties only, no BGSM. `material_kind` for FNV materials never sets `MAT_FLAG_BGSM_PBR`. Disney path is unreachable for FNV.
- ✓ **No regression risk for FNV.** The Disney port + per-material IOR Fresnel + Burley diffuse + HK subsurface + anisotropic GGX is all FO4-and-later territory; FNV gets the original Lambert + GGX path it has always used.

### V4 — #1125 skyTint interior-cell gate

- **Sites**:
  - `crates/renderer/shaders/triangle.frag:527-531` — RT reflection miss fallback
  - `triangle.frag:2141-2144` — RT refraction miss fallback
  - `triangle.frag:208` — `jitter.w` carries `is_exterior` flag
  - `crates/renderer/shaders/composite.frag:36` — `depth_params.x` mirrors the flag for composite sky
- **Expected**: Interior cells (sealed rooms — Prospector Saloon, Vault interiors) must NOT read default clear-noon-blue skyTint as a "ceiling colour"; they get cell ambient (`sceneFlags.yzw`) alone.
- **Found**: Both miss fallbacks identical: `isExterior ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw`. Comments document #1125 / REN-D9-NEW-01 closure.
- ✓ FNV's heavy interior surface (Prospector Saloon 809 entities, Doc Mitchell's House, every Vault interior) gets correct cell ambient on glass refractions / reflections. **No regression.**

### V5 — Today's sun-sprite mip 0 fix (commit `8b5d77c1`)

- **Site**: `crates/renderer/shaders/composite.frag:281`
- **Found**: `vec4 sprite = textureLod(textures[nonuniformEXT(sun_tex_idx)], uv, 0.0);`
- **What this fixes**: BSA-extracted sun sprites carry mipmaps; without the explicit `0.0` LOD, the driver's `dFdx/dFdy` on the sky-disc UV gradient picks a high mip (the sun is a tiny screen-space feature with steep gradients), producing a pixelated sun disc. Force-mip-0 sample restores the full-resolution sun sprite.
- ✓ Wired. FNV's daytime exterior (WastelandNV grid) renders crisp sun disc again. **Today's improvement.**

### V6 — SVGF mesh_id disocclusion + motion-vector reprojection

- **Sites**:
  - `crates/renderer/shaders/svgf_temporal.comp:88` — `mesh_id 0 = sky/clear; no accumulation needed`
  - `:128` — bilinear history taps weighted by mesh_id match
  - `:156-159` — same-mesh disocclusion rejection (#650 / SH-5)
  - `:49-52` — motion-vector RG16_SNORM reprojection
- **Expected**: SVGF rejects history when `mesh_id` differs across frames; alpha-blend draws (bit 31 set) opt out of history entirely.
- **Found**: All wired. Bit 31 alpha-blend opt-out at `triangle.frag` (mesh_id encoding) → consumed correctly by SVGF.
- ✓ **No regression.**

### V7 — TAA disocclusion + alpha-blend opt-out

- **Site**: `crates/renderer/shaders/taa.comp:147-158`
- **Expected**: `disocclusion = ((currMid & 0x7FFFFFFFu) != (prevMid & 0x7FFFFFFFu))` (strip alpha-bit before compare); alpha-blend fragments (bit 31) opt out of history; off-screen fragments opt out.
- **Found**: All three gates wired (`offscreen || disocclusion || alphaBlend`).
- ✓ Halton(2,3) jitter + YCoCg γ=1.5 clamp + α=0.1 blend all preserved (verified in 2026-05-24 Dim 7 perf audit). **No regression.**

### V8 — BLAS budget dynamic, not the prompt's stale "1 GB"

- **Site**: `crates/renderer/src/vulkan/acceleration/predicates.rs:455`
- **Expected (per audit prompt)**: 1 GB BLAS budget
- **Actual**: `(device_local_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)` where `MIN_BLAS_BUDGET_BYTES = 256 MB` (constants.rs:61). On the dev GPU (RTX 4070 Ti, 12 GB VRAM) that's **~4 GB**, not 1 GB.
- **Log line at acceleration/mod.rs:203**: `"BLAS memory budget: {} MB (derived from VRAM); ..."`
- ✓ Code is correct. The audit prompt's "1 GB" figure is stale documentation — note for prompt refresh, not a code finding.

## Baseline Comparison

| Metric | Roadmap claim | Verified today | Status |
|---|---|---|---|
| NUM_RESERVOIRS (RIS) | 8 | 8 | ✓ |
| RESERVOIR_W_CLAMP | 64.0 | 64.0 | ✓ |
| GLASS_RAY_BUDGET | 8192 | 8192 | ✓ |
| BLAS budget | 1 GB (audit prompt) | device_local/3, min 256 MB | code correct, prompt stale |
| TAA jitter pattern | Halton(2,3) period 16 | Halton(2,3) period 16 | ✓ (per Dim 7 perf audit) |
| TAA YCoCg γ | 1.5 (post-#1108) | 1.5 | ✓ |
| Disney BSDF gate | MAT_FLAG_BGSM_PBR only | MAT_FLAG_BGSM_PBR at 3 sites | ✓ |
| skyTint interior fallback | cell ambient alone (#1125) | `sceneFlags.yzw` at 2 sites | ✓ |
| SVGF disocclusion | mesh_id + motion vector | mesh_id + motion vector | ✓ |
| TAA alpha-blend opt-out | bit 31 of mesh_id | bit 31 of mesh_id | ✓ |

## Regression Guard List

Confirmed still correct in current source:

- **#1125** — skyTint interior gate (2 sites)
- **#1248** — Fresnel F0 from per-material IOR (PBR-gated, FNV bypasses)
- **#1249** — Disney diffuse port (PBR-gated, FNV bypasses)
- **#1250** — anisotropic GGX (PBR-gated, FNV bypasses)
- **#1252** — Disney diffuse split (PBR-gated, FNV bypasses)
- **#1253/#1254** — input-domain clamps on dielectricF0FromIor + deriveAxAy (defensive, no FNV impact)
- **#1147 Phase 2b** — PBR / SSS / model-space-normals gating in triangle.frag
- **#1190 (TD4-NEW-01)** — `inst.flags` / MAT_FLAG_* bits routed through generated shader include — bit definitions stay in lockstep
- **#1226** — TLAS-scratch shrink (was dead code, now wired and BLAS-budget-correlated)
- **#1228** — `missing_blas` counter split by cause (skinned / rigid / ssbo_evicted)
- **8b5d77c1** — sun-sprite mip 0 (today)

## Notes

- This dimension came out of the heaviest 12-day renderer churn (Disney BSDF, water caustics, multi-phase #1210, today's sun fix) without a single FNV regression. The gating discipline (`MAT_FLAG_BGSM_PBR` + `is_exterior`) is doing real work — every new PBR path is one if-statement away from accidentally lighting FNV scenes with the wrong lobe, and that didn't happen.
- The audit prompt's "BLAS budget: 1 GB" line is the only drift — informational for the next prompt refresh. The code is correct (dynamic, VRAM-derived).
- M22 (shadows + reflections + 1-bounce GI) + M37.5 (TAA) + M33 (sky/clouds) + M31.5 (streaming RIS) all verified live and unregressed against this dimension's baseline.
- No new finding warrants `/audit-publish`. This is a clean regression-guard pass.
