# Renderer + NIF Lighting Audit — 2026-04-12c

**Focus**: Why FNV interiors render too dark. Full trace from ESM/NIF → GPU → pixel.

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH     | 2 |
| MEDIUM   | 3 |
| LOW      | 2 |
| **Total** | **8** |

The Prospector Saloon is dark because of **three compounding root causes**, not a single bug:

1. **sRGB light colors treated as linear** (CRITICAL) — every color from ESM LIGH records, XCLL ambient/directional, and NIF light blocks is parsed as u8/255.0 and passed straight to the PBR shader without sRGB→linear conversion. The shader computes physically-based lighting with gamma-encoded inputs, producing systematically wrong energy levels.

2. **Duplicate lights with inflated radii** (HIGH) — LIGH records with NIF-embedded NiPointLight blocks create 2-4x the expected lights. The NIF lights get radius 2048 (from constant-only attenuation coefficients) while the ESM LIGH record has the correct 200-800 radius. The inflated lights dominate cluster budgets.

3. **Missing per-light ambient contribution** (HIGH) — Gamebryo's D3D9 fixed-function pipeline adds `Material.Ambient * Light.Ambient * dimmer` per light on top of the global scene ambient. With 20-40 lights, this is a significant fill. ByroRedux has no per-light ambient term at all.

The ambient boost (2.5x) and directional fill (35% unshadowed) are workarounds for these missing contributions.

---

## Numerical Trace

For a grey wall (albedo 0.5, roughness 0.7) in the Prospector Saloon with boosted ambient [0.30, 0.28, 0.25], one point light at distance 300, and the directional fill:

| Stage | Value (R channel) | Notes |
|-------|-------------------|-------|
| ambient | 0.150 | sceneFlags.yzw * albedo * (1-metalness) |
| GI miss (sky fill) | 0.036 | vec3(0.6,0.75,1.0) * 0.06 |
| indirect = (ambient + GI) * ao | 0.084 | ao = 0.45 floor |
| Point light Lo | 0.028 | attenuation ~0.06 at d=300, r=512 |
| Directional fill Lo | 0.010 | color 0.15 * NdotL 0.3 * BRDF |
| Total direct | 0.038 | |
| Total (direct + indirect) | 0.122 | |
| After ACES | 0.107 | Nearly linear in this range |
| After sRGB gamma | 0.36 | 92/255 — dim but visible |

The values ARE in a reasonable range with the 2.5x boost. The problem is that without the boost (raw XCLL ~0.12), the ambient would be ~0.06, producing a final of ~0.18 sRGB (46/255 — nearly black).

**The real fix is not to boost — it's to get the lighting equation right.**

---

## Findings

### RL-01: sRGB light/ambient colors passed to shader without linearization
- **Severity**: CRITICAL
- **Dimension**: Lighting Pipeline
- **Location**: `crates/plugin/src/esm/cell.rs:224-231` (XCLL), `cell.rs:548-550` (LIGH), `crates/nif/src/import/walk.rs:351` (NIF lights)
- **Status**: NEW
- **Description**: All color values from ESM and NIF files are stored in sRGB space (u8 per channel from the Creation Kit color picker). They are divided by 255 and passed directly to the PBR fragment shader as linear values. The shader's physically-based BRDF math expects linear-space inputs. The gamma curve means mid-tone colors (~128/255 = 0.5 sRGB) are actually 0.214 linear — a 2.3x error. This systematically under-lights the scene for warm-toned lights and over-lights for near-white lights.
- **Impact**: Every light in every cell has wrong energy. Warm interior lights (amber, orange) lose 40-60% of their intended brightness. The ambient boost hack partially compensates but is not a correct fix.
- **Suggested Fix**: Apply `srgb_to_linear()` to all color channels at parse time:
  ```rust
  fn srgb_to_linear(c: f32) -> f32 {
      if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
  }
  ```
  Apply to: XCLL ambient/directional, LIGH record colors, NIF NiLight diffuse colors, NIF NiMaterialProperty emissive/specular colors.

### RL-02: Duplicate light entities from overlapping NIF + ESM paths
- **Severity**: HIGH
- **Dimension**: Light Import
- **Location**: `byroredux/src/cell_loader.rs:428-455` (NIF lights), `cell_loader.rs:627-636` (ESM light_data on meshes)
- **Status**: NEW
- **Description**: For a LIGH record with both a mesh (candle model) and an NIF containing NiPointLight, two independent light spawn paths fire:
  1. `cached.lights` walk at lines 428-455 spawns NIF-embedded lights (often with radius 2048 from constant attenuation)
  2. Lines 627-636 attach `LightSource` to EVERY mesh entity if `light_data.is_some()`
  A candle NIF with 3 sub-meshes produces 4 lights: 1 NIF + 3 ESM. The NIF light has inflated radius 2048, the ESM lights have correct radius.
- **Impact**: 2-4x light count, inflated radii dominate cluster budget, push correct local lights out of per-cluster limits.
- **Suggested Fix**: Skip the mesh-entity `light_data` attachment when `cached.lights` is non-empty. When both exist, prefer the ESM `light_data` radius over the NIF-computed `attenuation_radius`.

### RL-03: Missing per-light ambient contribution (D3D9 legacy gap)
- **Severity**: HIGH
- **Dimension**: Lighting Model
- **Location**: `crates/renderer/shaders/triangle.frag:444-466`
- **Status**: NEW
- **Description**: Gamebryo's D3D9 fixed-function pipeline adds `Material.Ambient * Light.Ambient * dimmer` per light. This is a SEPARATE term from the global scene ambient. With 20-40 interior lights, each contributing a small ambient fill, this adds up to a significant portion of interior illumination. ByroRedux has only the global ambient term — no per-light ambient at all.
- **Impact**: Interior base illumination ~40-60% lower than Gamebryo's. The 2.5x ambient boost partially compensates.
- **Suggested Fix**: Add a per-light ambient contribution in the light loop. For each point light in range, add `lightColor * atten * 0.15 * albedo` (approximate the D3D9 per-light ambient term). This fills the scene with gentle ambient from each nearby light without requiring NdotL.

### RL-04: NIF light attenuation_radius returns 2048 for constant-only coefficients
- **Severity**: MEDIUM
- **Dimension**: Light Import
- **Location**: `crates/nif/src/import/walk.rs:367-387`
- **Status**: NEW
- **Description**: Many Gamebryo NIFs have attenuation coefficients (1.0, 0.0, 0.0) — constant only, no falloff. The `attenuation_radius` function falls to the "no attenuation" branch and returns hardcoded 2048.0. The ESM LIGH record has the actual authored radius (200-800 for interiors).
- **Suggested Fix**: When a LIGH record exists, always prefer its radius over the NIF-computed radius.

### RL-05: No exposure control in the rendering pipeline
- **Severity**: MEDIUM
- **Dimension**: Tone Mapping
- **Location**: `crates/renderer/shaders/composite.frag`
- **Status**: NEW
- **Description**: The composite pass does `direct + indirect → ACES → sRGB` with no exposure multiplier. The pipeline works for dim interiors (~0.1-0.3 linear) but will break for bright exteriors (>1.0 linear) or mixed indoor/outdoor scenes. No auto-exposure or manual exposure control exists.
- **Impact**: Not urgent for current interior-only testing. Will be needed for exterior cells.
- **Suggested Fix**: Add an exposure uniform to the composite params: `final = aces(exposure * (direct + indirect))`. Default exposure = 1.0, tune per-cell or implement auto-exposure.

### RL-06: The 2.5x ambient boost should be removed once sRGB linearization is fixed
- **Severity**: MEDIUM
- **Dimension**: Lighting Pipeline
- **Location**: `byroredux/src/render.rs:418`
- **Status**: NEW
- **Description**: The 2.5x interior ambient boost was added as a workaround for the compounding darkness from RL-01 (sRGB colors) and RL-03 (missing per-light ambient). Once those root causes are fixed, the boost will make interiors too bright. It should be removed or reduced after RL-01 and RL-03 are addressed.

### RL-07: APPLY_REPLACE texture mode not checked — some meshes incorrectly lit
- **Severity**: LOW
- **Dimension**: Material Pipeline
- **Location**: `crates/nif/src/import/material.rs`
- **Status**: NEW
- **Description**: NiTexturingProperty's ApplyMode can be APPLY_REPLACE (0), which in Gamebryo disables lighting entirely for that mesh (raw texture color only). ByroRedux does not check for this mode — such meshes go through the full PBR pipeline.
- **Impact**: Minor visual incorrectness on specific meshes. Most FNV content uses APPLY_MODULATE.

### RL-08: fxlight model filter may drop ESM light_data for effect meshes
- **Severity**: LOW
- **Dimension**: Light Import
- **Location**: `byroredux/src/cell_loader.rs:289-294`
- **Status**: NEW
- **Description**: The `fxlight` substring filter skips the entire REFR processing (including `light_data` spawning) for effect meshes. Some LIGH records reference fxlight meshes but have valid ESM `light_data` for the actual point light — those lights are silently dropped.
- **Suggested Fix**: Move the filter check after the light_data entity spawn, or spawn the light entity before the `continue`.

---

## Root Cause Priority

The correct fix order is:

1. **RL-01** (CRITICAL) — sRGB linearization: fixes the energy budget for all lights
2. **RL-02** (HIGH) — duplicate light elimination: fixes cluster budget and removes inflated-radius ghosts
3. **RL-03** (HIGH) — per-light ambient: restores the missing fill that makes interiors livable
4. **RL-06** (MEDIUM) — remove/reduce the 2.5x hack after 1-3 are fixed
5. **RL-04** (MEDIUM) — prefer ESM radius over NIF attenuation_radius
6. **RL-05** (MEDIUM) — exposure control (needed for outdoor scenes)
7. **RL-07, RL-08** (LOW) — cleanup

## Gamebryo D3D9 Lighting Equation (Reference)

```
Color = Mat.Emissive
      + Mat.Ambient * SceneAmbient              // XCLL ambient
      + Σ [ Mat.Ambient * Light[i].Ambient      // per-light ambient fill
          + Mat.Diffuse * Light[i].Diffuse * NdotL  // diffuse
          + Mat.Specular * Light[i].Specular * spec  // specular
          ] * dimmer / atten(C + L*d + Q*d²)
```

## ByroRedux Current Equation

```
ambient = SceneAmbient * albedo * (1 - metalness)
Lo = Σ [ (kD * albedo/π + specBRDF) * Light[i].color * NdotL * shadow / atten ]
indirect = (ambient + GI_bounce) * AO
final = aces(Lo + indirect)
```

## Missing Terms

| Gamebryo Term | ByroRedux | Status |
|---------------|-----------|--------|
| Material.Emissive (always-on) | emissive bypass (gated by emissive_mult > 0.01) | Partial |
| Material.Ambient * SceneAmbient | sceneFlags.yzw * albedo | Approximate (ignores per-material ambient) |
| Material.Ambient * Light[i].Ambient | Not implemented | **MISSING** |
| C + L*d + Q*d² attenuation | radius-based inverse-square windowed | Different curve |
| sRGB → linear for all colors | Not implemented | **MISSING** |
| Exposure control | Not implemented | **MISSING** |
| NiTexturingProperty APPLY_REPLACE | Not checked | Minor |
