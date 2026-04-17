# OBL-D3-H2: CREA record type absent from MODL match arm — Oblivion creatures don't spawn

**Issue**: #396 — https://github.com/matiaszanolli/ByroRedux/issues/396
**Labels**: bug, high, legacy-compat

---

## Finding

`crates/plugin/src/esm/cell.rs:230-233` enumerates every record type with a MODL sub-record that the walker builds into the statics lookup table:

```rust
b"STAT" | b"MSTT" | b"FURN" | b"DOOR" | b"ACTI" | b"CONT" | b"LIGH" | b"MISC"
| b"FLOR" | b"TREE" | b"AMMO" | b"WEAP" | b"ARMO" | b"BOOK" | b"KEYM" | b"ALCH"
| b"INGR" | b"NOTE" | b"TACT" | b"IDLM" | b"BNDS" | b"ADDN" | b"TERM" | b"NPC_"
| b"SCOL" | b"MOVS" | b"PKIN" | b"TXST" => {
```

**Missing: `CREA`** (creature, Oblivion/FO3/FNV) and **`ACRE`** (placed creature ref, Oblivion only — FO3+ folded into ACHR).

Oblivion groups creatures separately from NPCs (440 KB CREA group in `Oblivion.esm`, ~250 records including goblin/rat/zombie/daedra). FO3+ folded creatures into NPC_; that's why the current code never needed CREA. ACRE placement-ref matcher at `cell.rs:468` similarly only handles `REFR`/`ACHR`.

## Impact

- Ayleid ruins, Oblivion gates, dungeon caves, Arena barracks: every creature placement fails the base-ref lookup silently and skips rendering.
- Pure-static interiors (Anvil Heinrich Oaken Halls, shops, houses) unaffected.

## Fix

Two 1-line changes:

```rust
// cell.rs:230-233 — add CREA to the MODL match arm
b"STAT" | ... | b"TXST" | b"CREA" => { ... }

// cell.rs:468 — add ACRE alongside REFR/ACHR
b"REFR" | b"ACHR" | b"ACRE" => { ... }
```

CREA uses the standard MODL subrecord (identical to STAT), so no field-layout work. ACRE placement structure matches ACHR byte-for-byte on Oblivion.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: `cell.rs:1035, 1036, 1131, 1154` test helpers hard-code STAT; add CREA/ACRE counterparts.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Parse a minimal synthetic CELL with one ACRE referencing a CREA base; assert the placed reference survives the walker.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 3 H2 (also Dim 6 #3).
