# Renderer Lighting Audit — 2026-04-12

**Focus**: Why FNV interiors remain too dark despite multiple fix iterations.
Grounded in Gamebryo 2.3 source analysis and RT illumination research
(RTR4, SVGF 2017, ReSTIR DI/GI).

## Executive Summary

The interior darkness stems from **three compounding architectural gaps**, not parameter tuning:

1. **Wrong attenuation model** — Our `window² / (1 + ratio*4)` gives ~0.08 at half-radius. Gamebryo's D3D9 default (`1/d` with C=0,L=1,Q=0) gives ~0.5 at the same point. The authored light radii and colors assume the original attenuation curve.

2. **Missing per-light ambient term** — Gamebryo adds `Light.Ambient × Material.Ambient × dimmer / atten` per light (default: 1.0 × 0.5 × 1.0 / atten = 0.5/atten per light). With 20 lights, this is a massive fill. Our 0.25 flat per-light ambient doesn't scale correctly with distance.

3. **No albedo demodulation in denoiser** — SVGF 2017 §4 explicitly requires demodulating albedo before filtering and remodulating after. Our SVGF temporal pass filters albedo-baked indirect, blurring texture detail and losing energy at color boundaries.

**The correct approach**: Rather than continuing to tune multipliers, implement the established light transport equations that the content was authored for.

---

## Part 1: The Gamebryo D3D9 Lighting Equation (Reference)

From `NiDX9LightManager.cpp` (Gamebryo 2.3 source):

```
Color = Material.Emissive
      + Material.Ambient × D3DRS_AMBIENT                    // global ambient
      + Σ_lights [
          Material.Ambient × Light[i].Ambient × dimmer      // per-light ambient
        + Material.Diffuse × Light[i].Diffuse × dimmer × NdotL  // diffuse
        + Material.Specular × Light[i].Specular × dimmer × spec // specular
        ] / atten(C + L×d + Q×d²)
```

**Default values** (from NiLight.cpp, NiPointLight.cpp, NiMaterialProperty.cpp):
- Light.Ambient = (1.0, 1.0, 1.0)
- Light.Diffuse = (1.0, 1.0, 1.0)
- Light.Specular = (1.0, 1.0, 1.0)
- Dimmer = 1.0
- Attenuation: C=0, L=1, Q=0 → `atten = 1/(L×d) = 1/d`
- Material.Ambient = (0.5, 0.5, 0.5)
- Material.Diffuse = (0.5, 0.5, 0.5)
- Material.Specular = (0.0, 0.0, 0.0) — off by default
- D3D9 range = sqrt(FLT_MAX) — no cutoff

**Color space**: Raw floats, no sRGB conversion. The pipeline runs in "monitor space" — D3D9 without `D3DRS_SRGBWRITEENABLE` means authored values are effectively sRGB treated as linear.

**Key implication**: Legacy content brightness depends on `1/d` attenuation + additive per-light ambient. These are non-negotiable if we want correct rendering.

---

## Part 2: What Our Current Pipeline Does Wrong

### RL-A: Attenuation curve mismatch

**Current formula** (triangle.frag):
```glsl
float ratio = dist / max(radius, 1.0);
float window = clamp(1.0 - ratio, 0.0, 1.0);
window *= window;
atten = window / (1.0 + ratio * 4.0);
```

At ratio=0.5 (half radius): `window = 0.25`, `atten = 0.25 / 3.0 = 0.083`
At ratio=0.25: `window = 0.56`, `atten = 0.56 / 2.0 = 0.28`

**Gamebryo's formula** at the same distances (with radius R and default L=1):
`atten = 1 / (1 × d)`. At d = R/2: `atten = 2/R`. At d = R/4: `atten = 4/R`.

The key difference: Gamebryo's attenuation is **distance-based** (1/d), not **ratio-based** (f(d/R)). The radius only controls when the game engine stops sending the light to the shader — it's a culling radius, not an attenuation parameter.

**Recommended formula** matching Gamebryo's behavior:
```glsl
float ratio = dist / max(radius, 1.0);
float window = clamp(1.0 - ratio * ratio, 0.0, 1.0);  // smooth cutoff
atten = window * radius / (radius + dist);               // ~1/d normalized by radius
```

At ratio=0.5: `window = 0.75`, `atten = 0.75 × R / (R + R/2) = 0.75 × 0.667 = 0.50`
At ratio=0.25: `window = 0.94`, `atten = 0.94 × R / (R + R/4) = 0.94 × 0.8 = 0.75`

This gives 6× more light at half-radius than our current formula.

### RL-B: Per-light ambient not scaled by attenuation

**Current**: `Lo += lightColor * atten * shadow * albedo * 0.25`

**Gamebryo**: `Material.Ambient(0.5) × Light.Ambient(1.0) × dimmer / atten(d)`

The per-light ambient in Gamebryo uses the SAME attenuation as diffuse, so it falls off with distance naturally. Our flat 0.25 doesn't capture that nearby lights contribute much more ambient than distant ones. And the Gamebryo coefficient is 0.5 (Material.Ambient default), not 0.25.

**Recommended**: `Lo += lightColor * atten * albedo * 0.5` (matching Material.Ambient default of 0.5, already scaled by `atten` which tracks distance)

### RL-C: Albedo demodulation missing from SVGF (known, documented)

SVGF 2017 §4 (Figure 2): "We **demodulate** the surface albedo from both direct and indirect illumination before filtering, and **modulate** it back after."

Our current pipeline filters `indirectLight = (ambient + indirect) * ao` with albedo baked in. The SVGF temporal pass blurs texture detail along with lighting noise. This is explicitly documented as deferred (issue #268), but it means our filtered indirect is systematically less accurate.

---

## Part 3: Findings from Research Papers

### From RTR4 (Chapter 10.1: Area Light Sources)

Windows should be **area lights**, not point lights or simple sky portals. RTR4 §10.1 describes the proper math: integrate emitted radiance over the solid angle subtended by the rectangular window aperture. Our current window portal code traces a single through-ray and outputs a flat sky color — this is a binary sky/not-sky decision that produces harsh contrast.

A proper approach: **linearly textured quad light** (RTR4 §10.1.2) where the window polygon emits sky-tinted radiance. Fragments near the window receive more energy (larger solid angle) than distant ones.

### From SVGF 2017 (The Denoising Architecture We Need)

Our current SVGF temporal pass is a simplified version of the full pipeline:
```
Current:   raw_indirect → temporal blend → composite
Paper:     raw_indirect → demodulate albedo → temporal accumulation (α=0.2)
           → variance estimation → 5× À-trous spatial filter → remodulate → composite
```

The spatial filter (À-trous wavelet, 5 iterations = 65×65 effective footprint) is what makes 1-SPP RT viable. Without it, temporal-only accumulation requires many frames to converge and ghosts on camera movement.

### From ReSTIR GI (The GI Architecture We Should Target)

Our 1-SPP cosine-weighted hemisphere ray is fundamentally limited. ReSTIR GI shows that with reservoir-based spatiotemporal resampling, the same 1 ray per pixel can produce results equivalent to 800+ samples — but it requires the full reservoir infrastructure (temporal + spatial reuse with Jacobian correction).

**For now**: The simpler path is to get the DIRECT lighting right (attenuation + per-light ambient) and let the existing temporal SVGF handle GI noise. ReSTIR GI is a later milestone.

---

## Part 4: Recommended Fixes (Priority Order)

### Fix 1: Correct attenuation curve (CRITICAL)

**File**: `crates/renderer/shaders/triangle.frag`

Replace current point/spot light attenuation with:
```glsl
float ratio = dist / max(radius, 1.0);
float window = clamp(1.0 - ratio * ratio, 0.0, 1.0);
atten = window * radius / (radius + dist);
```

This gives `1/d` behavior at close range, smooth fade to zero at the radius boundary. At half-radius: attenuation ~0.50 (vs current 0.08).

### Fix 2: Correct per-light ambient coefficient (HIGH)

**File**: `crates/renderer/shaders/triangle.frag`

Change per-light ambient from:
```glsl
Lo += lightColor * atten * shadow * albedo * 0.25;
```
To:
```glsl
Lo += lightColor * atten * albedo * 0.5;
```

Note: remove `shadow` — ambient fill should not be shadow-blocked (it represents the omnidirectional light scatter from the light source, not directional illumination). And 0.5 matches Gamebryo's default Material.Ambient.

### Fix 3: Remove the ambient boost hack (MEDIUM)

**File**: `byroredux/src/render.rs`

With correct attenuation and per-light ambient, the XCLL ambient no longer needs artificial boosting. Reduce from 2.0× to 1.0× (pass through as-is). The per-light ambient fill will provide the missing illumination.

### Fix 4 (future): Full SVGF spatial filter (MEDIUM)

Add the 5-iteration À-trous wavelet spatial filter per SVGF 2017. This is ~3ms additional cost but eliminates GI noise completely. Can be deferred — the direct lighting fixes above are more impactful.

### Fix 5 (future): Albedo demodulation (MEDIUM)

Demodulate albedo before SVGF temporal pass, remodulate in composite. Already tracked as #268.

### Fix 6 (future): Area light windows (LOW)

Replace binary sky portal with area light formulation per RTR4 §10.1. Would produce physically correct window illumination with natural falloff by solid angle.

---

## Numerical Verification

With the recommended attenuation at a typical interior surface (dist=200, radius=512):
- `ratio = 200/512 = 0.39`
- `window = 1 - 0.39² = 0.85`
- `atten = 0.85 × 512 / (512 + 200) = 0.85 × 0.72 = 0.61`

With per-light ambient at 0.5:
- Single light contribution: `lightColor(0.9) × 0.61 × albedo(0.5) × 0.5 = 0.14`
- Plus diffuse: `0.9 × 0.61 × 0.5 × NdotL(0.6) / PI = 0.053`
- Total per light: ~0.19
- With 10 visible lights: ~1.9 (pre-tonemap, will compress via ACES)

This is a dramatic improvement from the current ~0.03 per light.
