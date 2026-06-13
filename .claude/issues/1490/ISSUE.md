## Finding REN2-05 — Renderer Audit 2026-06-11

- **Severity**: MEDIUM
- **Dimension**: Denoiser & Composite (Dims 6, 10, 15 converged; canonical Dim 10)
- **Location**: `crates/renderer/shaders/composite.frag:104-118` (consumers: sky `:320`, aerial-perspective haze `:559`)
- **Status**: NEW (pre-existing bug; the camera-relative cascade shrank it from ~30° at Markarth coordinates to ≤~1.35° but made it discontinuous). Validated CONFIRMED at HEAD `1e8a25ab`.

## Description

`screen_to_world_dir` returns `normalize(world.xyz / w)` — direction from the coordinate-space origin, not from the camera (`normalize(P_far − camera_pos)`). The `/ w` is only the perspective-divide guard from #926; no `- params.camera_pos.xyz` subtraction exists, though `camera_pos` is declared in the same UBO (`:46`) and used elsewhere (`:401`, `:539`). With the relative `inv_view_proj`, the camera sits up to ~7094 u from the relative origin against a 300000 far plane → up to ~1.35° skew (~75% of the sun disc's angular radius), varying continuously with camera position and jumping at every 4096-unit origin snap.

## Impact

Sky-dome swim under camera translation, sun disc misaligned vs the `sun_dir` used for shadows, near-horizon cloud projection error, one-frame sky/haze pop per grid crossing. Exterior-only; no geometry-lighting or SVGF impact.

## Suggested Fix

One line: `return normalize(world.xyz / w - params.camera_pos.xyz);` — `params.camera_pos` (already relative, same UBO) is the matching origin. Recompile `composite.frag.spv`.

## Related

REN2-04, REN2-07(e) (the `camera_pos` doc comment at `composite.frag:46`).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files — other `inv_view_proj` direction reconstructions (ssao.comp, volumetrics)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer
- [ ] **TESTS**: Regression test added for this specific fix

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
