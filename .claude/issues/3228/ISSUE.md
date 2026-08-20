# SF-2026-08-20-D5-01: Starfield's DNAM[0] maps to fog_far, producing a 3.45-20 world-unit above-water ramp where every other game authors 58-4,710

**Issue**: #3228 · https://github.com/matiaszanolli/ByroRedux/issues/3228
**Finding ID**: SF-2026-08-20-D5-01
**Filed**: 2026-08-20 (comprehensive audit suite)

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-20.md` § SF-2026-08-20-D5-01 (Dimension 5 — ESM + cell bring-up regression surface).

**Severity**: LOW · **Status**: NEW
**Location**: `crates/plugin/src/esm/records/misc/water.rs` — `decode_dnam_starfield`, the opening `read_f32_at(data, 0)` block (`:1163-1166`); consumer `byroredux/src/env_translate.rs:557`; GPU sink `crates/renderer/src/vulkan/water.rs:81` (`deep.a`)

## Description

`decode_dnam_starfield` opens with `p.fog_near = 0.0; p.fog_far = depth.max(1.0)`
reading DNAM offset 0. On all 15 vanilla records that field is `3.45 … 20.0`.

`WaterParams::default()` is `fog_near 80.0 / fog_far 600.0`, and the same field measures
`86.0` on 39 of 47 FO76 records, `58` on FNV's `NVCleanWaterGS`, and `110 … 4,710` on
Skyrim — a **5-75x scale difference** for a field the parser treats as the same
quantity.

## Evidence

Measured over `Starfield.esm`, DNAM offset 0, n = 15:

```
8.0 x2   4.0 x2   12.0 x2   3.45   5.0   10.0   12.5   7.0   3.5   5.17   20.0   9.0
```

`deep.a` is also the refraction-**miss** distance handed to `absorbWaterColumn`
(`water.frag:962-965`), so the same value doubles as a ray-length fallback.

## Impact

The shared `t = clamp((hitDist - fogNear) / span)` ramp (`water.frag:468`) saturates
within 8 world units on Starfield instead of hundreds, so the legacy scalar absorption
term is pinned at its far end for essentially every fragment. This **compounds** the
absorption-divisor defect (SF-2026-08-20-D8-01) but is independent of it.

## ⚠️ Premise caveat — read before "fixing"

The audit could **not** establish what DNAM 0 actually *means* on Starfield. The
`/audit-esm` census labels it *"depth amount"*, which is not necessarily a distance in
the same units as Skyrim's fog range. This finding reports the **divergence**, not a
corrected semantics — per the no-guessing policy it needs a byte-level source (xEdit SF1
`wbStruct(DNAM)`, or a shipped-water screenshot comparison), not a chosen constant.

That is also why it is LOW: the observation is solid, the corrective action is not yet
determined.

## Suggested Fix

Decide the field's semantics from a source before changing anything.

- If it is a **depth scale** rather than a fog distance, it needs its own `WaterParams`
  slot and `fog_near`/`fog_far` should stay at their defaults for Starfield.
- If it is a **distance**, record the unit in the field doc so the 75x spread stops
  looking like a bug.

## Related

- **#3226** (SF-2026-08-20-D8-01) — the absorption divisor this compounds
- #3227 (SF-2026-08-20-D8-02)
- #3106 (WATR-ARB-03 — the FO76 decoder whose DNAM layout is the Starfield one minus the trailing roughness float)
- #3224 — the FO4 sibling: the same `fog_near = 0.0` hardcode, there with a *known* correct source (DNAM 12/16)

## Completeness Checks
- [ ] **SIBLING**: whatever unit is established is reconciled against `decode_dnam_fo4` / `decode_dnam_fo76`, which share the same DNAM head
- [ ] **CANONICAL-BOUNDARY**: if a new slot is needed it is added to `WaterParams` -> `WaterMaterial` -> `GpuWaterParams` at the parser boundary, not branched on `GameKind` in the shader
- [ ] **TESTS**: whichever reading wins is pinned by a real-data assertion naming a specific `Starfield.esm` record
