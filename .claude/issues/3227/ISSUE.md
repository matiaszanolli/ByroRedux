# SF-2026-08-20-D8-02: env_translate clamps Starfield water concentrations to 0..1 but vanilla authors 0.019..20.0

**Issue**: #3227 · https://github.com/matiaszanolli/ByroRedux/issues/3227
**Finding ID**: SF-2026-08-20-D8-02
**Filed**: 2026-08-20 (comprehensive audit suite)

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-20.md` § SF-2026-08-20-D8-02 (Dimension 8 — NIFAL / WATAL canonical translation).

**Severity**: MEDIUM · **Status**: NEW — **the feature this breaks shipped in this delta** (`900b028c` "concentration controls", `4c81531f` "oceanness")
**Location**: `byroredux/src/env_translate.rs:865-869` (the clamp); consumer `crates/renderer/shaders/water.frag:477-487`; parser source `crates/plugin/src/esm/records/misc/water.rs` (`decode_dnam_starfield`, the `concentration.zip([16, 20, 24, 28])` loop)

## Description

`900b028c` carried the Starfield concentration block through to the GPU, and `4c81531f`
made its fourth component (oceanness) feed both the absorption density and the
forward-scattering term. Translation clamps the whole quad to `0..1`:

```rust
for (dst, src) in mat.concentration.iter_mut().zip(rec.params.concentration) {
    if src.is_finite() && src > 0.0 {
        *dst = src.clamp(0.0, 1.0);
    }
}
```

The authored values are **not** in `0..1`. The clamp turns a per-water discriminator
into a constant. (Note the immediately preceding loop in the same function clamps its
own field to `0.01 ..= 100_000.0` — the `0..1` window here is not a house style, it is
a wrong range.)

## Evidence — all 15 `Starfield.esm` records, DNAM 16/20/24/28

```
WaterClear        8.840   6.594   4.710  | 0.514      WaterMudBrown   7.392  15.580  18.550 | 1.000
WaterSulfuric     0.000  19.348  20.000  | 0.000      WaterSilt       2.682  20.000   2.898 | 0.000
WaterOceanAlgae  19.420   7.608  11.232  | 1.000      WaterAlgae     17.608  15.652  16.812 | 0.000
ENV_Test...       0.059   0.436   0.019  | 1.630
```

**41 of 60** authored values exceed `1.0`; the authored range is `0.019 … 20.0`.

Feeding the clamped result through the shader's own expression
`clamp(dot(conc.rgb, vec3(0.25, 0.50, 0.25)) + conc.a * 0.25, 0, 1)` gives
`concentrationDensity == 1.0` **exactly** on **12 of 15** records. The remaining three
are `0.750` (`WaterSulfuric`, one channel authored 0), `0.925` (`WaterClearSterile`) and
`0.487` (the OBSOLETE test record).

So `WaterClear` and `WaterMudBrown` — the two records whose concentrations are most
obviously meant to differ — produce a **bit-identical** density.

**Oceanness's absorption contribution is fully masked as a side effect**: the RGB dot
term already reaches `1.0` before `conc.a * 0.25` is added, so the `4c81531f`
oceanness-into-absorption path is **dead on 12 of 15 records**. (Its *other* use,
`oceanScatter` at `water.frag:1061`, reads `push.concentration.a` directly and does
survive — 0.0 / 0.514 / 0.699 / 1.0 across the set.)

## Impact

The "carry Starfield concentration controls" feature reaches the GPU but **conveys no
information for vanilla content**: every ocean, lake, puddle, algae pool and mud pool
gets the same optical density. Bounded and non-crashing, hence MEDIUM rather than HIGH
— but it is also the `(1.0 + concentrationDensity)` factor that **doubles** the
attenuation rate in SF-2026-08-20-D8-01 on 12 of 15 records.

## Why the tests miss it

`resolve_water_material_carries_starfield_absorption_ranges`
(`byroredux/src/env_translate.rs:2496`) uses `concentration: [0.2, 0.4, 0.6, 0.8]` —
every component chosen **inside** the clamp window, so the `assert_eq!` passes and the
clamp is never exercised.

## Suggested Fix

Do **not** clamp at the translation boundary. Either carry the authored magnitude and
normalise inside the shader against a documented reference concentration, or normalise
at the boundary by a source-derived maximum — and pin it with a real-data test asserting
that `WaterClear` and `WaterMudBrown` produce *different* `concentration` vectors.

## Related

- **#3226** (SF-2026-08-20-D8-01) — the absorption divisor; this is the factor that doubles its rate
- #3148 (ESM-2026-08-20-D8-01) — the real-data WATR guard that cannot detect this class
- #3208 — `resolve_water_material` is 522 LOC, which is why this clamp sits unnoticed among 40+ field writes
- #2888

## Completeness Checks
- [ ] **SIBLING**: the same `0..1` clamp checked against the other Starfield-only lanes written in the same `resolve_water_material` arm
- [ ] **CANONICAL-BOUNDARY**: normalisation is decided once at the EXAL/WATAL translation boundary (`env_translate.rs`) or once in the shader — not re-derived at both
- [ ] **TESTS**: a real-data assertion pins `WaterClear` != `WaterMudBrown`, so an in-window synthetic fixture can no longer pass vacuously
