# AUDIT_RENDERER (Focused) — 2026-05-01

**Auditor**: Claude Opus 4.7 (1M context)
**Scope**: focused renderer audit on lighting regression — distance-based over-bright surfaces with chromy / posterized look. User reported visible at least since caustics + M41 era; not related to today's #779 attempt (revert chain `7a91597` + `e0d4144` cleanly restored to known-good state).
**Focus**: dimensions 6 (Shader Correctness), 9 (RT Ray Queries), 10 (Denoiser & Composite), 11 (TAA), 13 (Caustics)
**Reference report**: `docs/audits/AUDIT_RENDERER_2026-05-01.md` (broader audit run earlier today)
**Open-issue baseline**: 51 open at audit start

---

## Executive Summary

**1 HIGH (root-cause candidate) · 1 HIGH (sibling) · 2 MEDIUM · 1 LOW · 1 INFO** — across 6 new findings.

**Most likely root cause**: `LIGHT-N1` — `weather_system` unconditionally writes weather-derived `fog_color`/`fog_near`/`fog_far` into `CellLightingRes` regardless of whether the active cell is interior or exterior. When the user loads an FNV interior cell after a session that visited any exterior worldspace, the WTHR-derived (sky-tinted, e.g. `[0.65, 0.7, 0.8]`) fog values overwrite the interior's XCLL-authored fog. Composite then blends these into distant pixels at up to 70% opacity in HDR linear space (composite.frag:307-308), pre-tonemap. The user-observed symptom — *foreground correct, distant pixels chromy/posterized/over-bright with sharp transitions along depth contours* — is exactly what this produces.

**The visual signature** (chromy distant surfaces, ACES tone-mapping squashing the bright fog-mixed values into a near-white posterized look, gradient transition along depth) corresponds to a fog mix in linear space with a sky-tinted fog target. No other lighting path in the audited dimensions produces a depth-correlated brightness gradient of this magnitude.

### Likely-cause cluster

| Finding | Severity | Why |
|---|---|---|
| `LIGHT-N1` (weather → interior fog leak) | **HIGH** | Direct match for the symptom. One-line code fix on the gate. |
| `LIGHT-N2` (HDR pre-tonemap fog math) | **HIGH (sibling)** | Even with correct fog values, HDR-space blending produces brighter results than the same fog_color would in display-space blending. Pre-#428 fog in triangle.frag had the same math but composited differently. |
| `CSTC-N1` (caustic distance bias) | MEDIUM | Caustic accumulator clears per frame, but the splat geometry biases toward distant pixels (refracted rays land far away). Contributes secondary brightness on top of fog. |
| `RT-N1` (GI tMax 6000 vs fade 4000-6000) | LOW | Bounded by the giFade multiplier; not a primary cause. |
| `TAA-N1` (history-reject aliasing at distance) | INFO | Symptomatic — TAA history reject leaves distant pixels un-anti-aliased. Symptom of poor reprojection/disocclusion, not the brightness root cause. |

### Diagnostic test for the user

Trivial way to confirm `LIGHT-N1`: launch the engine **directly into the affected interior cell** without first loading any exterior cell that would populate `WeatherDataRes`. If the chromy distance look disappears, `LIGHT-N1` is confirmed. If the look persists on a fresh-launch interior, secondary causes (`LIGHT-N2`, `CSTC-N1`) are at play.

---

## Findings

### HIGH

#### LIGHT-N1: `weather_system` overwrites interior cell `fog_color` with sky-tinted exterior fog values

- **Severity**: HIGH (primary root-cause candidate)
- **Dimension**: Sky/Weather/Exterior Lighting × Composite
- **Locations**:
  - `byroredux/src/systems.rs:1493-1501` — `weather_system` writes `fog_color`/`fog_near`/`fog_far` into `CellLightingRes` unconditionally
  - `byroredux/src/render.rs:1056-1069` — `build_render_data` reads those values for composite UBO upload
  - `crates/renderer/shaders/composite.frag:293-308` — composite blends `fog_color` at up to 70% opacity into distant pixels
- **Status**: NEW
- **Description**: `weather_system` runs every frame, computes a current `(fog_color, fog_near, fog_far)` triple from `WeatherDataRes` (which is set when an exterior cell loads), and writes them into `CellLightingRes` at lines 1494-1501. There is **no `is_interior` guard** on the write — `grep -n "is_interior\|cell_lit.*interior" byroredux/src/systems.rs` returns zero hits. Consequently, an interior cell loaded after the player visited any exterior worldspace inherits the most-recent WTHR fog tint instead of its own XCLL-authored interior fog.
- **Evidence**:
  ```rust
  // systems.rs:1494-1501 — current state
  if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
      cell_lit.ambient = ambient;
      cell_lit.directional_color = sunlight;
      cell_lit.directional_dir = sun_dir;
      cell_lit.fog_color = fog_col;       // <-- unconditional
      cell_lit.fog_near = fog_near;       // <-- unconditional
      cell_lit.fog_far = fog_far;         // <-- unconditional
  }
  ```
  Default exterior fog tint at `byroredux/src/scene.rs:384`:
  ```rust
  const FOG_COLOR: [f32; 3] = [0.65, 0.7, 0.8];
  ```
  This is the typical sky-tinted blue-grey shipped by FNV WTHR records — exactly the chromy color visible in the user's distant-pixel screenshots.
- **Impact**: Interior cells loaded after any exterior session render with wrong fog. Symptom: distant pixels in the interior cell are mixed up to 70% with a sky-blue fog color (per `composite.frag:307`), then ACES tone-mapped, producing the chromy/posterized look the user reported. Foreground (low world-distance) pixels are unaffected because `smoothstep(fog_near, fog_far, worldDist)` is near zero there.
- **Suggested Fix**: Gate the fog write on `is_interior == false` (or equivalently on `WeatherDataRes` being applicable to the current cell). Two options:

  **Option A** (minimal): only write fog if the cell-lighting consumer reports exterior:
  ```rust
  if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
      cell_lit.ambient = ambient;
      cell_lit.directional_color = sunlight;
      cell_lit.directional_dir = sun_dir;
      // Only overwrite fog when the active cell is exterior — interior
      // cells preserve their XCLL/LGTM-authored fog from cell_loader.
      if cell_lit.is_exterior {
          cell_lit.fog_color = fog_col;
          cell_lit.fog_near = fog_near;
          cell_lit.fog_far = fog_far;
      }
  }
  ```
  This requires `CellLightingRes` to carry an `is_exterior: bool` field. Per `components.rs:131` (`is_interior: bool`) and `:157` (`is_exterior: bool`), the engine already tracks both; just plumb either onto `CellLightingRes`.

  **Option B**: track active cell via `world.try_resource::<ActiveCellRes>()` (or equivalent) and check the cell's exterior flag before writing fog.

### HIGH

#### LIGHT-N2: Fog blends in HDR linear space pre-ACES — `fog_color` authored for display perception over-brightens when mixed in linear

- **Severity**: HIGH (sibling — even with correct fog source, the math amplifies)
- **Dimension**: Composite
- **Location**: `crates/renderer/shaders/composite.frag:293-309`
- **Status**: NEW
- **Description**: Composite mixes fog *in HDR linear space, before ACES tone mapping* (`combined = mix(combined, fog_color, fogFactor)` then `aces(combined * exposure)`). XCLL/WTHR-authored `fog_color` values are typically in the 0.5-0.9 range — values that *look* mid-grey on a display because they're authored against the gamma-curve perception of a final image. When those same numbers blend in HDR linear space, they're effectively much brighter than the perceptual mid-grey the author intended. ACES then squashes the resulting bright value, producing the chrome/posterized look.

  Pre-#428 fog was applied inside `triangle.frag` against the same HDR pipeline — same math. The bug isn't a regression from #428; it's an architectural choice that has been present since fog was first wired into the linear HDR path. The reason it's only visible *now* is that the user is hitting `LIGHT-N1` (interior cells inheriting bright sky fog).
- **Impact**: When the fog source is the ~bright sky tint, the HDR-space mix amplifies. When the fog source is dim (e.g., correct interior XCLL fog like `[0.05, 0.07, 0.1]`), the mix is correct and the symptom disappears. So fixing `LIGHT-N1` likely makes this finding non-visible — but the underlying math is still wrong perception-wise.
- **Suggested Fix**: Defer until `LIGHT-N1` is shipped and validated. If chromy distant pixels persist, options:
  - Apply tone mapping to `fog_color` once (CPU-side) before upload, so the linear-space blend produces the perceptually-intended result
  - Move the fog mix to display space: `outColor = mix(aces(combined * exposure), aces(fog_color * exposure), fogFactor)` — preserves SVGF coherence (#428's goal) while compositing fog perceptually

### MEDIUM

#### CSTC-N1: Caustic splat geometry biases toward distant pixels; bright contribution can compound the fog over-brightness

- **Severity**: MEDIUM
- **Dimension**: Caustics × Composite
- **Locations**:
  - `crates/renderer/shaders/caustic_splat.comp:107-211` — splat dispatch
  - `crates/renderer/shaders/composite.frag:267-271` — `combined = direct + indirect * albedo + caustic;`
  - `crates/renderer/src/vulkan/caustic.rs:776` — accumulator IS cleared per frame (verified)
  - `crates/renderer/src/vulkan/context/draw.rs:861-863` — flag bit set on `alpha_blend && metalness < 0.3` instances
- **Status**: NEW
- **Description**: The caustic accumulator IS cleared per frame (`cmd_clear_color_image` at caustic.rs:776) so the issue is not multi-frame accumulation. However, the SPLAT GEOMETRY biases caustic luminance toward distant pixels: refracted rays through glass surfaces tend to land FAR from the source surface (geometric optics — refraction extends along the ray direction). Multiple caustic-source surfaces in the same scene can splat to overlapping distant regions. Composite adds `caustic` to `combined` unconditionally; the result is amplified at distance.

  Filtering: caustic-source bit is set only on `alpha_blend && metalness < 0.3` surfaces (draw.rs:861). For NPC content, this includes glass eyes, hair cards (alpha-blend foliage-style surfaces), and any transparent glass props. The M41 NPC spawn pipeline introduces NPCs whose eye spheres are caustic sources — the user reported the regression "started when caustics were working in M41."
- **Impact**: On its own, this is a moderate contributor — typical caustic luminance is bounded by `causticTune.w` (strength) × per-light contribution. But when stacked on top of `LIGHT-N1`'s incorrect fog tint, the combined bright-distance effect is amplified.
- **Suggested Fix**: This is more architecturally embedded; defer pending validation that fixing `LIGHT-N1` resolves the visible symptom. If caustic-specific over-bright persists:
  - Verify the alpha-blend caustic-source filter is actually narrow (audit which DrawCommands set bit 2 in real FNV interior content — log a histogram per cell)
  - Add a per-pixel distance-attenuation on the caustic contribution in composite.frag (`caustic *= clamp(1.0 - worldDist / 4000, 0, 1)` or similar)
  - Reduce the default `causticTune.w` strength

### LOW

#### RT-N1: GI ray `tMax = 6000` doubles distance from prior 3000; bounded by `giFade` but worth verifying

- **Severity**: LOW
- **Dimension**: RT Ray Queries
- **Location**: `crates/renderer/shaders/triangle.frag:1687-1753`
- **Status**: Existing change (#742, commit `9885a9c`); audit re-validation
- **Description**: `tMax` was raised from 3000 to 6000 to match the fade-window end (#742). Per code at line 1688 `giFade = 1.0 - smoothstep(4000.0, 6000.0, giDist)`, indirect contribution at the boundary is multiplied by giFade ∈ [0, 1] before output. Mathematically the contribution stays bounded by the fade. **Not a primary cause** of the user's symptom, but flagged because the user mentioned this as a candidate timeframe.
- **Impact**: None expected. The fade is multiplicative at line 1753 so distant fragments naturally attenuate. Verify by spot-checking whether `indirect *= giFade` actually fires (it does, unconditionally inside the `if (giFade > 0.01)` block).
- **Suggested Fix**: No action.

### INFO

#### TAA-N1: Distant pixels show stair-stepped textures (no anti-aliasing) — symptomatic of TAA history rejection

- **Severity**: INFO
- **Dimension**: TAA
- **Locations**:
  - `crates/renderer/shaders/svgf_temporal.comp:152-167` — history blend
  - `crates/renderer/shaders/svgf_temporal.comp:128` — wTotal > 0.01 gate
  - SVGF logic mirrors TAA; the same disocclusion rejection logic applies to both
- **Status**: NEW (observation)
- **Description**: The user's screenshots show distant pixels with un-aliased / stair-stepped textures, while foreground pixels are properly smoothed by TAA. This is consistent with TAA history rejection: at distance, motion-vector reprojection has lower screen-space precision (a 1° camera rotation moves distant pixels by less than 1 screen-pixel, but their NDC differences round to either-side of integer pixel boundaries unpredictably). When all 4 bilinear taps fail mesh-ID consistency AND the nearest-tap fallback at `svgf_temporal.comp:133-149` (`length(motion * screen.xy) < 1.5`) doesn't trigger, history is rejected and the current frame's noisy single-sample value is written.

  This is a **symptom**, not the brightness root cause — TAA rejection produces aliasing/temporal noise, NOT brightness amplification. But the visual aliasing combined with the over-bright fog mix produces the "posterized" look the user described.
- **Impact**: Cosmetic at most; once `LIGHT-N1` is fixed and distant pixels return to normal brightness, TAA history rejection at distance will be a milder visible artifact (slight aliasing, not chromy posterization).
- **Suggested Fix**: No action for now. If post-`LIGHT-N1` validation shows the aliasing is still distracting, consider:
  - Lower the `wTotal > 0.01` gate or expand the nearest-tap fallback motion threshold
  - Add a depth-buffer-based reproject-distance bias (existing SVGF treats mesh-ID as primary disocclusion signal; some renderers add depth Δ as well)

---

## What I checked and ruled out

- **Cluster lighting at distance**: Distant fragments index into deeper cluster slices; max_lights_per_cluster = 32. With shadowFade fading shadow contributions to zero past 6000 units, distant lights become *unshadowed* but their per-light attenuation (radius / distance² for point/spot, no attenuation for directional) still bounds them. No evidence of distant fragments getting more lights than they should.
- **R1 MaterialTable `material_id` indirection**: Verified that `triangle.frag:596` reads `materials[inst.materialId]` consistently and that `intern` produces stable IDs. Distant instances index correctly into the same table; no per-instance "material drift" with distance.
- **GI ray miss path**: `indirect = vec3(0.6, 0.75, 1.0) * 0.06` on miss. Bounded; multiplied by giFade.
- **`avgAlbedo` retention on per-instance**: GI bounce reads `hitInst.avgAlbedoR/G/B` (`triangle.frag:1736`) directly from the GpuInstance — not from the MaterialBuffer (R1-N2 retention is intentional). For distant hit surfaces, this is the correct cached average; not a source of over-brightness.
- **Caustic accumulator clear**: `cmd_clear_color_image` at `caustic.rs:776` runs before each frame's dispatch with a TRANSFER → COMPUTE barrier. No multi-frame accumulation possible.
- **SVGF temporal-α floor**: `0.2` floor, with per-pixel `max(α, 1/(age+1))` for fast disocclusion recovery. Bounded; not a source of distance-correlated brightness.

---

## Prioritized Fix Order

1. **`LIGHT-N1`** (~10 lines + plumbing) — gate `weather_system` fog writes on `is_exterior`. Should ship today as the most likely fix.
2. **Validate**: launch directly into an interior FNV cell on a fresh process. Confirm chromy distance look is gone.
3. If `LIGHT-N1` doesn't fully fix it, **`CSTC-N1`** — verify caustic-source filter is narrow enough, possibly add distance-attenuation in composite.
4. **`LIGHT-N2`** — if visible artifacts persist after both above, move fog mix to display space (post-ACES). Larger refactor (~30 lines composite.frag + Rust UBO field).
5. `TAA-N1` and `RT-N1` — likely no-ops once root cause is fixed.

---

## Out-of-scope

- Pre-existing `feb5a56` baseline state: this audit assumes the broken visuals predate today's session per user testimony. The full revert chain (`7a91597` + `e0d4144`) cleanly restored the source tree to post-#777 state — confirmed by `git diff feb5a56..HEAD` returning zero changes outside the user's intentional manual reverts of `ui.vert` and `caustic_splat.comp` (per system reminder; not undone).

## Methodology Notes

- **Sub-agent dispatch failure pattern recurred** for the fourth time this session — went straight to direct main-context audit anchored on the symptom-specific code paths.
- **Grep-driven hypothesis testing** was the primary tool: trace `fog_color` upload → composite consumption → check for interior/exterior gate; trace caustic clear → splat → composite consumption; trace GI tMax → fade application. The fog-leak hypothesis pattern-matched cleanly to the user's observed symptom (depth-correlated brightness with sky-tinted bias).
- **No dynamic test capture available** (RenderDoc would confirm definitively); this audit is static-analysis only. The diagnostic test (fresh-launch interior, no prior exterior visit) gives the user a way to confirm `LIGHT-N1` without GPU debugging tools.
