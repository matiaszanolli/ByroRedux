# #3145 — ESM-2026-08-20-D5-06 / LC-D5-01: Oblivion `WATR` damage and `FNAM` flags are never decoded — every plane of Oblivion lava is harmless, and watal.md records the omission as a game distinction the data disproves

**Finding**: ESM-2026-08-20-D5-06 / LC-D5-01
**Labels**: bug, import-pipeline, medium, legacy-compat
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3145

---

- **Severity**: MEDIUM
- **Dimension**: ESM Dim 5 — CELL / WRLD walkers (WATR schema) · LEGACY_COMPAT Dim 5 — EXAL / WATAL per-game water authoring → canonical
- **Record / Sub-record**: `WATR` / `DATA`, `WATR` / `FNAM`
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:1293-1310` (the `b"FNAM"` arm's `matches!` allowlist, which names five `GameKind`s and omits `Oblivion`), `:1330-1332` (the `b"DATA"` arm's `!matches!(game, GameKind::Oblivion)` guard on the damage capture), `:468-546` (`decode_data_oblivion`, which reads nothing past offset 96); test pin at `:1469`; consumer `byroredux/src/cell_loader/water.rs:492-502`; doc row `docs/engine/watal.md:479`
- **Status**: NEW

> **Merged finding.** This is `ESM-2026-08-20-D5-06` and `LC-D5-01` from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` — the same defect seen from the parser side and the layer side. Both reports are represented here; neither is filed separately.

## Description

The canonical water-damage path is **fully built and live**: `WaterPlane::damage_per_second` → `WaterContact::damage_per_second` (`crates/physics/src/water.rs:372`) → `water_damage_for_contact` (`byroredux/src/systems/character.rs:1048`), applied every frame at `character.rs:480-481`. It shipped in the 2026-08-10 WATAL Phase 2/3 checkpoint.

It is populated by a filter that requires **both** `record.water_flags.or(record.legacy_flags)` to have bit `0x01` set **and** `record.legacy_damage` to be `Some`:

```rust
// byroredux/src/cell_loader/water.rs:492-502
.filter(|record| {
    record.water_flags.or(record.legacy_flags)
        .is_some_and(|flags| flags & 0x01 != 0)
})
.and_then(|record| record.legacy_damage)
```

**Oblivion can satisfy neither half.**

1. The `b"FNAM"` arm is an explicit five-game allowlist — `Fallout3NV | Skyrim | Fallout4 | Fallout76 | Starfield`. `GameKind::Oblivion` is absent, so `water_flags` and `legacy_flags` are both `None` for every TES4 record. A unit test at `water.rs:1469` *pins* that (`assert_eq!(oblivion.water_flags, None);`).
2. The damage value is equally unreachable. The `b"DATA"` arm captures `legacy_damage` only from a 2-byte sub-record and only when `!matches!(game, GameKind::Oblivion)`. Oblivion packs its damage `u16` in the **tail of its 102-byte `DATA`**, and `decode_data_oblivion` reads floats from offsets 0–96 and never touches offset 100.

## Evidence

Direct scan of vanilla `Oblivion.esm` (TES4's 20-byte record header; all 23 `WATR` records; damage read as the trailing `u16` of `DATA`):

```
FNAM byte distribution: {0x02: 16, 0x01: 5, 0x00: 2}

EDID                          FNAM   DATA len   trailing u16
OblivionCitadelLavaPlane      0x01      102          5000
OblivionLavaTest01            0x01      102            50
CamoranLava02                 0x01       42            50
CamoranLava                   0x01        2         65535
OblivionOil01                 0x01       62             0
DefaultWater / SewerWater /
SwampWater / … (16 records)   0x02   102/86             0
XPBlood, Blood                0x00   102/42             0
```

The correlation is exact and self-proving: `FNAM` bit `0x01` is set on **precisely** the five lava/oil records and on nothing else, and four of those five carry a non-zero damage value while all eighteen non-flagged records carry zero. That is the identical `FNAM` bit-`0x01` semantic the code already implements for FO3/FNV — Oblivion is simply excluded from the arm.

Note `CamoranLava` specifically: its entire `DATA` is the 2-byte damage payload (`65535`), and the `!matches!(game, GameKind::Oblivion)` guard routes even *that* into `decode_data_oblivion`, which reads nothing from a 2-byte buffer.

At HEAD, `parse_watr(…, GameKind::Oblivion)` returns `legacy_flags: None`, `water_flags: None`, `legacy_damage: None` for all 23 records.

**The spec records the omission as intentional.** `docs/engine/watal.md:479`:

```
| legacy water damage | SENTINEL | AUTHORED when `FNAM` bit 0x01 is set | SENTINEL | `DATA` uint16 damage + `FNAM` |
```

The first column is Oblivion. The layer asserts TES4 does not author water damage. The corpus above disproves that on five records.

## Impact

**Every damaging water surface in Oblivion is canonically harmless.** The Oblivion realm's Citadel lava (5000 dmg/s authored), Camoran's Paradise lava, and the Oblivion oil pools all resolve to `damage_per_second = 0.0` and can be swum through without effect.

This is a parser-side gap with a **live, correct reader on the other end** — the WATAL physics half is wired for a signal the ESM layer never emits on TES4. It is silent: there is no "record authored damage but we dropped it" warning anywhere, and it is *doubly* silent because `watal.md` documents the gap as a game distinction, so a reader checking the spec is told the behaviour is correct.

Scored MEDIUM rather than HIGH because Oblivion is not the reference title and the corpus is five records; the blast radius *within* Oblivion is the entire Oblivion-realm hazard model.

## Related

- `/audit-physics` Dim 6 owns the consumer; `cell_loader/water.rs:492-502` is correct given a populated `legacy_damage`.
- #3110 — the 86-byte Oblivion `DATA` variant, whose evidence independently locates the trailing `u16` damage at offset 84 on that shorter shape.
- #3105 — the sibling one-field simulator misalignment in the same Oblivion decoder.
- `docs/engine/watal.md` §4 row "legacy water damage".

## Suggested Fix

1. Add `GameKind::Oblivion` to the `b"FNAM"` arm's `matches!` gate and set `legacy_flags` for it. TES4 bit `0x01` = causes damage and bit `0x02` = reflective — both already match the FO3/FNV meaning the arm implements.
2. Read the trailing `u16` of the Oblivion `DATA` payload into `legacy_damage`. The offset is length-dependent: **100** on the 102-byte shape, **84** on the 86-byte shape (see #3110's census) — derive it from `data.len() - 2` rather than hardcoding one.
3. Let a 2-byte Oblivion `DATA` fall through to the damage-only path as the other games do, instead of routing it into `decode_data_oblivion`.
4. Correct the `watal.md` §4 "legacy water damage" row from `SENTINEL` to `AUTHORED` in the Oblivion column.
5. Replace the `assert_eq!(oblivion.water_flags, None)` pin at `:1469` — it encodes the defect as expected behaviour. Pin instead with a fixture built from `OblivionCitadelLavaPlane` (`FNAM = 0x01`, damage `5000`).

---
*Filed from `docs/audits/AUDIT_ESM_2026-08-20.md` (D5-06) and `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` (LC-D5-01), merged. Verified against HEAD `bb0b92f2` before filing.*

## Completeness Checks
- [ ] **SIBLING**: the five `GameKind` allowlists in `parse_watr` re-checked for other arms that silently exclude Oblivion
- [ ] **CANONICAL-BOUNDARY**: the damage decode stays at the ESM parser → `WaterRecord` boundary; the consumer at `cell_loader/water.rs:492` must not gain a per-game branch. See `/audit-nifal`.
- [ ] **DOC**: `docs/engine/watal.md:479` updated in the same change — the SENTINEL row is what makes this defect invisible to a spec reader
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16 — different bits, different formats, fixing one by analogy with the other breaks working code
- [ ] **TESTS**: a regression test pins this against *shipped bytes* (`OblivionCitadelLavaPlane`), not against the decoder's own output; the existing `water_flags == None` pin is deleted
