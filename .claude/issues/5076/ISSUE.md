# #5076 — ESM-2026-09-29-D2-02: Oblivion EFID values are 4-char effect codes, not FormIDs, but three decode paths push them through remap_fid — a false warn! per effect on every Oblivion DLC/mod

**Labels**: low, bug, esm-plugin, game:oblivion

**Source**: `docs/audits/AUDIT_ESM_2026-09-29.md` — finding `ESM-2026-09-29-D2-02`

**Severity**: LOW

**Dimension**: Sub-Record Byte Accounting / FormID & Load Order

**Record / Sub-record**: `SPEL`/`ENCH`/`ALCH`/`INGR` / `EFID` (Oblivion)

**Location**: `crates/plugin/src/esm/records/misc/magic.rs:587-592` (`MagicEffectAccumulator::feed`); `crates/plugin/src/esm/records/items.rs:942-945` (`parse_alch`), `:1047-1050` (`parse_ingr`)

**Status in report**: NEW. The code has been there since #4071/#3714, before the 09-21 baseline, and no prior report or issue covers it. #969 added `magic_effects_by_code` for the lookup side, but not the decode side.

## Description

On Oblivion, `EFID` is `wbInteger('Magic Effect Name', itU32, wbChar4)` (`wbDefinitionsTES4.pas:1499`), e.g. `b"FIDG"`. `EsmIndex::magic_effects_by_code` (index.rs:226-237) exists precisely because this is not a FormID. All three paths still call `remap_fid` on it. The code's high byte, an ASCII letter from 0x41 to 0x5A, is read as a mod index. That is always out of range, so the remap warns and passes the value through unchanged.

## Evidence

EFID counts in the vanilla Oblivion DLC esps, each with one master (`Oblivion.esm`):

| esp | EFIDs |
|---|---|
| `Knights.esp` | 197 |
| `DLCMehrunesRazor.esp` | 123 |
| `DLCVileLair.esp` | 121 |
| `DLCSpellTomes.esp` | 110 |
| `DLCThievesDen.esp` | 62 |
| `DLCBattlehornCastle.esp` | 53 |
| `DLCFrostcrag.esp` | 35 |
| `DLCOrrery.esp` | 16 |
| **Total** | **717** |

Each of these 717 is a warn.

## Impact

717 false warnings on a vanilla Oblivion + DLC load. The value is preserved only because no plugin carries 65 or more masters; with that many, a code would be silently rewritten into a real slot's FormID space. This is not a correctness defect today.

## Related

D2-01 (the same "non-FormID u32 through the remap" family), #969, #4071.

## Suggested Fix

Skip the remap for `EFID` when `game == GameKind::Oblivion`. The accumulator already carries `game`; `parse_alch` and `parse_ingr` would need it threaded through, or they could route Oblivion through the accumulator. Add a test that feeds an Oblivion `b"FIDG"` EFID through a remap that has masters.

Validated at HEAD 9fcfdc3fc: `MagicEffectAccumulator::feed` (`records/misc/magic.rs`) and `parse_alch` / `parse_ingr` (`records/items.rs`) still call `remap_fid` on EFID with no Oblivion gate (`parse_scrl` also remaps EFID, but SCRL is not an Oblivion record).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `parse_scrl` EFID arm; the lookup side via `magic_effects_by_code`)
- [ ] **TESTS**: A regression test pins this specific fix (an Oblivion `b"FIDG"` EFID through a remap with masters)
