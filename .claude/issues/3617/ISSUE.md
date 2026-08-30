# #3617: OBL-D3-05: LVSP has a RecordType constant but no parser — 306 leveled-spell lists never dispatch

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 3 (ESM Record Coverage)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/record.rs` (`RecordType::LVSP`), `crates/plugin/src/esm/records/mod.rs` (the `parse_leveled_list` dispatch arm)

## Description

`RecordType::LVSP` (leveled spell list) is declared but has no parser: the dispatch arm
routes only `CONT | LVLI | LVLN | LVLC` to `parse_leveled_list`. LVSP shares the identical
LVLD/LVLF/LVLO layout.

## Evidence

Verified 2026-08-30:

```
crates/plugin/src/record.rs:206:    pub const LVSP: Self = Self(*b"LVSP");
crates/plugin/src/esm/records/mod.rs:428:  b"CONT" | b"LVLI" | b"LVLN" | b"LVLC" => {
```

One declaration, zero dispatch. Measured: **306 LVSP records in `Oblivion.esm`**.

## Impact

NPC_/CREA `SPLO` entries pointing at a leveled spell resolve to nothing — the actor silently
gets no spell where the record intended a level-scaled one. 306 records on Oblivion; the
record type is shared with later titles.

## Suggested Fix

One token added to the existing match arm — `b"LVSP"` alongside `b"LVLI" | b"LVLN" | b"LVLC"`
— since the payload layout is identical.

## Related

`parse_leveled_list` (`crates/plugin/src/esm/records/`), OBL-D3-06 (the other unhandled
Oblivion record types).

## Completeness Checks
- [ ] **SIBLING**: confirm the LVSP entries land in whatever index `SPLO` resolution reads, not only in the generic leveled-list map
- [ ] **TESTS**: a regression test pins an `Oblivion.esm` LVSP decoding its LVLO entries and resolving from a `SPLO` reference
