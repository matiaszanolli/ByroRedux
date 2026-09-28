# #4942: REN-D7-2026-09-27-01: The ReSTIR direct-radiance EMA has no light-change invalidation, so flicker, pulse and toggles are low-pass filtered, and its parked mode keys on the bare camera flag, not SVGF's scene-static signal

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4942
- **Labels**: medium,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D7-2026-09-27-01**._

- **Severity**: MEDIUM
- **Dimension**: Light Animation
- **Location**: `crates/renderer/shaders/triangle.frag` — the ReSTIR accumulate block (`bool cameraStatic = dofParams.w > 0.5; float historyCap = cameraStatic ? 64.0 : 16.0; float alphaFloor = cameraStatic ? 0.025 : 0.1;` … `accum = mix(prevAccum, frameContribution, alpha)`). The `reprojValid` acceptance (`sameSurface && rpHistLen > 0.0 …`) is where the history is kept. The flag is fed from `assemble_camera_and_lights.rs` `dof_params: [.., if camera_static {1.0} else {0.0}]`.
- **Status**: NEW. It predates the window (introduced `883f57cd7`, 2026-07-20). Dim 2 (REN-D2-2026-09-27-01) covers the ReSTIR normalisation only; this is a different defect. I searched issues for "restir temporal history", "light flicker", "accum history light" and "historyCap": no match.
- **Description**:
  - The direct term for every shadow-casting light (`if (!useRestir || !needsVisibility) Lo += shadowableRadiance;` routes all `needsVisibility` lights through ReSTIR) is `frameContribution = rad · W · V̄`. That is **radiance**, recomputed each frame from the light's current intensity.
  - It is then EMA-blended with the reprojected `prevAccum`. History is accepted whenever the surface ID, depth and normal match. Nothing compares the light's radiance or intensity to the previous frame, and `LightHistory::remap` deliberately keeps a light's history across colour/position changes (its own test `follows_identity_through_sort_animation_and_motion`).
  - The comment's premise, "once the camera is parked, direct-light noise is the only thing changing on a static receiver", is false for this engine. `systems/light_anim.rs` animates `LightSource.intensity` every frame: `flicker_intensity` uses 6 noise buckets per authored period (the 0.5 s default gives 12 Hz steps), and PULSE uses a sine.
  - The parked mode also reads the **bare** camera flag. SVGF's progressive mode was changed under #4046 to AND in `caustic_scene_static` (light rig included), so SVGF indirect drops its long history the frame a light changes, while ReSTIR direct keeps a 64-frame history with a 0.025 floor.
  - The same light change therefore propagates through indirect immediately and through direct over tens of frames.
- **Evidence**:
  - The EMA is a first-order low-pass with gain `α / |1 − (1−α)e^{−iω}|`.
  - A 12 Hz flicker at 60 fps has ω = 2π/5 ≈ 1.257. That gives gain ≈ 0.09 at α = 0.1 (camera moving) and ≈ 0.02 at α = 0.025 (parked). The flicker amplitude reaching direct lighting is about 9 % (moving) and about 2 % (parked) of the authored modulation.
  - A light switched off or dimmed while the camera is parked fades with τ ≈ 1/0.025 = 40 frames.
  - TAA (taa mode) adds α = 0.1 on top, but its 3×3 clamp follows a global brightness change quickly, so the ReSTIR EMA is the dominant attenuator.
- **Impact**: Flickering torches, candles and fires, the signature animated lights of every Bethesda interior, lose most of their flicker in shadowed direct lighting, most visibly when the player stands still. Scripted light toggles smear. Direct and indirect respond to one event on different time constants.
- **Related**: #4046 (SVGF got the scene-static fix), #2516 / #2478 (flicker rate and period plumbing that this filter undoes), REN-D2-2026-09-27-01, `light_history.rs`.
- **Suggested Fix**: Accumulate the **visibility** (the shadow ratio `V̄`, or `frameContribution / unshadowed rad·W`) and multiply by this frame's radiance when shading, so intensity animation passes through while shadow-ray noise is still filtered. Alternatively, reset `histLen` when the selected light's `rpRad` luminance differs from last frame by more than a relative ε. Either way, drive the parked cap from the same scene-static signal SVGF uses.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
