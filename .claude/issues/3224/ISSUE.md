# FO4-D6-2026-08-20-01: FO4's authored water colour-depth ramp is never read and fog_near is hardcoded to zero on every FO4 water body

**Issue**: #3224 · https://github.com/matiaszanolli/ByroRedux/issues/3224
**Finding ID**: FO4-D6-2026-08-20-01
**Filed**: 2026-08-20 (comprehensive audit suite)

Filed from `docs/audits/AUDIT_FO4_2026-08-20.md` § FO4-D6-2026-08-20-01 (Dimension 6 — FO4 data through the ESM parser).

**Severity**: MEDIUM · **Status**: NEW — delta-local (the sibling `alpha_controls` capture landed this cycle in `1a428278` "depth-dependent alpha controls"; the colour half did not follow)
**Location**: `crates/plugin/src/esm/records/misc/water.rs` — `decode_dnam_fo4`, the `read_f32_at(data, 0)` block at the head of the function and the `alpha_controls.zip([20, 24, 28, 32])` loop below it

## Description

xEdit `dev-4.1.6` (`wbDefinitionsFO4.pas`) lays FO4's `WATR.DNAM` *Fog Properties* block
out as **two pairs** of depth thresholds, each scaling `Depth Amount` @0:

- a **colour** ramp — `Color Shallow Range` @12, `Color Deep Range` @16
- an **alpha** ramp — `Alpha Shallow Range` @28, `Alpha Deep Range` @32

`decode_dnam_fo4` captures the alpha pair into `alpha_controls[2..4]` and ignores the
colour pair entirely. Instead it writes:

```rust
if let Some(depth_amount) = read_f32_at(data, 0) {
    p.fog_near = 0.0;                        // <- hardcoded, discards @12
    p.fog_far = depth_amount.max(1.0);       //    and @16
}
```

There is no deferral comment, no `TODO`, and no mention of either field anywhere in the
tree (`grep -rn "color_deep_range\|Color Deep Range" crates byroredux docs` -> 0 hits),
unlike the several BGSM fields whose absent sinks carry explicit "deferred" notes.

## Evidence

Measured over all 47 vanilla FO4-family `WATR` records carrying a `DNAM`
(`Fallout4.esm` 42 + 5 in DLC; `DNAM` is 201 B x 45 and 188 B x 2):

| DNAM offset | xEdit field | distinct values / 47 | read by engine |
|---|---|---|---|
| 0 | Depth Amount | 24 | yes -> `fog_far` |
| **12** | **Color Shallow Range** | **8** | **no** |
| **16** | **Color Deep Range** | **39** | **no** |
| 20 | Shallow Alpha | 6 | yes |
| 24 | Deep Alpha | 1 | yes |
| 28 | Alpha Shallow Range | 6 | yes |
| 32 | Alpha Deep Range | 37 | yes |

`Color Deep Range` is the *second most* individually-authored field in the whole fog
block — 39 distinct values across 47 records. It is not a constant the parser can
safely default.

The discarded `fog_near` is **not inert**: it is live all the way to the shader —
`byroredux/src/env_translate.rs:556` (`mat.fog_near = rec.params.fog_near`) ->
`day_fog_near` / `night_fog_near` (`:609`, `:614`) -> `byroredux/src/render/water.rs:187`
(day/night lerp) -> `:235` (UBO upload).

The rest of `decode_dnam_fo4` was checked offset-by-offset against the same xEdit
definition and **all 30 other reads are correct** — this decoder is the reference
implementation for the family (the FO4 half of the WATR arbitration, #3104-#3110).
The colour ramp is the single gap.

## Impact

Every FO4 water body blends shallow tint -> deep tint starting at depth 0 and finishing
at the full `Depth Amount`, instead of over the authored
`[depth * shallow_range, depth * deep_range]` band. On the 38 records where
`Color Shallow Range` is 1.0 the shallow tint should barely appear near the surface;
the engine gives it the widest possible band. Diamond City's ponds, the Quannapowitt
lake, and every Institute/Vault interior pool are in scope.

MEDIUM, not HIGH: the water still renders and the alpha ramp — the more visually
dominant of the two — is correct.

## Suggested Fix

Add a `color_range: [f32; 2]` lane to `WaterParams` beside `alpha_controls` (or widen
`alpha_controls` into a named 6-lane struct), read offsets 12/16, and set
`fog_near = depth_amount * color_shallow_range` /
`fog_far = depth_amount * color_deep_range` rather than `0.0` / `depth_amount`.
Pin with a fixture asserting a non-zero `fog_near` for an FO4 `DNAM` whose offset 12 is
< 1.0.

## Related

- `1a428278` — the alpha-controls commit that established the sink shape
- #2785 (CLOSED) — `WaterMaterial::fog_near` travelling the EXAL arm unread; it *is* read
  now, which is what makes this live
- #3104-#3110 — the WATR layout arbitration. FO4's decoder is the confirmed-correct
  reference there; **do not "fix" the other games' offsets by analogy with this one.**
- #3132 (WATR floats reach the water UBO without a finiteness gate)

## Completeness Checks
- [ ] **SIBLING**: same colour/alpha-pair question checked in `decode_dnam_fo76` and `decode_dnam_starfield` (both share the FO4-derived head)
- [ ] **CANONICAL-BOUNDARY**: the new lane is decoded at the ESM parser boundary and carried through `WaterParams` -> `WaterMaterial` -> `GpuWaterParams`; no per-game branch added in the shader
- [ ] **TESTS**: a regression test pins a non-zero `fog_near` for an FO4 `DNAM` with offset 12 < 1.0
