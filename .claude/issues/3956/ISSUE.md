# LC-2026-09-06-D5-01: FO4/FO76 authored height-fog is decoded into WeatherHeightFog and dropped at the EXAL boundary, while the shader runs on a hardcoded 30 m scale height

Issue: #3956 · Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-06.md` (base `a8233f2f`)
Labels: medium, legacy-compat, terrain-exterior, bug, game:fo4, game:fo76

Reported by `/audit-legacy-compat` — `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-06.md` (base `a8233f2f`).

- **Severity**: MEDIUM (translatable block silently dropped by the exterior translate boundary, with a live canonical sink)
- **Dimension**: 5 — EXAL exterior environment → renderer
- **Location**: `byroredux/src/env_translate.rs` (`translate_exterior_cell_lighting` and `translate_weather` — neither reads `wthr.fog_height`); `crates/plugin/src/esm/records/weather.rs:496-510` (the decode); `crates/renderer/src/vulkan/context/draw.rs:811-821` (the sink); `crates/renderer/src/vulkan/volumetrics.rs:328` (`DEFAULT_SCALE_HEIGHT_METERS = 30.0`)
- **Status**: NEW

## Description

`parse_wthr`'s `FNAM` arm decodes FO4/FO76's ten-value height-fog extension (day/night near+far height mid and range, day/night high density scale — the fields xEdit attributes to form versions 119 and 120) into `WeatherRecord::fog_height: Option<WeatherHeightFog>`.

**Nothing outside the parser ever reads that field.** A repo-wide grep for `fog_height` returns only `crates/plugin`'s own decode, plus `fog_height_reference` — an unrelated render constant (the #2225 ground-anchor Y).

Meanwhile `composite.frag` has a live height-fog branch (`height_fog_params`, gated `is_exterior && extinction > 0`), and `draw.rs:813` fills its scale-height lane with the engine constant `DEFAULT_SCALE_HEIGHT_METERS * WORLD_UNITS_PER_METER` for **every** game. The authored quantity and the consumed quantity are the same physical parameter — decoded on one side of the EXAL boundary and hardcoded on the other.

## Evidence

The decode, `crates/plugin/src/esm/records/weather.rs:498`:

```rust
record.fog_height = Some(WeatherHeightFog {
    day_near_height_mid: r.f32().unwrap_or(0.0),
    day_near_height_range: r.f32().unwrap_or(10000.0),
    night_near_height_mid: r.f32().unwrap_or(0.0),
    night_near_height_range: r.f32().unwrap_or(10000.0),
    day_high_density_scale: r.f32().unwrap_or(1.0),
    night_high_density_scale: r.f32().unwrap_or(1.0),
    day_far_height_mid: r.f32().unwrap_or(0.0),
    day_far_height_range: r.f32().unwrap_or(10000.0),
    night_far_height_mid: r.f32().unwrap_or(0.0),
    night_far_height_range: r.f32().unwrap_or(10000.0),
});
```

gated `matches!(game, GameKind::Fallout4 | GameKind::Fallout76) && sub.data.len() >= FO4_FNAM_SIZE`, and pinned by `parse_fo4_fnam_retains_all_eighteen_floats` plus `starfield_long_fnam_does_not_assume_the_fo4_tail_schema` (which asserts an unverified Starfield tail must *not* be decoded as FO4 height fog — evidence the schema was taken seriously).

The sink, `crates/renderer/src/vulkan/context/draw.rs:811`:

```rust
height_fog_params: [
    fog_extinction_per_meter.max(0.0) / volumetrics::WORLD_UNITS_PER_METER,
    volumetrics::DEFAULT_SCALE_HEIGHT_METERS * volumetrics::WORLD_UNITS_PER_METER,
    fog_single_scatter_albedo.clamp(0.0, 1.0),
    if sky_params.is_exterior && fog_extinction_per_meter > 0.0 { 1.0 } else { 0.0 },
],
```

**The discriminating evidence** that this is a leak rather than a deliberate deferral: `fog_day_max` / `fog_night_max` are read by the *same* `SubReader` in the *same* `FNAM` arm, three lines earlier — and both `translate_weather` and `translate_exterior_cell_lighting` forward them into `FogMedium::from_legacy_ramp(near, far, Some(max))`. The boundary consumes half of one subrecord's tail and drops the other half without a note.

## Impact

On FO4 and FO76 exteriors the fog's altitude profile is the engine's 30 m default instead of the authored per-weather one — for every weather, in every worldspace. Fog still renders (nothing is missing or black), so this is a wrong-value divergence rather than a dropout, which is what keeps it at MEDIUM rather than escalating.

No other game is affected: the block is FO4/FO76-only by decode gate, and the Starfield tail is deliberately not assumed.

## Confidence

The code path is certain — decode present, zero consumers, hardcoded sink, all three grep-verified at `a8233f2f`.

**The occupancy is NOT measured.** No corpus census was run in the reporting sweep (only ~8 GB of 29 GB RAM was free, and whole-ESM parsing in `byroredux-plugin` has OOM-killed sessions before), so "how many vanilla FO4 weathers actually ship the 72-byte `FNAM`" is unknown, and the existing tests are synthetic. This is rated MEDIUM on the boundary defect, which is occupancy-independent — not on assumed content. **Census FO4/FO76 `FNAM` sizes before sizing the work.**

## Related

- #1926 / #1927 — the composite fog branch removed as unreachable (different fields, same fog subsystem)
- LC-2026-09-06-D5-02 — the `fog_*_power` half of the same subrecord tail (filed separately and de-rated, because its canonical field is currently shader-unconsumed)
- `docs/engine/exal.md` §2 "Weather / TOD — canonical; the cleanest of the dynamic categories" — which this contradicts

## Suggested Fix

Carry `WeatherHeightFog` onto `WeatherDataRes` / `CellLightingRes` as a canonical `Option`, TOD-lerped the way `skyrim_dalc_per_tod` already is, and let `draw.rs` prefer it over `DEFAULT_SCALE_HEIGHT_METERS` when `Some`. Keep the constant as the no-authored-data fallback — that is the EXAL-correct shape (an explicit canonical default, not a render-time branch).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the rest of the WTHR tail — `fog_*_power` (D5-02) and the nine capture-ahead-of-consumer fields the report lists as dropped candidate #5
- [ ] **LOCK_ORDER**: If a `RwLock` scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: the per-game decision stays at the EXAL parser→canonical boundary in `byroredux/src/env_translate.rs` — never pushed into the shader, never re-derived at render time
- [ ] **TESTS**: A regression test pins that an authored `fog_height` reaches `height_fog_params[1]` and that its absence still yields `DEFAULT_SCALE_HEIGHT_METERS`
