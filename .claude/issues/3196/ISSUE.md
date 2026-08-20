# #3196 — FNV-2026-08-20-D1-01: the new FNAM & 0x02 reflective gate (02c0d4b6) zeroes authored reflectivity on 36 of 78 vanilla FNV WATR records — and FNV's own data disproves the premise

**Source**: `docs/audits/AUDIT_FNV_2026-08-20.md`
**Filed**: 2026-08-20 · **HEAD**: `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3196

---

- **Severity**: HIGH
- **Dimension**: FNV Dim 1 — Cell Loading End-to-End (WATR → canonical water); impact lands in Dim 3 (RT reflections)
- **Location**: `byroredux/src/env_translate.rs:716-725` (`resolve_water_material`), fed by `crates/plugin/src/esm/records/misc/water.rs:1305-1307` (`legacy_flags`)
- **Status**: NEW — **regression introduced in this delta** by `02c0d4b6` ("fix(water): honor legacy reflective flags", 2026-08-19), a 51-line commit whose body is its subject line

## Description

`resolve_water_material` reads authored reflectivity out of the record and then overrides it to zero when the `FNAM` byte does not have bit `0x02`:

```rust
mat.reflectivity = rec.params.reflectivity;
// FO3/FNV's legacy FNAM bit 0x02 is an explicit reflective
// surface gate. Preserve authored reflectivity when the flag is
// present; when it is explicitly absent, suppress the RT
// reflection contribution instead of making non-reflective
// sludge/puddle records mirror-bright.
if rec.legacy_flags.is_some_and(|flags| flags & 0x02 == 0) {
    mat.reflectivity = 0.0;
}
```

`mat.reflectivity` is uploaded as `tint_reflect.w` (`byroredux/src/render/water.rs:271`, `crates/renderer/src/vulkan/water.rs:88-96`) and multiplies the entire Schlick-Fresnel RT reflection term in `crates/renderer/shaders/water.frag:40`. Zero means no reflection at all.

The premise is a generalisation from Oblivion (where the bit-0x01/bit-0x02 correlation does hold — see #3145). **It does not transfer to FO3/FNV, and the reference title ships the counterexample.**

## Evidence

All from vanilla `FalloutNV.esm` (78 `WATR` records, 245 650 747 B master), byte-level Python GRUP walk.

**1. FNV authors reflectivity as a float in `DNAM`, not as an `FNAM` bit — the disproof.**

`NVCleanWater` (`001009CA`) and `NVCleanWaterNoReflect` (`0017B612`) have **byte-identical 196-byte `DNAM` payloads except for one f32**:

```
DNAM offset 20:  NVCleanWater = 0.6 (9a99193f)  ->  NVCleanWaterNoReflect = 0.0 (00000000)
every other one of the 49 f32 slots: identical
FNAM:            NVCleanWater = 0x02            ->  NVCleanWaterNoReflect = 0x02   (UNCHANGED)
```

Offset 20 is exactly what `decode_dnam_pre_fo4:719` already reads into `p.reflectivity`. The author who needed a non-reflective variant of a water type zeroed that float and **left `FNAM` bit 0x02 set**. The bit is not the reflectivity channel on FO3/FNV, and the authored float already carries the "not reflective" signal the new gate was written to supply.

**2. `FNAM` bit 0x02 is clear on 44 of 78 records, 36 of which author non-zero reflectivity.** Ranked by how many `CELL` `XCWT` references point at them:

```
EDID                              FormID    FNAM  DNAM refl   cells
WaterTypeUtility                  000B03A7  0x00     0.100       18
PPurityWater01Murky               000AED89  0x00     0.020       18
BuzzardPointWater                 000881EE  0x00     0.180       14
WaterTypeDirty                    00075387  0x00     0.100        8
WaterTypeIrradiated               000B03A9  0x00     1.000        4
RadioactiveWater                  0015F8B2  0x00     0.180        2
CreekWater02AVGnv                 00172DF0  0x00     0.230        1
WaterTypeDirtyDark                0007538A  0x00     0.100        1
d4BadWater10r20h                  000AFB9A  0x00     0.370        1
... 27 more with 0 direct XCWT refs (WaterTypeCave 0.75, WaterTypeVault92 1.00,
  LamplightWater 1.00, DCMallWater 1.00, ReflectingPoolWaterType 0.29,
  TakomaParkMuddyWater 0.53, TGWater03 0.82, WaterTypeOasisClean 0.50, ...)
```

**67 of 451 `XCWT`-bearing cells** lose their authored reflection outright.

**3. The gate is purely destructive — it can never add correct behaviour.** Because FNV encodes "not reflective" as `DNAM[20] == 0.0`, every record the gate zeroes was already going to be correct without it; and the four cells using `NVCleanWaterNoReflect` — the one record whose EditorID states the intent — are *unaffected* by the gate (bit set) and correct only because of the float. The gate has a **100 % false-positive rate** on this corpus.

**4. The name-based read is inverted in both directions.** `ReflectingPoolWaterType` (`00074893`, reflectivity `0.29`) is gated OFF; `NVCleanWaterNoReflect` is gated ON.

## Impact

On the reference title, RT reflections vanish from **15.3 % of `XCWT`-referenced water cells**, including the Goodsprings-era `CreekWater02AVGnv`, the 18-cell `WaterTypeUtility` and `PPurityWater01Murky` families, and all irradiated/toxic water — with no log line, no telemetry counter and no test that can see it.

This is a **delta-local regression**: before `02c0d4b6` these surfaces rendered with their authored `0.02`–`1.00` reflectivity. Same code path, same bit, same era gate on FO3 (`Fallout3.esm` shares most of these very records), so the blast radius is both pre-FO4 Fallout titles. Scored HIGH per the severity table's "affects rendering correctness" row and the reference-title rule; not CRITICAL because nothing crashes or corrupts.

## Related

- #3145 (`ESM-2026-08-20-D5-06` / `LC-D5-01`) — the **Oblivion** half, where the bit correlation *does* hold. That finding's suggested fix explicitly claims the FO3/FNV meaning is the same, which this data contradicts; **the two must be reconciled before either is implemented.**
- #3107 (`WATR-ARB-04`) — the `DNAM` 52-byte prefix reader, which is what makes offset 20 one of the few fields that *does* survive today.
- The merged FO3/FNV `XNAM` issue filed alongside this one — the sibling `FNAM` bit-0x01 damage claim, disproved by the same census.

## Suggested Fix

Delete the `legacy_flags & 0x02` override from `resolve_water_material`. The authored `DNAM`/`DATA` offset-20 scalar is already the reflectivity channel on FO3/FNV and already encodes zero for non-reflective records.

If a gate is still wanted for Oblivion (where #3145's correlation stands), scope it to `GameKind::Oblivion` explicitly rather than to the presence of `legacy_flags`, which is set for five game kinds.

Pin the fix with a fixture built from the `NVCleanWater` / `NVCleanWaterNoReflect` pair — they differ in exactly one float and are a ready-made two-row regression test.

## DO NOT FIX BY ANALOGY

The WATR-side `FNAM` bit `0x10` decode is empirically correct and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16 (#3152). Different bits, different formats. This issue concerns **bit 0x02 only**.

---
*Filed from `docs/audits/AUDIT_FNV_2026-08-20.md` (Dim 1). Verified against HEAD `bb0b92f2` before filing — the gate is live at `env_translate.rs:723`.*

## Completeness Checks
- [ ] **SIBLING**: every other consumer of `rec.legacy_flags` in `resolve_water_material` checked for the same Oblivion-generalised premise
- [ ] **CANONICAL-BOUNDARY**: the fix stays at the ESM/NIFAL parser→`WaterMaterial` boundary — never pushed into `render/water.rs` or `water.frag`, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this against *shipped bytes* — the `NVCleanWater` / `NVCleanWaterNoReflect` pair, asserting the second is non-reflective **because of `DNAM[20]`** and the first is reflective **despite both having `FNAM == 0x02`**
