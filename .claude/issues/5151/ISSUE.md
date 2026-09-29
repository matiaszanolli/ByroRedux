# SF-2026-09-29-D4-01: Starfield WATR distance and inverse-distance fields are metric but reach WATAL unconverted — absorption 70× too strong, normals flatten 1.4 m from the camera, underwater fog saturates at ~1 m

**Labels**: high,bug,water,esm-plugin,game:starfield,legacy-compat

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-29.md`
**Severity**: HIGH
**Dimension**: ESM → cell bring-up (WATAL handoff)
**Location**:
- `crates/plugin/src/esm/records/spatial_units.rs` — `normalize` has no `index.waters` arm;
- `crates/plugin/src/esm/records/misc/water.rs` — `decode_dnam_starfield` (shared with FO76 via `decode_dnam_fo76`), incl. the "same 300-unit default across the three" comment on `noise_falloff`;
- consumers: `crates/renderer/shaders/water.frag` (`absorbWaterColumn`, the `noiseFalloff` normal fade, the noise UV), and `byroredux/src/systems/water.rs` (`compute_underwater_params`, `underwater_color_at_depth`).

## Description
b9e961eeb moved Starfield scene data into engine units (70 BU/m) at the parse boundary, but left the WATR record out. Starfield's WATR DNAM is authored in metres, and every distance-bearing lane reaches `WaterMaterial` as metres where BU are expected:
- **`absorption_coefficients` (DNAM 4/8/12)** are per-metre extinction coefficients (0.16558 / 0.09624 / 0.07627 on 14 of 15 vanilla WATRs; `WaterSulfuric` authors 0.3 / 0.075 / 0.01). They are inverse lengths, so the fix is ÷70, not ×70. `water.frag` applies `exp(-hitDist * coeff)` with `hitDist` in BU, and `underwater_color_at_depth` does the same with `depth` in BU.
- **`noise_falloff` (DNAM 132)** is 100 on all 15 vanilla records (100 m). `water.frag` fades normals to flat by `1 - dist/noiseFalloff`, so every Starfield water surface loses its normal detail 100 BU (1.4 m) from the camera. The decoder comment says "the same 300-unit default across the three"; the real value is 100.
- **`noise_uv_scale_{a,b,c}` (DNAM 120/124/128) — not settled**: tile sizes 72.11 / 39 / 13. `normalize_noise_uv_scale` inverts them and `uvWorld = vWorldPos.xz` is in BU, so as shipped the primary noise repeats every 72 BU (~1 m); read as metres the tile would be 5,048 BU. FO76 (BU, same decoder) authors 279 / 168 / 56, so BU and metric readings are 3.9× and 18× off FO76 respectively. The data alone cannot decide this — settle it with a capture before lifting (no-guessing policy).
- **`underwater_fog_near/far` (DNAM 40/44)** are −150 / 75 m. Clamped → 0 / 75, so underwater fog saturates 75 BU (≈1 m) from the camera. FO76 authors −9,000 / 850 BU and FO4 −6,000 / 1,100.
- `depth_amount` (DNAM 0; 8 m vs FO4 471–1,087 BU) is packed into `optical.x` but read by no shader today; lift it only for consistency.

## Evidence
Read-only raw DNAM dump of all 15 `Starfield.esm` WATRs (e.g. `WaterClear` 0x18: `8 | 0.16558 0.096239 0.076271 | … | -150 75 | … | 72.1141 39 13 | 100 100 100 | 1 0.08`) compared with `SeventySix.esm` (`-9000 850`, falloff `4096`). The absorption triplet is the strongest unit evidence: red > green > blue and per-metre magnitude match liquid-water extinction. Read per BU it would be ~11.6 m⁻¹ for red (opaque at hand depth). Red transmission through 70 BU (1 m) is `exp(-70·0.16558)` = **9.3e-6 as shipped** vs 0.847 with per-metre applied per metre; through 20 BU (28 cm) 0.037 vs 0.954. There is no Starfield branch in `resolve_water_material` (`byroredux/src/env_translate.rs`) and no `BETHESDA_UNITS_PER_METER` use in the WATAL path.

## Impact
Every Starfield water body turns opaque deep tint within ~30 cm of depth, normals go flat beyond ~1.4 m, and the camera sees ~1 m of underwater visibility. Covers cell water (XCLW/XCWT) and placed-water meshes (`apply_placed_water_type`), since `cell_loader/water.rs` has no game gate. Latent only in that no gate exercises a Starfield water scene (`m-exteriors.sh` skips Starfield).

## Related
#5134 (EXT-D1-2026-09-29-01 — same root cause for WTHR fog; lists this WATR case as an unverified sibling, verified here with data); #5001 (SF-2026-09-29-D4-02); #4837 (open, Starfield oceanness ÷20 — distinct lane); #4285 (closed, WATAL concentration normalisation); #3226 (closed, earlier Starfield absorption divisor bug).

## Suggested Fix
Add an `index.waters` arm to `spatial_units::normalize` (already `GameKind::Starfield`-gated, so FO76's shared decoder stays in BU): multiply `underwater_fog_near/far`, `noise_falloff` and `depth_amount` by 70, and divide `absorption_coefficients` by 70. Decide the noise UV tile sizes separately, with a capture. Pin the lift with a `spatial_units_tests` case carrying the vanilla `WaterClear` DNAM. Fix the "300-unit default" comment.

Validated at HEAD 9fcfdc3fc: `spatial_units::normalize` lifts cells/statics/scols/worldspaces/lighting_templates/navmeshes only (no waters arm); `decode_dnam_starfield` stores absorption/falloff/fog values raw; `water.frag` `absorbWaterColumn` and the `noiseFalloff` fade consume them in BU.

## Completeness Checks
- [ ] **SIBLING**: FO76 (shared `decode_dnam_starfield`) stays in BU — the lift keys on `GameKind::Starfield` only
- [ ] **CANONICAL-BOUNDARY**: the unit lift stays at the parse boundary (`spatial_units::normalize`), never in `water.frag` / the renderer
- [ ] **TESTS**: A regression test pins this specific fix (`WaterClear` DNAM fixture through `normalize`)
