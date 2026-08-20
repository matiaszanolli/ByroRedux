# #3148 — ESM-2026-08-20-D8-01: the only real-data WATR guard can detect nothing but NaN — every clamp runs before the assertion, and FO76 is not in its master list

**Finding**: ESM-2026-08-20-D8-01
**Labels**: bug, import-pipeline, low, legacy-compat
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3148

---

- **Severity**: LOW
- **Dimension**: ESM Dim 8 — real-data validation
- **Record / Sub-record**: `WATR`
- **Location**: `crates/plugin/tests/parse_real_esm.rs:250-370` (`installed_masters_water_fields_are_finite_and_ordered`)
- **Status**: NEW

## Description

`installed_masters_water_fields_are_finite_and_ordered` is the **only** real-data guard over the WATR schema — the one test in the tree that reads shipped bytes from vanilla masters and checks the translated result. It cannot detect an offset-map error, and it omits Fallout 76 entirely.

What it actually asserts, over every record of every listed master:

1. `is_finite()` on 17 scalars plus the four colour triples, `noise_amplitude_scales`, `depth_weights`, `effect_controls`, `absorption_ranges` and the two silt colours (`:334-344`)
2. `p.fog_far >= p.fog_near` (`:346`)
3. `p.underwater_fog_far == 0.0 || p.underwater_fog_far >= p.underwater_fog_near` (`:347-350`)
4. For Starfield only: at least one record has an authored `absorption_ranges` entry `> 0.0` (`:358-363`)

Assertions 2 and 3 **cannot fail**, because the parser clamps every read *before* the assertion sees it: `decode_data*` and `decode_dnam*` apply `.max(0.0)` on `fog_near`, then `fog_far = value.max(p.fog_near + 1.0)`, and the same pattern on the underwater pair. The ordering invariant is established by the decoder, so the test re-asserts the decoder's own postcondition. Assertion 1 can only catch NaN/Inf.

**All of the offset-map errors filed in #3104–#3110 pass this test green.** Reading a displacement starting size as a normal magnitude, reading a 90.0 degree value into a radians field, reading a float's byte pattern as an RGBA colour — every one of them produces finite, correctly-ordered values.

## Evidence

The master list at `:257-294` names six games:

```
BYROREDUX_OBL_DATA        Oblivion.esm
BYROREDUX_FNV_DATA        FalloutNV.esm
BYROREDUX_FO3_DATA        Fallout3.esm
BYROREDUX_SKYRIMSE_DATA   Skyrim.esm
BYROREDUX_FO4_DATA        Fallout4.esm
BYROREDUX_STARFIELD_DATA  Starfield.esm
```

`SeventySix.esm` is absent — despite FO76 data being on disk (47 `WATR` records, all `DNAM` 148 B) and despite a FO76-specific decoder (`decode_dnam_fo76`) existing in the tree and being wholesale wrong (#3106). The one decoder in the file with no real-data coverage at all is the one that turned out to be decoding the wrong layout.

## Impact

A real-data guard that can only detect NaN is not a schema guard. Its presence is actively harmful: it is the artefact a reader points at to argue the WATR decoders are validated against shipped bytes, and it certified all seven arbitration defects as fine.

## Related

- #3109 — three *unit* fixtures in `water.rs` that pin the swapped labelling as expected behaviour. That issue's suggested fix asks for a new real-data assertion in this same file; this issue is about the guard that already exists there being structurally blind, and about the missing FO76 row. Fix them together.
- #3106 — `decode_dnam_fo76`, the decoder this test does not cover.
- Same defect class as **ESM-D2-07** (2026-08-13) and the `AVIF` fixture called out on 2026-08-16: a fixture that encodes the hypothesis under test cannot falsify it.

## Suggested Fix

1. Add `SeventySix.esm` to the master list.
2. Add assertions that an offset-map error *can* fail. The strongest signal available, and the one that actually caught #3104, is **invariance**: assert that no scalar folded into `noise_amplitude_scales` is bit-identical across a game's entire WATR population. A field constant across 34 authored records is a simulator default, not an authored control.
3. Add per-game corpus assertions with named records and expected values — e.g. `absorption_ranges[0] > 0` and `underwater_fog_far > 0` on a named `SeventySix.esm` record; `underwater_fog_far > underwater_fog_near` and `noise_wind_speeds[0] > 0` on a named `FalloutNV.esm` record.
4. Move the clamp-dependent ordering assertions to unit tests of the clamp itself, where they mean something, and stop counting them as real-data coverage.

---
*Filed from `docs/audits/AUDIT_ESM_2026-08-20.md` (Dim 8). Verified against HEAD `bb0b92f2` before filing.*

## Completeness Checks
- [ ] **SIBLING**: the other real-data guards in `crates/plugin/tests/parse_real_esm.rs` checked for the same clamp-before-assert blindness
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16 — different bits, different formats
- [ ] **TESTS**: the strengthened guard is verified to *fail* against at least one of the #3104–#3110 defects before those are fixed, otherwise it is still blind
