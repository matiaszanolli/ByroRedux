# FNV-RT-2026-08-26-03

**Issue**: #3323
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 3 — RT Lighting Pipeline
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/renderer/shaders/triangle.frag:1650-1672`,
`byroredux/src/render/sky.rs:56-79`, `crates/renderer/src/vulkan/context/mod.rs:883-892`,
`crates/renderer/src/vulkan/context/draw.rs:2312-2317`

**Premise verified**: the window-portal escape branch transmits `skyTint.rgb` with **no**
interior gate — unlike the two glass *miss* fallbacks, which #1125 correctly gated on
`isExteriorGlass` (`include/raytrace.glsl:46` for the reflection miss and
`triangle.frag:2163` for the refraction miss; both verified live, see Regression guards
below). Meanwhile:
- `sky.rs:70-77` — `build_sky_params` decides interiority once from `CellLightingRes` and,
  when interior, returns `SkyParams { dalc_cube, ..SkyParams::default() }`. Every TOD/weather
  field is dropped deliberately (#1199 / #2226: an interior must never read a stale exterior
  `SkyParamsRes`).
- `context/mod.rs:886` — `SkyParams::default().zenith_color = [0.15, 0.3, 0.6]`.
- `draw.rs:2312-2317` — `sky_tint = [zenith_color.xyz, sun_angular_radius]`.

So on any FNV interior `skyTint.rgb` is *always* `(0.15, 0.3, 0.6)`. The #925 comment sitting
directly above the site claims the opposite — "pull the sky colour from the active TOD/weather
palette … so interior windows cross-fade with night / dawn / dusk / storm … Pre-fix this was
hardcoded `vec3(0.6, 0.75, 1.0)` and Megaton / Vault 21 interiors always looked midday."
The hardcoded literal moved from the shader into `SkyParams::default()`; the behaviour it
described did not change for interiors.

**Impact (FNV-visible)**: architectural glass in FNV interiors that passes the portal test
(`materialKind == GLASS`, render layer Architecture, `texColor.a ∈ (0.02, 0.5)`,
`rtLOD < 2`, `dot(-V,N) > 0.1`, escape ray clears 2000 BU) transmits the same daylight blue
at 03:00 as at noon — Vault 21/34/22 window walls, the Novac motel-room and Dino Dee-lite
panes, and any Prospector/Strip interior with an exterior-facing pane. This is a *narrower*
version of the very symptom #925 says it fixed, still live because that fix only reached
exteriors.

**Fix sketch**: this is a policy question, not a one-line gate — an interior window genuinely
*does* see sky, so dropping to `sceneFlags.yzw` (the #1125 treatment) would be wrong here.
The correct source is the worldspace's current TOD sky colour, which an interior deliberately
does not upload. Either (a) plumb a separate `exteriorSkyTint` lane on `GpuCamera` that
survives interior cells (weather sim already runs — `CloudSimState` is documented as surviving
cell transitions), or (b) accept and re-document the limitation at both sites and delete the
now-false #925 claim. Do **not** simply widen the interior `SkyParams` bypass — that is
exactly what #2226 removed.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
