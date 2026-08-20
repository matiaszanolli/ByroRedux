# SF-2026-08-20-D8-01: Starfield per-channel absorption is applied as a divisor, inverting the hue and driving every vanilla Starfield water opaque within 0.18 world units

**Issue**: #3226 · https://github.com/matiaszanolli/ByroRedux/issues/3226
**Finding ID**: SF-2026-08-20-D8-01
**Filed**: 2026-08-20 (comprehensive audit suite)

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-20.md` § SF-2026-08-20-D8-01 (Dimension 8 — NIFAL / WATAL canonical translation).

**Severity**: HIGH · **Status**: NEW · **Game**: Starfield only
**Location**:
- `crates/renderer/shaders/water.frag` — `absorbWaterColumn`, the `channelTransmission = exp(...)` block (`:493-500` at `bb0b92f2`)
- `byroredux/src/systems/water.rs` — `underwater_color_at_depth`, the `(-optical_depth / range).exp()` line (`:512`)
- parser source: `crates/plugin/src/esm/records/misc/water.rs` — `decode_dnam_starfield`, the `absorption_ranges.zip([4, 8, 12])` loop
- contract text: `crates/core/src/ecs/components/water.rs:240-243`, `docs/engine/watal.md:321-322`

## Description

Both consumers treat `absorption_ranges` as a **1/e distance** and *divide* by it:

```glsl
// water.frag — absorbWaterColumn
channelTransmission = exp(
    -hitDist * (1.0 + concentrationDensity)
    / max(authoredRanges, vec3(0.01))
);
```

```rust
// byroredux/src/systems/water.rs — underwater_color_at_depth
(color[index] * (-optical_depth / range).exp()).clamp(0.0, 1.0)
```

The vanilla data is ordered like an **extinction coefficient**, not a distance.
Dividing therefore attenuates the *smallest*-valued channel the most — blue — so the
surviving radiance is red, and the magnitudes drive the whole column opaque almost
immediately.

## Evidence — all 15 `Starfield.esm` WATR records, DNAM 4/8/12

Walked with an independent out-of-tree ESM reader (no `cargo`, no engine launch):

```
R = 0.16558   G = 0.09624   B = 0.07627     on 14 of 15 records
R = 0.30000   G = 0.07500   B = 0.01000     WaterSulfuric (the only variant)
```

Two independent arguments, both unit-free:

**1. Ordering.** `R > G > B` on every record. As an absorption *distance* that says red
penetrates 2.17x further than blue — i.e. `WaterClear`, `WaterOceanClear`,
`WaterClearLake` and `WaterClearNitrogen` all render red-transmitting. As a
*coefficient* the same ordering is the textbook water curve (red absorbed first).
**Rescaling the unit cannot flip an ordering.**

**2. Magnitude.** Solving `channelTransmission < 0.01`, using each record's real
`concentrationDensity`:

| record | opaque at, current `/range` | opaque at, `*coeff` |
|---|---:|---:|
| `WaterClear` / `WaterOceanClear` / 10 others | **0.176 wu** | 13.9 wu |
| `WaterSulfuric` | **0.026 wu** | 8.8 wu |
| `ENV_Test_CorrosivePuddle_OBSOLETE` | **0.236 wu** | 18.7 wu |

0.18 world units is under 3 mm at Bethesda scale. The **reciprocals** of the authored
triple — 6.0 / 10.4 / 13.1 wu — are commensurate with the same record's own DNAM-0
depth value (3.45-20), which is what a real range triple would look like.

The underwater half is the same defect at a second site: at 1 wu of camera submersion
`underwater_color_at_depth` returns `color * exp(-6.0)` ~= `color * 0.0025`, so the
authored underwater tint is **black** on every Starfield water.

## Reachability — this is live, not theoretical

`Starfield.esm` authors **6,550 `XCLW`** water-height sub-records and **0 `XCWT`**
anywhere, so exterior cells take the worldspace `NAM2` fallback at
`byroredux/src/cell_loader/exterior.rs:1145`
(`cell.water_type_form.or(wctx.default_water_type_form)`) into these same 15 WATR
records. The feature matrix lists Starfield exterior grid / LAND / world streaming as
supported.

Other games are unaffected: the shader branch is gated on
`any(greaterThan(authoredRanges, vec3(0.0)))` and only `decode_dnam_starfield`
populates the field (FO76's decoder drops it — #3106).

## Impact

Every Starfield water surface renders as flat `deep_color` with no refraction and a red
cast in the vanishing near band, and the underwater post-process tint is crushed to
black. HIGH: it defeats the entire refraction/absorption path on a supported game, at
both the surface and underwater sites.

## ⚠️ Why the tests miss it — green by construction

Both guarding tests use synthetic fixtures that **encode the distance hypothesis and
therefore cannot falsify it**:

- `resolve_water_material_carries_starfield_absorption_ranges`
  (`byroredux/src/env_translate.rs:2490-2510`) uses `absorption_ranges: [12.0, 34.0, 56.0]`
  — a synthetic triple 100-700x the real values **and in the opposite order**.
- `starfield_absorption_attenuates_channels_independently_after_near_plane`
  (`byroredux/src/systems/water.rs`) uses `[10.0, 20.0, 40.0]` — same shape.

The only real-data guard, `installed_masters_water_fields_are_finite_and_ordered`,
asserts finiteness and `far >= near`, both of which survive.

This is **row 16 of the 16 green-by-construction instances** catalogued in
`docs/audits/AUDIT_SUITE_SUMMARY_2026-08-20.md` ("Starfield | Absorption tests |
Synthetic fixtures encode the distance hypothesis; cannot falsify it").

## Suggested Fix

**Establish the field's authored unit from a source before editing — do not pick a
constant** (no-guessing policy). If the values are extinction coefficients, both sites
become `exp(-distance * coeff)` and the field must be renamed off `_ranges` in
`WaterParams`, `WaterMaterial`, `GpuWaterParams`, both shaders and `watal.md`.

*Auditor's note for whoever resolves the unit*: `decode_dnam_starfield`'s own comment
cites the xEdit field name as **"Color Absorbtion Ranges"**, which is the strongest
piece of counter-evidence to the coefficient reading and is exactly why this needs a
byte-level source rather than a judgement call. The ordering and magnitude arguments
above stand on their own regardless of what the field is *named*.

Whatever the outcome, **replace the two synthetic fixtures with a real-data assertion**
pinned to a named `Starfield.esm` record — e.g. `WaterOceanClear` must stay meaningfully
transmissive at 5 world units.

## Related

- #3228 (SF-2026-08-20-D5-01) — DNAM[0] -> `fog_far` produces a 3.45-20 wu ramp; compounds this
- #3227 (SF-2026-08-20-D8-02) — the `(1.0 + concentrationDensity)` factor that doubles the attenuation rate, saturated to `1.0` on 12 of 15 records
- #3106 (WATR-ARB-03) — FO76 never reaches this path
- #3148 (ESM-2026-08-20-D8-01) — the blind real-data WATR guard
- #3124 — `GpuWaterParams` has three declaration sites and no lockstep guard
- #2785

## Completeness Checks
- [ ] **SIBLING**: both consumers fixed together — `water.frag`'s `absorbWaterColumn` **and** `systems/water.rs`'s `underwater_color_at_depth`; the field doc in `crates/core/src/ecs/components/water.rs` and `docs/engine/watal.md:321-322` updated in the same change
- [ ] **CANONICAL-BOUNDARY**: the unit decision lands at the WATAL parser -> `WaterMaterial` boundary, not as a per-game branch in the shader
- [ ] **TESTS**: the two synthetic fixtures are replaced by a real-data assertion naming a specific `Starfield.esm` record, so the fix cannot be green-by-construction again
