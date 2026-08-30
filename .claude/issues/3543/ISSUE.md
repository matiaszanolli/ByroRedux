# #3543: SK-D4-01: the Deleted (0x20) tombstone is honoured only for placements — 9 DLC-deleted base records merge live on vanilla Skyrim

**Source**: `docs/audits/AUDIT_SKYRIM_2026-08-30.md` — Dimension 4 (Multi-Master Load Order)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/cell/walkers.rs` (`RECORD_FLAG_DELETED`), `crates/plugin/src/esm/records/` (no test), `EsmIndex::merge_from`, doc at `crates/plugin/src/esm/cell/mod.rs`

## Description

`RECORD_FLAG_DELETED` (0x20) is tested at exactly one site in the whole crate —
`crates/plugin/src/esm/cell/walkers.rs`, inside the REFR/ACHR/ACRE placement walk. No
parser under `crates/plugin/src/esm/records/` tests bit `0x20`, so a **base** record that
a later plugin marks Deleted is merged by `EsmIndex::merge_from` under plain
last-write-wins and **replaces the master's live record with the DLC's tombstoned copy**.

## Evidence

Verified against current code (2026-08-30): `grep -rn RECORD_FLAG_DELETED crates/plugin/src`
returns the constant declaration plus **one** test at the REFR walk, and three doc mentions.
Zero record parsers consult it.

Measured on the shipped Skyrim SE DLC set (excluding NAVM, which has no consumer, and
REFR/ACHR, which the placement walker already skips):

| Plugin | Type | FormID (raw) | `data_size` | header flags |
|---|---|---|---|---|
| `Update.esm` | STAT | `0006CD7C` | 153 | `0x04010820` |
| `Dawnguard.esm` | STAT | `000BD6A5` | 198 | `0x00000020` |
| `Dawnguard.esm` | **NPC_** | `0007932F` | 220 | `0x00040020` |
| `Dawnguard.esm` | IDLE | `000FDC30` | 210 | `0x00000020` |
| `Dawnguard.esm` | IDLE | `000F6CBB` | 192 | `0x00000020` |
| `Dawnguard.esm` | SMQN | `000F2199` | 147 | `0x00000020` |
| `Dragonborn.esm` | **SPEL** | `0010E38C` | 307 | `0x00000020` |
| `Dragonborn.esm` | INFO | `000CEFBE` | 20 | `0x00000020` |
| `Dragonborn.esm` | EXPL | `000F3A8C` | 163 | `0x00000020` |

Every one carries a **non-empty payload** (20–307 bytes), so these are full override
records, not zeroed stubs the merge would harmlessly absorb. Every raw FormID has top byte
`0x00` — master index 0 = `Skyrim.esm` — so all nine are DLC overrides of base-game records
that the DLC then deletes. Correct behaviour: drop the base record from the merged index.
Actual behaviour: keep it, carrying the DLC's stale content.

The associated doc over-claims: `crates/plugin/src/esm/cell/mod.rs` states deleted records
"never appear in `over` at all" — true of REFRs, but read in a file named `cell/mod.rs` it
reads as though the tombstone story is complete. It is complete for placements only.

## Impact

Nine vanilla records merge live that should be removed, including `Dawnguard.esm`'s deleted
`NPC_ 0007932F` (still spawnable, still resolvable by the equip chain) and
`Dragonborn.esm`'s deleted `SPEL 0010E38C`. Scope on vanilla is small — which is why this is
MEDIUM, not HIGH — but the mechanism is general: a mod load order with real conflict
resolution hits it at far higher volume than vanilla does.

## Suggested Fix

One flag test in the record walk mirroring the placement-walk site, plus a removal signal
through `merge_from` analogous to `CellData::deleted_refs` (#2370). Correct the
`cell/mod.rs` doc to say the tombstone is honoured for placements only.

## Related

#2370 (`CellData::deleted_refs`), #1660 (REFR tombstone skip).

## Completeness Checks
- [ ] **SIBLING**: the same 0x20 test applied across every record parser, not just the one type that motivated the fix
- [ ] **LOCK_ORDER**: if a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: a regression test pins a DLC-deleted base record being dropped from the merged index (use one of the nine measured FormIDs)
