# #4831: REN-D2-2026-09-24-01: `depthLinearize` is fed the authored-fog near/far as if they were the projection planes — the #4588 caustic gate is only 3 % when fog near ≈ 0, and composite's froxel bilateral is inert then (regression of #4588)
state: OPEN  labels: ['bug', 'renderer', 'medium', 'shaders']

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D2-2026-09-24-01**._

_MERGED: D2-01 = D3-01 = D7-01 = D8-02_

**Regression of #4588** (closed): the fix landed but the defect returns — see Status below.

- **Severity**: MEDIUM. Visual artifact only, value-dependent on authored fog. Four independent auditors reproduced the numbers.
- **Dimension**: Caustics (site A) / Denoiser/Composite (site B)
- **Location**:
  - Site A: `crates/renderer/shaders/caustic_splat.comp` `main` (the glass-caustic receiver occlusion gate: `nearPlane = max(screen.z, 1.0e-4)`, `farPlane = max(screen.w, nearPlane + 1.0)`, `depthLinearize(receiverZ, …)`).
  - Site B: `crates/renderer/shaders/composite.frag` `linearViewDepth` (`params.fog_params.x/.y`) feeding `froxelColumnDepthWeight` and the underwater-shaft `underwaterDistance`.
  - Vacuous pin: `crates/renderer/src/vulkan/caustic.rs` `glass_occlusion_gate_tests::the_linearized_gate_catches_occluders_the_ndc_slop_missed` (the mirror `rejects()` encodes and decodes with the same true planes).
- **Status**: Site A = **Regression of #4588** (the fix landed with the wrong plane source, so the defect returns whenever fog near ≠ ~0). Site B = NEW (introduced 2026-08-17..19 with `linearViewDepth`, never audited; #3308 only made the decode convention-aware). The orchestrator re-read both decode sites.
- **Description**: `depthLinearize(z, n, f) = n / (1 - z (1 - n/f))` inverts the *projection's* depth encoding and is only meaningful with the camera projection's near and far.
  - `CameraUBO.screen` is `[w, h, fog_near, fog_far]` (`assemble_camera_and_lights.rs`), and `CompositeParams.fog_params` is `[fog_near, fog_far, fog_clip, fog_power]` (`draw.rs` `build_composite_params`, pinned by a test).
  - Both come from the cell/weather fog record (`render/mod.rs`, `cell_lit.fog_near.max(0.0)` / `fog_far`; 0/0 with no lighting; `env_translate` fallback 15000/80000).
  - The real projection is near = `NEAR_PLANE_BU_SCALE` 5 BU, far = `DEFAULT_RENDER_DISTANCE` 400,000 BU, conventional depth. `GpuCamera` carries no projection near/far.
  - The #4588 commit message justifies the choice with "the same fog-ramp linearization range composite.frag's depthLinearize call uses". That precedent is itself the same fault.
- **Evidence** (formula evaluation of the exact GLSL, not a GPU measurement). Caustic gate: minimum in-front fraction of the receiver distance before the gate rejects, intended 3 %:

  | fog (near, far) BU | 50 BU | 300 BU | 1000 BU |
  |---|---|---|---|
  | fog near = 0, or true planes | 3.0 % | 3.0 % | 3.0 % |
  | (64, 4000) | 3.5 % | 5.8 % | 11.6 % |
  | (100, 2000) | 4.5 % | 11.4 % | 26.3 % |
  | (1500, 9000) | 8.5 % (100 BU: 13.4 %) | 28.7 % | 55.9 % |
  | pre-#4588 NDC slop | 4.4 % | 23.1 % | 50.4 % |

  For fog near ≥ ~1000 the "fixed" gate is looser than the slop it replaced. Composite site: `tolerance = max(2.0, 0.01 * reference)`. With fog near 0, every distance up to 6000 BU decodes to ≤ ~0.12, far below the 2.0 floor, so `froxelColumnDepthWeight` is 1.0 for every surface/surface pair. That is the doorway-leak guard the function exists for. With fog near > 0, distances compress toward `fog_far` and a 2× depth step is accepted at (300, 3000) and at the (15000, 80000) fallback. The underwater-shaft depth ramp is ~0 with fog near 0 and ~1.0 at almost any range with fog near > 0.
  - Real-data census (raw ESM scan by the Dim 8 auditor): FNV `CELL XCLL` (n = 388) near median 64, p90 2000, 12 % have near ≤ 0; Skyrim.esm `CELL XCLL` (n = 590) 58 % near ≤ 0; FNV WTHR day (n = 63) 46 % ≤ 0. So the composite bilateral is inert on roughly 58 % of Skyrim interiors, 12 % of FNV interiors and 46–67 % of day weathers.
  - `water.frag`, `volumetrics_inject.comp`, `volumetrics_integrate.comp`, `cluster_cull.comp` and `clusters.glsl` are **not** affected: they use world reconstruction, or use `screen.w` deliberately as a range.
- **Impact**: (a) caustics splat onto nearer occluders, by as much as tens of percent of the distance in cells with authored fog near (bottle in front of its own pool); (b) volumetric in-scatter and, since `0572bfd5a`, the new interior godrays bleed across depth edges at the 4–8 px froxel footprint; (c) exterior underwater shafts are wrong-depth or absent. Behaviour varies per cell and weather, and `cargo test` cannot see it.
- **Related**: #4588 (closed), #3308, `0ff7b5377` / `3b6ef2e53` (composite origins), `cluster_cull.comp` `clusterFar` (correct handling of the same lane).
- **Suggested Fix**: One owner fixes both shaders in one change. Stop decoding a device depth with planes the pass does not have: compare world distances (`caustic_splat.comp` has `reconstructWorldPos` and `cameraPos`; `composite.frag` already reconstructs `worldPos` from `inv_view_proj`), or add the projection near/far to `GpuCamera` / `CompositeParams` (`caustic_flags.yzw` are declared reserved, so no size change there) with lockstep pins. Replace the CPU mirror with one that decodes through the same inputs the shader receives, over real fog pairs. With a correct decode the 1 % bilateral tolerance may start rejecting shallow-angle surfaces and need retuning (the Dim 7 auditor's unmeasured estimate). Needs a live A/B (see *Needs-RenderDoc*).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)


---

# #4832: REN-D2-2026-09-24-02: the window-portal grazing gate is sign-inverted against its own ray, and the sky sample added in `0572bfd5a` inherits it
state: OPEN  labels: ['bug', 'renderer', 'medium', 'shaders']

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D2-2026-09-24-02**._

- **Severity**: MEDIUM. Visual only; the analytic sign relation is certain, the live effect is unverified.
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag` — `main`, the `if (isWindow && reflectionGlassRayEnabled)` block: `float windowFacing = dot(-V, N); if (windowFacing > 0.1) { vec3 throughDir = -N; … }` and, inside `if (!hitsInterior)`, `exteriorSkyRadianceOr(-N, exteriorSkyTint.rgb)`.
- **Status**: NEW. #421 changed the ray to `-N`, #821 documented the raw-`N` asymmetry, #3323 fed the live exterior tint; none re-derived the sign. The `exteriorSkyRadianceOr(-N, …)` direction is new in `0572bfd5a`. The orchestrator re-read the gate and the surrounding comments.
- **Description**: `V = normalize(cameraPos.xyz - fragWorldPos)` points *toward* the camera. Visible fragments have `dot(N, V) > 0`, and `if (!gl_FrontFacing) N = -N` flips back faces toward the viewer. The portal gate accepts only `dot(-V, N) > 0.1`, i.e. `N` facing away from the camera.
  - For an accepted fragment `throughDir = -N` has `dot(throughDir, V) = windowFacing > 0.1`, so the ray leaves the pane on the **camera's** side, back into the room. That is the opposite of the comments ("`-N` fires straight through the glass plane to the outside", "guarantees raw `-N` always points away from the camera").
  - A normally oriented visible pane (`N` toward the camera) fails the gate, sets `hitsInterior = true` and is demoted to the IOR path. The panes that pass are inverted-normal ones, whose ray runs into the room and again reports `hitsInterior`.
  - Downstream, the IOR path's interior refraction miss is cell ambient (`sceneFlags.yzw`), so a demoted pane shows ambient rather than the outdoor sky the portal exists to transmit.
- **Impact**: Interior windows do not show the live TOD sky or, since `0572bfd5a`, the baked cubemap sun, horizon and clouds. `traceArchitecturalWindowGlass` in `volumetrics_inject.comp` is documented as "the same render-layer criterion as triangle.frag's window portal", so godrays now light the room through a pane that does not itself transmit that light.
- **Conflict to resolve on a device.** The Dim 10 auditor notes that #3323 cites Vault 21/34/22 as working. The Dim 2 auditor traced that claim to a source reading, not a capture. The algebra above supports Dim 2; it is filed as needs-capture.
- **Related**: #421, #821, #925, #3323, #1125.
- **Suggested Fix**: Orient the ray from the viewer, not from the authored normal sign: `float facing = dot(N, V); if (abs(facing) > 0.1) { vec3 throughDir = facing > 0.0 ? -N : N; … }`, and sample the sky cube along that same `throughDir` (hoist it out of the inner block). **Needs a live capture** (FNV Novac motel room or Vault 21 window, Skyrim inn window) before and after, and a source pin on the orientation.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

