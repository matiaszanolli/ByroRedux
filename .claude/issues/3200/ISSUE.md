# #3200 — FNV-D4-01 / FO3-D3-01 (MERGED): FO3+FNV water hazard is authored via WATR.XNAM (73/78 FNV, 51/53 FO3) and parse_watr has no XNAM arm — every plane of Fallout water is harmless, and watal.md records the gap as correct

**Source**: `docs/audits/AUDIT_FNV + AUDIT_FO3 (merged)_2026-08-20.md`
**Filed**: 2026-08-20 · **HEAD**: `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3200

---

> **MERGED FINDING.** This issue covers `FNV-2026-08-20-D4-01` (`docs/audits/AUDIT_FNV_2026-08-20.md`, Dim 4) and `FO3-2026-08-20-D3-01` (`docs/audits/AUDIT_FO3_2026-08-20.md`, Dim 3). They are the same undecoded sub-record on the same shared `GameKind::Fallout3NV` parser, measured independently on both masters; filing them separately would fragment one fix.

- **Severity**: MEDIUM
- **Dimension**: FNV Dim 4 / FO3 Dim 3 — ESM Record Parser, this era's data through it
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:1282-1372` (`parse_watr`'s sub-record match — **no `XNAM` arm**), `:1327-1345` (the `DATA` arm that populates `legacy_damage`), consumed at `byroredux/src/cell_loader/water.rs:492-508` and `:825-841` (`WaterPlane::damage_per_second`); spec claim at `docs/engine/watal.md:479` and `:309`
- **Status**: NEW

## Description

**Two independent routes make FO3/FNV water permanently harmless.**

**(a) The decoded damage channel is structurally zero.** Both `WaterPlane` spawn sites gate the canonical damage channel on

```rust
record.water_flags.or(record.legacy_flags).is_some_and(|flags| flags & 0x01 != 0)
```

and then take `record.legacy_damage`, which is populated only from a **2-byte** `DATA` sub-record (`water.rs:1333`). Vanilla FO3/FNV satisfies neither half.

**(b) The era's real hazard channel is `XNAM`, and it is never parsed.** In FO3/FNV a water type's harm is not the `DATA` damage word — it is the `XNAM` link to a `SPEL` record ("water quality"). `parse_watr` has arms for `ANAM`, `FNAM`, `TNAM`, `NNAM`, `NAM0`–`NAM5`, `DATA`, `DNAM` and `GNAM`, but **none for `XNAM`** (`grep -n XNAM crates/plugin/src/esm/records/misc/water.rs` returns nothing at HEAD). The link is dropped at the parse boundary.

**(c) The spec records (a) as correct behaviour.** `docs/engine/watal.md` §4's provenance table asserts that for FO3/FNV the legacy-damage row is **AUTHORED**:

> `| legacy water damage | SENTINEL | AUTHORED when FNAM bit 0x01 is set | SENTINEL | DATA uint16 damage + FNAM |`

That claim is **false on data for both titles.**

## Evidence

### FNV — `FalloutNV.esm`, 78 `WATR` records

```
FNAM byte histogram over all 78 records:   { 0x02: 34, 0x00: 44 }
                                             <- bit 0x01 NEVER set, on any record
2-byte DATA damage values (70 records carry one): ALL ZERO
                                             <- including RadioactiveWater, WaterTypeIrradiated,
                                                ToxicDumpPool01/02, d5TerribleWater20r4h
The 8 records with a long 186-byte DATA never reach the `sub.data.len() == 2` arm at all,
so they cannot populate `legacy_damage` under any flag value.
```

`XNAM` is present on **73 of 78** FNV records — a 4-byte FormID pointing at the radiation/poison actor effect (e.g. `0x00045656`, `0x000B878F`, `0x00020D78`).

### FO3 — `Fallout3.esm`, 53 `WATR` records

`XNAM` census (`dump_record_subs … WATR`): **51 of 53** records carry a 4-byte `XNAM`, resolving to six distinct `SPEL` records —

```
0x00020517 SPEL WaterHeal2Good        0x00020D76 SPEL WaterHeal1Rads500
0x00020D77 SPEL WaterHeal5Terrible    0x00020D78 SPEL WaterHeal3Average
0x00045656 SPEL WaterHeal4Bad         0x000B878F SPEL WaterHeal1Purified
```

`0x00045656` alone is referenced by 27 records. All 53 `DATA` damage words read `[00, 00]`; the 11 records whose `DATA` is the 186-byte visual payload never reach the `len() == 2` arm and are left `None`. `MNAM` (material) is `[00]` on 53/53 and is likewise unparsed — nothing to recover there.

## Impact

`WaterPlane::damage_per_second` is structurally `0.0` for **every FO3 and FNV cell**. The consumer is live and runs every frame (`WaterContact::damage_per_second` → `water_damage_for_contact`, `byroredux/src/systems/character.rs:1048`, applied at `:480-481`), so both titles exercise the code path with a permanently-zero input — the same green-by-construction shape catalogued across this sweep.

Concretely: swimming through Vault 22's irradiated water, the toxic dump pools, `WaterTypeIrradiated`, `WaterTypeIrradiatedDirty`, `d5TerribleWater20r4h` and the Potomac is free, and all of it reads identically to purified water. **This is Fallout 3's single most title-defining environmental mechanic and it is currently a no-op.**

The engine already ships both halves this would feed — a water-contact damage runtime and a radiation affliction model (`crates/core/src/character/affliction.rs`). Only the FO3/FNV source is missing.

The doc row compounds it: a reader checking `watal.md:479` is told the current behaviour is correct FO3/FNV authoring.

Scored MEDIUM, matching #3145's scoring of the structurally analogous Oblivion defect.

## Related

- **#3145** (`ESM-2026-08-20-D5-06` / `LC-D5-01`) — the **Oblivion** damage gap. **Different defect, cross-referenced deliberately:** #3145 is an Oblivion `DATA`-offset decode error where the flag *is* authored and the damage value *is* non-zero, so data is being lost. Here nothing authored is lost from `DATA` (FO3/FNV genuinely write `0`); the defect is an **undecoded sub-record** plus a false spec row. The two games need opposite fixes and `watal.md`'s single row cannot describe both.
- #3196 — the sibling `FNAM` bit-**0x02** claim on the same records, disproved by the same census.
- #3157 (`LC-D6-03`) — the *other* wrong row in the same `watal.md` §4 table (appearance payload sub-record + size). Same table, different row; fix together.
- #3107 — the `DNAM` 52-byte prefix truncation on the same records.

## Suggested Fix

1. **Add an `XNAM` arm** to `parse_watr` capturing the effect `SPEL` FormID onto `WatrRecord` — name it for what it is (`effect_form`, **not** `damage`). It is a 4-byte FormID needing only a `remap_fid`, so the provenance is captured at the parse boundary now and a future effect runtime has a landing site instead of re-discovering the field later.
2. **Resolve it in `cell_loader/water.rs`** from the form's `EFIT` magnitude to `WaterPlane::damage_per_second` / a radiation channel. Keep `legacy_damage` as the Skyrim-era path; do **not** overload one field with two source semantics.
3. **Correct `docs/engine/watal.md:479`** to mark the FO3/FNV column SENTINEL-on-vanilla with a note that the era's hazard is authored through `XNAM` (actor effect), not the `DATA` damage word.

Wiring `XNAM` to a *fully simulated* effect needs a SPEL/magic-effect runtime that does not exist yet — that part is a missing foundation, deliberately out of scope here. Steps 1 and 3 are unblocked today.

---
*Filed from `docs/audits/AUDIT_FNV_2026-08-20.md` (Dim 4) + `docs/audits/AUDIT_FO3_2026-08-20.md` (Dim 3), merged. Verified against HEAD `bb0b92f2` — `parse_watr` has no `XNAM` arm.*

## Completeness Checks
- [ ] **SIBLING**: every other per-game `WATR` arm checked for an undecoded hazard/effect sub-record (Oblivion — see #3145)
- [ ] **CANONICAL-BOUNDARY**: `XNAM` → damage resolution happens at the ESM→`WaterPlane` boundary, never re-derived at render or contact time. See `/audit-nifal`.
- [ ] **TESTS**: a real-data assertion that at least one vanilla FO3 **and** one FNV record yields a non-zero hazard channel — not a fixture that encodes the current zero
- [ ] **DOC**: `watal.md:479` and `:309` corrected in the same change, alongside #3157's row
