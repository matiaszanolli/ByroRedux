# Renderer Audit — Exterior Focus — 2026-05-03

**Auditor**: Claude Opus 4.7 (1M context)
**Scope**: `--focus 15` (Sky / Weather / Exterior Lighting), extended into adjacent paths the prior renderer audits never touched as a primary topic: directional sun shadow rays at long range, GI miss → sky-color fill, sun arc cardinal orientation, cell-streaming temporal discontinuity, distant LOD batch routing, terrain LAND boundary correctness.
**Why a separate audit**: Per the user's observation, the audit series has been heavily *interior-biased* (every visual issue surfaced via `GSDocMitchellHouse` / `WhiterunBanneredMare`). This pass anchors on the FO3/FNV exterior streaming path the engine *can* load today rather than another interior cell.
**Reference reports**:
- `docs/audits/AUDIT_M33_2026-04-21.md` — last sky/weather audit (4 CRITICAL parser defects, all closed since)
- `docs/audits/AUDIT_RENDERER_2026-05-03.md` — general renderer audit (Dim 15 sub-section)
- `docs/audits/AUDIT_RENDERER_2026-04-27.md` — backlog reference
**Open-issue baseline**: 51 open at audit start (`/tmp/audit/renderer/issues.json`).
**Method**: Direct main-context delta audit. Sub-agent dispatches deliberately not used per established methodology (see prior audits). Verified each finding against current code before classifying.

---

## Executive Summary

**0 CRITICAL · 1 HIGH · 3 MEDIUM · 2 LOW · 2 INFO** — across 8 new findings, plus 5 carry-over confirmations from prior audits.

The good news from the parser side: every one of the 4 CRITICAL findings from the 2026-04-21 M33 audit (NAM0 size gate, cloud-texture FourCC, DNAM-as-speeds, FNAM-empty) has been closed. WTHR loads correctly across FNV / FO3 / Oblivion. Cloud textures, fog distances, classification flags all decode against authoritative byte layouts.

The findings concentrate on the **runtime side that 04-21 didn't audit**:

1. **`SUN-N1` (HIGH)** — Directional sun light ignores `sun_intensity` ramp; at night, surfaces (especially ceilings / overhangs) still receive non-zero "sun light" from the (0,-1,0) hardcoded night sun direction.
2. **`SUN-N3`, `SUN-N4` (MEDIUM)** — Sun glow halo + sun disc both render below-horizon at sunset / sunrise without elevation gating. `sun_intensity` doesn't multiply the glow term.
3. **`STRM-N1` (MEDIUM)** — Cell streaming doesn't call `signal_temporal_discontinuity` — newly-streamed geometry ghosts for ~5 frames as SVGF history bleeds over it.
4. **`SUN-N2` (LOW)** — Sun arc tilts NORTH (`z = -0.15`) instead of SOUTH for the NH-latitude Bethesda settings (Mojave / Tamriel / Capital Wasteland are all temperate-NH analogs). Cosmetic; one-character fix.
5. **`STRM-N2` (LOW)** — Cloud scroll accumulator resets to zero on every interior→exterior re-entry; clouds visibly "jump" back to origin position.
6. **`STAT-N1`, `STAT-N2` (INFO)** — Distant LOD batches (BSPackedCombined*) and SpeedTree wind animation parse but have no consumer — known M35 / future work; flagged for tracking.

**The headline carryover** is `#671 RT-8` (GI miss → hardcoded sky color), which was on the 04-27 backlog list and is the one item that would *most visibly* improve exterior rendering quality. The hardcoded `vec3(0.6, 0.75, 1.0) * 0.06` at `triangle.frag:2126` is independent of TOD, per-cell ambient, weather, or sun direction — at midnight in a clear cell, the engine still adds a constant blue tint to every GI-miss fragment. This is the corrective lever for "exteriors look correct at all times of day."

| Sev | Count | NEW IDs |
|--|--:|--|
| CRITICAL | 0 | — |
| HIGH | 1 | SUN-N1 |
| MEDIUM | 3 | SUN-N3 · SUN-N4 · STRM-N1 |
| LOW | 2 | SUN-N2 · STRM-N2 |
| INFO | 2 | STAT-N1 · STAT-N2 |

### Carryovers from prior audits — exterior-relevant, all still open

| ID | Issue | Site | Status |
|---|---|---|---|
| `#671` / RT-8 | GI miss hardcoded sky color | `triangle.frag:2126` | Still open. Re-confirmed: `vec3(0.6, 0.75, 1.0) * 0.06` literal. |
| `RT-14` | GI ray tMax (6000) vs fade window (4000-6000) mismatch | `triangle.frag:2090,2065` | Still open. tMax = 6000, fade end = 6000. Exterior fragments at the boundary get wasted ray cost with zero contribution after multiplicative fade. |
| `#693` / O3-N-05 | CELL parser drops XCMT + XCCM | `crates/plugin/src/esm/cell/walkers.rs` | Still open. Per-cell climate override (Skyrim XCCM) silently dropped — every Skyrim exterior cell uses worldspace-default CLMT regardless of authored override. |
| `#539` / M33-07 | Skyrim WTHR not gated by GameKind | `parse_wthr` | Still open. Skyrim NAM0 has different stride than FNV/FO3/Oblivion; current parser may misread Skyrim weather. |
| `#528` / FNV-CELL-2 | Cloud texture load bypasses `resolve_texture` | `scene.rs` cloud loaders | Still open. No double-upload today but future TOD crossfade would compound the duplicate-load. |

---

## Exterior Rendering Assessment

**WTHR / CLMT parser**: solid. M33-01..06 all closed. NAM0 handles 240-byte (FNV+) and 160-byte (FO3 / Oblivion / older FNV) strides. Cloud textures decode from DNAM/CNAM/ANAM/BNAM. FNAM fog (16 B) is correctly typed for Oblivion, FNV, and FO3. HNAM (56 B) is properly routed to `OblivionHdrLighting`, not fog. Classification byte at offset 11.

**TOD interpolation**: `build_tod_keys` walks 7 keys (sunrise begin / end, day re-anchor, sunset begin / end, etc.) and lerps between WTHR's 6 NAM0 slots via `slot_a`, `slot_b`, `t`. Midnight wrap handled via `h + 24` if `hour < keys[0].0`. Last-key fall-through correctly picks `(keys[last], keys[0])`. Lerp is plain linear (no easing).

**Weather cross-fade** (`WeatherTransitionRes`): independently TOD-samples each side then blends in `transition_t`. Both sides pull through `build_tod_keys` independently. Correct.

**Sun arc**: cosine/sine semicircle east → up → west, `y` clamped to [0, 1] for daytime, `(0, -1, 0)` at night. **`SUN-N2` finding**: the constant `z = -0.15` is mis-labelled "south tilt" but is actually a **north tilt** under the engine's `(x, y, z)_zup → (x, z, -y)_yup` axis convention (per `crates/nif/src/import/coord.rs:18`).

**Sun intensity**: ramps 0 → 4 between 6h/7h and 17h/18h, holds 4 between 7h-17h, zero at night. Drives composite's sun disc brightness — but **NOT** the directional light's surface contribution (`SUN-N1`).

**Cloud parallax** (4 layers): scroll accumulators advance at `0.018 * dt` (layer 0), `0.018 * 1.35 * dt` (layer 1), etc. Hardcoded baseline; the real per-WTHR scroll source (ONAM / INAM) is unsourced (acknowledged in code comment). Scrolls reset on every cell-load (`STRM-N2`).

**Sky shader** (`composite.frag::compute_sky`):
- Horizon → zenith blend with `sqrt(elevation)` widening — fine.
- Below-horizon `sky_lower` blend at `elevation < 0` with `mix(horizon, sky_lower, smoothstep(-elevation*3, 0, 1))` — smooth, continuous at elevation = 0.
- 4 cloud layers, each with `dir.xz / max(elevation, 0.05)` projection, analytic LOD `log2(1/elevation) * 0.5`, horizon fade `smoothstep(0, 0.12, elevation)` — solid.
- Sun disc: `cos_angle > sun_size - 0.002` gate, smooth fringe via `mix(t * 0.5, 1.0, core)`. Sun sprite from CLMT FNAM if present, otherwise procedural. **`SUN-N4` finding**: no `elevation > 0` gate.
- Sun glow halo: `pow(max(cos_angle, 0), 4) * 0.15`. **`SUN-N3` finding**: `sun_intensity` doesn't multiply.

**Directional sun light** (triangle.frag PBR loop):
- `lightType > 1.5` arm at `triangle.frag:1896` sets `L = direction_angle.xyz`, `dist = 10000`, `atten = 1`.
- WRS reservoir shadow ray with `sunAngularRadius = 0.0047` (~physical sun) over 100,000-unit `tMax`.
- **`SUN-N1` finding**: the upload at `render.rs:1047` ships `cell_lit.directional_color` raw — no `sun_intensity` multiplication.

**GI miss path** (triangle.frag:2126): `indirect = vec3(0.6, 0.75, 1.0) * 0.06` — independent of WTHR, TOD, per-cell ambient. Carryover `#671` / RT-8.

---

## Findings

### HIGH

#### SUN-N1 — Directional sun light ignores `sun_intensity` ramp; ceilings + overhangs glow at midnight

- **Severity**: HIGH
- **Dimension**: Sky/Weather/Exterior Lighting × Shader Correctness
- **Locations**:
  - `byroredux/src/render.rs:1033-1058` — exterior directional light upload uses `cell_lit.directional_color` raw
  - `byroredux/src/systems.rs:1481` — `sky.sun_intensity = sun_intensity` (correctly computed; ramps 0 → 4)
  - `byroredux/src/systems.rs:1516` — `cell_lit.directional_dir = sun_dir` (correctly set to `(0, -1, 0)` at night per systems.rs:1437-1442)
  - `crates/renderer/shaders/triangle.frag:1896-1900` — directional arm reads `direction_angle.xyz` and `lightColor` from the SSBO without intensity gating
- **Status**: NEW
- **Description**: The composite shader correctly fades the sun *disc* in the sky over the sunrise / sunset transition by multiplying `sun_intensity` into `disc_color * sun_intensity * disc` at `composite.frag:217`. The directional light that illuminates *surfaces* takes a different path — it's uploaded as a `GpuLight` with `color_type = directional_color * 1.0` and `direction_angle = sun_dir`. There is **no `sun_intensity` multiplication** at upload time; the directional light's "color" is the raw TOD-interpolated `SKY_SUNLIGHT` slot.

  Per `systems.rs:1437-1442`, between hours 18 and 6 the sun is "pushed below horizon" via `sun_dir = (0, -1, 0)`. The directional light uploaded with this direction now points STRAIGHT DOWN. In the fragment shader's PBR loop:

  ```glsl
  // triangle.frag:1896-1900
  L = normalize(lights[i].direction_angle.xyz);  // (0, -1, 0) at night
  dist = 10000.0;
  atten = 1.0;
  ```

  - Floor fragment (N = `(0, 1, 0)`): `NdotL = max(0, dot(N, L)) = max(0, -1) = 0` ✓ (correctly skipped)
  - Ceiling fragment (N = `(0, -1, 0)`): `NdotL = max(0, +1) = 1` ✗ — receives full directional contribution

  Then `Lo += brdfResult * unshadowedRadiance` with `unshadowedRadiance = directional_color * 1`. Whatever value `SKY_SUNLIGHT[NIGHT]` happens to interpolate to lights up the ceiling. WRS shadow ray subtracts when occluded, but at distances > 4000 units `shadowFade` decays to zero, leaving the unshadowed contribution un-cancelled.

  The same issue applies at sunrise/sunset transitions where `sun_intensity` is ramping 0 → 4 but `directional_color` is at its full TOD-slot value — surfaces light up at the *full* daytime intensity instead of the ramp.
- **Evidence**:
  ```rust
  // render.rs:1046-1058 — current upload (no sun_intensity scaling)
  } else {
      (cell_lit.directional_color, 0.0)  // exterior arm
  };
  gpu_lights.push(byroredux_renderer::GpuLight {
      position_radius: [0.0, 0.0, 0.0, dir_radius],
      color_type: [dir_color[0], dir_color[1], dir_color[2], 2.0],
      direction_angle: [
          cell_lit.directional_dir[0],
          cell_lit.directional_dir[1],
          cell_lit.directional_dir[2],
          0.0,
      ],
  });
  ```
  At `sky_res.sun_intensity = 0` (midnight), the upload still ships `color_type = directional_color` at full TOD-NIGHT magnitude.
- **Trigger Conditions**: Exterior cell, hour ∈ [18, 6] (or sunrise/sunset transition), camera looking at any surface with `dot(N, sun_dir) > 0`. On real terrain this means rocks / vehicles / cliff overhangs. On vanilla content most exterior daytime ceilings are interior-occluded (covered by exterior buildings rendering above them), but the bug is observable at any exterior overhang at distance > 4000 units (where shadow fade drops the WRS subtraction).
- **Impact**: Visible artifact: at midnight in any exterior cell, ceilings of overhangs / under-rocks / under-bridges glow with the full TOD-NIGHT `SKY_SUNLIGHT` colour (often a dim blue, ~0.05-0.15 luminance) when they should be at near-zero light. Most noticeable on dark interior-of-exterior pockets (under bridges, inside ruins with open ceilings) where the night-sun bleeds through.
- **Suggested Fix**: Multiply `sun_intensity` into `directional_color` at upload time:

  ```rust
  // render.rs:1046 — fix
  } else {
      // Multiply by sun_intensity so the surface contribution fades
      // in lockstep with the composite sun disc. SkyParamsRes is
      // populated by weather_system every frame; pull the current
      // ramp value from there. Below-horizon (intensity == 0)
      // produces a zero-vector lightColor, which the WRS reservoir
      // weight gates out before the shadow ray fires.
      let intensity = world
          .try_resource::<SkyParamsRes>()
          .map(|sky| sky.sun_intensity)
          .unwrap_or(1.0);
      (
          [
              cell_lit.directional_color[0] * intensity,
              cell_lit.directional_color[1] * intensity,
              cell_lit.directional_color[2] * intensity,
          ],
          0.0,
      )
  };
  ```
- **Related**:
  - SUN-N3 (sun glow halo similarly ungated)
  - SUN-N4 (sun disc no elevation gate)
  - The 04-23 audit's `feedback_no_guessing.md` and the project's "raw monitor-space colours" convention apply: `directional_color` is the TOD-interpolated raw colour; `sun_intensity` is the perceptual ramp. The product is what reaches lighting.

### MEDIUM

#### SUN-N3 — Sun glow halo at composite.frag:223 ignores `sun_intensity`

- **Severity**: MEDIUM
- **Dimension**: Sky/Weather/Exterior Lighting
- **Location**: `crates/renderer/shaders/composite.frag:220-223`
- **Status**: NEW
- **Description**: The sun glow term adds `sun_col * pow(max(cos_angle, 0), 4) * 0.15` to every sky pixel. This term is OUTSIDE the disc gate at line 184, so it fires for every sky direction with `dot(dir, sun_direction) > 0`. The 0.15 constant is fixed; there is **no `sun_intensity` multiplier** on the glow.

  At sunset, when `sun_intensity` is fading 4 → 0, the disc itself fades correctly (line 217 multiplies it). But the halo around the sun stays at full constant brightness, gated only by whatever `sun_col` happens to be at the active TOD slot. If the active WTHR has authored a non-zero `SKY_SUN[NIGHT]` (some Bethesda WTHRs do, e.g. Skyrim's `MoonShadow` weather), the halo persists at midnight.
- **Evidence**:
  ```glsl
  // composite.frag:220-223
  // Sun glow: soft radial halo around the sun.
  float glow = max(cos_angle, 0.0);
  glow = pow(glow, 4.0);
  sky += sun_col * glow * 0.15;       // ← no `sun_intensity` multiply
  ```
- **Impact**: At midnight on any WTHR with non-zero `SKY_SUN[NIGHT]`, a faint warm halo persists in the sky around the sun's nominal direction. With sun_dir hardcoded to `(0, -1, 0)` at night, the halo is at the antipode of zenith — i.e., looking down. Sky pixels are normally only rendered above-horizon, so visibility depends on the sky branch being entered for below-horizon dirs. `composite.frag:107` does enter the sky_lower mix branch for `elevation < 0`, so the halo CAN render below the horizon line on a flat-ground exterior. Visually subtle.
- **Suggested Fix**: One-line: `sky += sun_col * glow * 0.15 * sun_intensity;`
- **Related**: SUN-N1 (same root: ramp not consistently propagated)

#### SUN-N4 — Sun disc renders without elevation gate; visible "below ground" at sunset/sunrise

- **Severity**: MEDIUM
- **Dimension**: Sky/Weather/Exterior Lighting
- **Location**: `crates/renderer/shaders/composite.frag:184-218`
- **Status**: NEW
- **Description**: The sun disc draws when `cos_angle > sun_size - 0.002`. There's no check that `dir.y > 0` (elevation positive) or that `sun_direction.y > 0` (sun above horizon). At sunset / sunrise / through the night, when the sun is below horizon (`sun_direction.y < 0`), the disc still draws additively over below-horizon sky pixels.

  Below-horizon sky pixels go through the `elevation < 0` branch (line 107) which mixes `horizon → sky_lower`. The disc add at line 217 then layers on top, painting a visible sun on the "ground tint" region.

  The visual effect requires (a) flat horizon view (no terrain occlusion at low elevation), and (b) sun_intensity > 0 (otherwise disc=0). The sunrise/sunset window has both: terrain may be 1000+ units away with nothing covering the immediate horizon, and `sun_intensity` is ramping but non-zero.
- **Evidence**:
  ```glsl
  // composite.frag:182-218 — disc drawn unconditionally when cos_angle is high
  float cos_angle = dot(dir, sun_direction);
  float sun_edge_start = sun_size - 0.002;
  if (cos_angle > sun_edge_start) {
      // ... compute disc, sample sprite ...
      sky += disc_color * sun_intensity * disc;   // ← no `dir.y > 0` gate
  }
  ```
- **Impact**: Visual artifact when looking at a flat horizon at sunrise or sunset with no terrain occlusion. Real exterior content (Mojave's open wasteland, Skyrim's tundra) has enough mountain / cliff geometry to occlude the horizon line — the sky branch only renders when `depth >= 0.9999`, so terrain depth-tests out the disc. The bug is visible primarily on:
  - Open flat exteriors with no nearby terrain
  - Indoor exterior cells (DLC interior with sky visible through skylights)
  - Camera elevated above mountains looking at sunset
- **Suggested Fix**: Two options:
  1. Gate the disc add on `dir.y > 0`: `if (cos_angle > sun_edge_start && dir.y > 0)`. Cleanest; matches the cloud-layer gates at lines 130, 147, 158, 169.
  2. Multiply `disc` by `smoothstep(0, 0.05, dir.y)` for a soft horizon fade, mirroring the cloud `horizon_fade`.

  Option 1 is simpler and matches existing cloud gating.
- **Related**: SUN-N3 (same compositing pass; both have the "no elevation/intensity gate" pattern)

#### STRM-N1 — Cell streaming doesn't signal SVGF/TAA temporal discontinuity; new geometry ghosts ~5 frames

- **Severity**: MEDIUM
- **Dimension**: Denoiser & Composite × M40 Streaming
- **Locations**:
  - `crates/renderer/src/vulkan/svgf.rs:70` — `signal_temporal_discontinuity` API exists, hands `svgf_recovery_frames` window to the alpha state machine
  - `byroredux/src/streaming.rs` / `byroredux/src/main.rs:406` (`step_streaming`) — no callers to `signal_temporal_discontinuity`
  - `byroredux/src/cell_loader.rs` — neither `load_one_exterior_cell` nor `unload_cell` notifies the renderer of the discontinuity
- **Status**: NEW
- **Description**: SVGF temporal accumulation blends the previous-frame denoised indirect into the current frame at α=0.2 (steady state). When new geometry streams in via the M40 cell loader, the previous-frame indirect history doesn't know about the change — it ghosts onto the newly-loaded geometry until α=0.2 accumulation washes it out (5+ frames at 60 FPS).

  `#674 (DEN-4)` shipped a recovery-α state machine: `signal_temporal_discontinuity` bumps α to 0.5 for `svgf_recovery_frames` upcoming frames. But there is no caller — the API is plumbed but disconnected. Cell-load and cell-unload events are the canonical discontinuity signals, and neither path calls it.
- **Evidence**:
  ```rust
  // svgf.rs:70 — API exists
  // for `svgf_recovery_frames` upcoming frames.
  ```
  ```bash
  $ grep -rn signal_temporal_discontinuity byroredux/ crates/
  crates/renderer/src/vulkan/svgf.rs:70: ... `svgf_recovery_frames` ...
  # Zero callers
  ```
- **Trigger Conditions**: M40 streaming load (cell becomes loaded around the player's grid). New BLAS entries are added to TLAS, new instances populate scene_buffers. SVGF history is ~5 frames stale on those pixels.
- **Impact**: Visible streaming-burst ghosting on exterior cell transitions. Mitigated partially by mesh ID rejection in SVGF (each newly-spawned mesh gets a fresh ID, so disocclusion fires at the per-pixel mesh boundary). But the same-mesh-different-position case (e.g., a new instance of an existing rock placed where another rock was just unloaded — same `mesh_handle`, same `mesh_id` after instance-index encoding) doesn't disocclude correctly.
- **Suggested Fix**: Hook `signal_temporal_discontinuity` from `step_streaming` whenever a cell is loaded or unloaded. The API takes `&mut self` on `SvgfPipeline`; thread it through `VulkanContext` so the cell loader can call `ctx.signal_temporal_discontinuity()` on every cell-state change.

  Alternative: signal on TLAS rebuild specifically (any time `last_blas_addresses` changes). This catches every geometry-set change, including the TLAS rebuild within a single frame.
- **Related**:
  - `#674` (DEN-4, closed) — established the API
  - `RT-14` (carryover) — distance fade behaviour at the cell-streaming radius is interrelated

### LOW

#### SUN-N2 — Sun arc tilts NORTH (`z = -0.15`); should be SOUTH for NH-latitude games

- **Severity**: LOW
- **Dimension**: Sky/Weather/Exterior Lighting
- **Location**: `byroredux/src/systems.rs:1432-1438`
- **Status**: NEW
- **Description**: The sun arc is computed in the engine's Y-up world space:
  ```rust
  let x = angle.cos();    // east → west
  let y = angle.sin();    // up
  let z = -0.15_f32;      // "slight south tilt" per the comment
  ```
  Per `crates/nif/src/import/coord.rs:18`, the engine's Z-up → Y-up axis swap is `(x, y, z) → (x, z, -y)`. Bethesda's authored Z-up convention has +Y = north (forward into the screen / cell grid +gy). After the swap, Bethesda +Y maps to engine -Z. So:
  - Engine -Z = NORTH
  - Engine +Z = SOUTH

  The constant `z = -0.15` is therefore a NORTH bias, not a south bias. The comment claims south.

  For the Bethesda settings:
  - Mojave (FNV) — desert at Nevada-equivalent ~36°N latitude
  - Capital Wasteland (FO3) — DC-equivalent ~38°N
  - Tamriel (Oblivion / Skyrim) — temperate-fictional ~30-50°N
  - Commonwealth (FO4) — Massachusetts ~42°N

  All northern hemisphere. The real sun in NH locations arcs through the SOUTHERN sky — bias should be `+0.15` (south = engine +Z), not `-0.15`.
- **Evidence**: `git log -p byroredux/src/systems.rs | grep -B 2 'south tilt'` shows the constant has been there since the initial M33 sun-arc commit; the `-0.15` value was chosen visually rather than verified against the engine's axis convention.
- **Impact**: Visible to players who orient by in-game compass: at noon, vertical objects (lamp posts, walls, NPCs) cast shadows pointing **NORTH** instead of **SOUTH**. Compasses and map orientation in the in-game UI are unaffected; only the lighting cardinal is wrong. Cosmetic.
- **Suggested Fix**: One character. `let z = 0.15_f32;` and update the comment to "slight south tilt" (now factual). Visual validation: at noon in any FNV exterior, a vertical pole's shadow should fall on the south side; spawn a small POI / freight container in `--grid 0,0` and confirm.
- **Related**: None. The `-Z = north` engine convention is consistent across the codebase (verified in coord.rs + WRLD axis docs); only this single constant is reversed.

#### STRM-N2 — Cloud scroll accumulator resets on every interior→exterior re-entry

- **Severity**: LOW
- **Dimension**: Sky/Weather/Exterior Lighting × Resource Lifecycle
- **Locations**:
  - `byroredux/src/cell_loader.rs:198` — `world.remove_resource::<SkyParamsRes>();` on cell unload
  - `crates/renderer/src/vulkan/context/mod.rs:413-426` — `SkyParams::default()` initializes `cloud_scroll: [0.0, 0.0]` for all 4 layers
  - `byroredux/src/systems.rs:1484-1502` — `weather_system` accumulates scroll per-frame
- **Status**: NEW
- **Description**: When the player exits an exterior cell to an interior, `unload_cell` removes the entire `SkyParamsRes`. On re-entry to the same exterior worldspace, `apply_worldspace_weather` constructs a fresh `SkyParamsRes` with `cloud_scroll: [0.0, 0.0]` for all 4 layers. The scroll accumulator is therefore reset; the cloud texture sampling restarts at UV origin (0,0) instead of resuming from where it was at exit.

  Visually, clouds that were drifting east-west "snap back" to their starting frame on every interior↔exterior transition. The longer the player was indoors, the more visible the snap-back.
- **Evidence**: `cell_loader.rs:198` removes `SkyParamsRes` unconditionally on unload. `scene.rs::apply_worldspace_weather` constructs a fresh one via the per-WTHR resolver at line 208 and the cloud_scroll fields are zero-initialised (no carry-over from any prior `SkyParamsRes`).
- **Impact**: Mostly cosmetic. Players spending non-trivial time in interiors (saving, looting, conversing) and emerging will see clouds reset to the worldspace's "initial" cloud frame. On real Bethesda content where interior visits last 30s+ at a time, the reset is a 30s × 0.018 UV/s = ~0.5 UV jump — visible if the user happens to look up at the moment of transition.
- **Suggested Fix**: Preserve the cloud_scroll accumulator across `SkyParamsRes` re-creation. Two options:
  1. Pull existing `SkyParamsRes` cloud_scroll fields before removing the resource on unload, and restore them on the next `apply_worldspace_weather` (requires a shared "preserved-cloud-state" resource not removed on unload).
  2. Move the cloud_scroll accumulator to a separate `CloudSimState` resource that survives cell transitions. Same shape as `GameTimeRes`, which correctly survives interior↔exterior transitions.

  Option 2 is cleaner and matches the existing time-survives pattern.
- **Related**: STRM-N1 (same general theme: cell-state machine forgets continuous-simulation state)

### INFO

#### STAT-N1 — Distant LOD batches (BSPackedCombined*) parse but never render — M35 deferred

- **Severity**: INFO
- **Dimension**: Sky/Weather/Exterior Lighting × Resource Lifecycle
- **Locations**:
  - `crates/nif/src/blocks/extra_data.rs:706+` — parser
  - `crates/nif/src/import/walk.rs:227-233` — walker explicitly **skips** `BSMultiBoundNode` subtrees that carry `BSPackedCombinedGeomDataExtra`
  - `crates/nif/src/import/walk.rs:490` — same skip on the second walker pass
- **Status**: Existing (deferred per M35 — this audit just notes the user-visible consequence)
- **Description**: Skyrim's exterior worldspaces ship distant content (mountains, distant cities, far-LOD trees) as **merged-geometry batches** under `BSMultiBoundNode` parents that carry a `BSPackedCombinedGeomDataExtra` extra-data block. The parser correctly decodes the extra-data; the importer's walker explicitly returns early on these subtrees. The packed-extra block stays available on the scene table but no renderer-side consumer reads it.
- **Impact**: Skyrim exterior at any cell shows the immediate streaming radius (3 cells default, 7 cells max) but **everything beyond the radius is empty sky**. Distant mountains, distant cities, distant tree silhouettes — all missing. Side-effect: the immediate-streamed terrain ends at a hard edge with sky beyond. Visible at any vista in Skyrim or FO4 that depends on far-LOD content.
- **Trigger Conditions**: Any Skyrim or FO4 exterior cell with authored distant LOD batches (most outdoor cells).
- **Suggested Fix**: Out of scope for this audit. Tracked under M35 (terrain streaming + distant LOD). The NIF-side parse is in place; the renderer-side importer needs a path that converts `BsPackedGeomData` entries into `ImportedMesh` records with the appropriate batched-render flag.
- **Related**: M35 milestone; no GH issue currently filed (the parser comment at walk.rs:230 calls out "M35 terrain-streaming work" but doesn't cite an issue number).

#### STAT-N2 — BSTreeNode SpeedTree wind bones parsed but no wind-animation system — trees render static

- **Severity**: INFO
- **Dimension**: Sky/Weather/Exterior Lighting × Animation
- **Locations**:
  - `crates/nif/src/blocks/node.rs:267-300` — `BsTreeNode` parser with `bones_1` (branch roots) + `bones_2` (trunk)
  - `crates/nif/src/import/walk.rs:894-909` — `extract_tree_bones` resolves the bones into `TreeBones` payload
  - `crates/nif/src/import/mod.rs:116-150` — `TreeBones` field on `ImportedNode`
- **Status**: Existing (deferred — this audit notes the user-visible consequence)
- **Description**: The SpeedTree integration parses tree wind-bone metadata end-to-end into `ImportedScene`. There's no system that consumes it — trees render as static geometry, immune to the cloud-driven wind multiplier or any dynamic motion.
- **Impact**: Skyrim and FO4 trees never sway. Visually noticeable on windy weathers (high `wind_speed` byte in the active WTHR's DATA record) where every other animated element responds to wind but trees stay rigid.
- **Suggested Fix**: Out of scope. Future SpeedTree wind-bone driver subsystem; no dependency on parser-side correctness which is already complete.

---

## Carryover Findings — Re-confirmed Open

These are not new findings; they appear in prior audits (the GH issue is the system of record). The user's observation that exterior rendering hadn't been audited is partly explained by these items being filed under their broader audits (renderer / FNV / Skyrim) rather than as a focused exterior-specific list. Re-listing them here so they're visible as a coherent exterior-rendering backlog:

### `#671` / RT-8 — GI miss uses hardcoded sky color

- **Severity**: HIGH (re-classifying — was MEDIUM in the carryover backlog)
- **Location**: `triangle.frag:2126`
- **Code**: `indirect = vec3(0.6, 0.75, 1.0) * 0.06;`
- **Why this is the headline carryover**: the hardcoded `(0.6, 0.75, 1.0)` blue sky × 0.06 attenuation is the single most-impactful exterior issue. It's TOD-independent, weather-independent, per-cell-ambient-independent. At midnight in a clear cell, every GI-miss fragment still receives a constant blue tint. At sunset in a stormy cell, the same blue tint appears. The fix would feed the active `SkyParamsRes::zenith_color` (or a hemisphere-integrated equivalent) into the GI miss path so the bounce-back sky colour follows the actual scene state.
- **Suggested Fix**: Replace the hardcoded literal with a sample of the active sky:
  ```glsl
  // triangle.frag — feed sky_zenith from SkyParamsRes via the camera UBO
  // or a dedicated "indirect_sky_color" UBO field; multiply by the
  // active TOD ambient floor so night cells get correctly-dim GI.
  indirect = sky_zenith * sky_ambient_floor;
  ```
  The `sky_zenith` is already plumbed to composite.frag via `CompositeParams.sky_zenith`. Add the same field to triangle.frag's camera UBO (already 8+ fields wide, capacity for one more) and route it through.

### `RT-14` — GI ray tMax (6000) vs fade-window end (6000) — boundary fragments waste ray cost

- **Severity**: LOW
- **Location**: `triangle.frag:2065,2090`
- **Status**: From 2026-05-01 audit; still open
- **Carryover note**: not a correctness issue; perf only. Fragments at distance 6000 from camera waste the ray query (giFade = 0 multiplies the result to zero). Reduce tMax to ~4000 (matching `smoothstep(4000.0, 6000.0)` start) and let the smoothstep gate the contribution; saves 33% of ray cost in fade-band fragments.

### `#693` / O3-N-05 — CELL parser drops XCMT (pre-Skyrim music) + XCCM (Skyrim per-cell climate override)

- **Severity**: LOW
- **Location**: `crates/plugin/src/esm/cell/walkers.rs`
- **Carryover note**: per-cell climate override means a Skyrim cell that wants to use a different CLMT than its worldspace default (e.g., a cave entrance with brighter ambient) silently inherits the worldspace default. Visible in cells like Solitude Avenues (different climate) and a handful of Dragonborn DLC interiors with dedicated atmosphere overrides.

### `#539` / M33-07 — Skyrim WTHR not gated by GameKind

- **Severity**: MEDIUM
- **Location**: `parse_wthr`
- **Carryover note**: Skyrim's WTHR has a different sub-record schema than FNV/FO3/Oblivion. The current parser dispatches by sub-record FourCC without a GameKind check, so Skyrim weathers may misread some fields. Hasn't been observed to break Skyrim WhiterunBanneredMare benchmarking (interior cell), but exterior Skyrim hasn't been validated end-to-end.

### `#528` / FNV-CELL-2 — Cloud texture load bypasses `resolve_texture`

- **Severity**: LOW
- **Location**: `scene.rs` cloud loaders
- **Carryover note**: defense-in-depth. The cloud loaders call `tex_provider.extract` + `texture_registry.load_dds` directly, bypassing the `resolve_texture` helper that handles refcount + dedup. No double-upload today (the loaders short-circuit on already-loaded paths), but a future TOD crossfade that wants to load both "current" and "target" weather's clouds simultaneously would double-upload via this path.

---

## Prioritized Fix Order

1. **#671 / RT-8** (carryover, HIGH after re-classification) — feed `sky_zenith` into the GI miss path. Single most-impactful exterior fix; TOD/weather/per-cell ambient all flow through.
2. **SUN-N1** (HIGH) — multiply `sun_intensity` into `directional_color` at upload. One-block change in `render.rs`.
3. **STRM-N1** (MEDIUM) — hook `signal_temporal_discontinuity` from cell-load/unload. Eliminates the streaming-burst ghosting.
4. **SUN-N3** + **SUN-N4** (MEDIUM × 2) — bundle: gate sun glow on `sun_intensity`, gate sun disc on `dir.y > 0`. Two-line shader edit.
5. **SUN-N2** (LOW) — flip the sign on the sun-arc cardinal tilt. One-character fix.
6. **STRM-N2** (LOW) — preserve cloud_scroll across cell transitions via a `CloudSimState` resource. Mirror `GameTimeRes` pattern.
7. **#693, #539, #528** (carryover LOWs) — defense-in-depth + Skyrim-exterior fidelity. Tackle as the Skyrim exterior render path becomes a primary test scene.

The HIGH + MEDIUM items combined are roughly **2 days of focused work**. RT-8 and SUN-N1 alone close the worst of the night-rendering visual artifacts and the brightness-mismatch-vs-disc.

---

## Verified Working — No Gaps

- **WTHR / CLMT parser**: M33-01..06 all closed; sub-record FourCCs match authoritative byte layout for FNV/FO3/Oblivion. NAM0 dual-stride (240 / 160 B) correctly handled.
- **TOD interpolation**: 7-key build, midnight wrap, per-channel lerp.
- **Weather cross-fade**: independent TOD-sample on both sides, blended in `transition_t`.
- **Sky branch gate**: `is_exterior > 0.5 && depth >= 0.9999` correctly skips interiors.
- **Cloud projection**: 4 layers, analytic LOD, horizon fade, opposite-direction parallax — all sound.
- **Below-horizon `sky_lower` mix**: continuous at elevation = 0.
- **Directional cluster routing**: cluster_cull correctly adds directional lights to every cluster (`lightType > 1.5` arm at cluster_cull.comp:208).
- **VHGT delta decode**: column-then-row accumulator, `* 8.0` scale per UESP. Adjacent-cell boundary continuity preserved (relies on Bethesda authoring-side correctness, but the parser doesn't corrupt).
- **WTHR cloud scroll wrap**: `rem_euclid(1.0)` keeps accumulators bounded.

---

## Methodology Notes

- The audit was deliberately re-anchored on the exterior path the prior audit series didn't surface as a primary topic. Per the user's observation, the test-data bias toward `GSDocMitchellHouse` and `WhiterunBanneredMare` had hidden the night-sun bleed and the GI-miss hardcode behind interior-only test scenes.
- The 2026-04-21 M33 parser audit (4 CRITICAL findings, all closed) covered the *parsing* side comprehensively. This audit covers the *rendering consumer* side — sun arc, directional light upload, GI miss, sun glow, sun disc, streaming continuity, cloud parallax persistence.
- Three findings (`SUN-N1`, `SUN-N3`, `STRM-N1`) share a structural pattern: an API or signal exists but isn't wired. `sun_intensity` is computed correctly but not consumed at the directional-light upload or sun-glow term; `signal_temporal_discontinuity` is plumbed but has no callers. This points to a category of bugs that pure code-grep audits miss — they show up only when end-to-end behaviour is traced from producer to consumer.

---

*Generated by `/audit-renderer --focus 15` on 2026-05-03. To file findings as GitHub issues: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-03_EXTERIOR.md`.*
