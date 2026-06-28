# FNV-D4-NEW-02: parse_scol docstring claims SCOL is FO4-and-later

**Severity**: LOW (doc-rot; behavior correct) · **Source**: `docs/audits/AUDIT_FNV_2026-06-27.md` (FNV-D4-NEW-02)
**Location**: `crates/plugin/src/esm/records/scol.rs` (`parse_scol` doc comment)
**Status**: NEW

## Description
The `parse_scol` doc comment (`scol.rs:132`) states *"Wire format is FO4-and-later; earlier games don't emit SCOL."* This is factually wrong and directly contradicts the (correct) dispatch gate in `records/mod.rs` (#1538): `is_scol_era = is_fo4_plus || Fallout3NV`. FalloutNV.esm carries **98 SCOL records** with **423 ONAM/DATA child pairs**, every DATA block an exact multiple of the 28-byte `WIRE_SIZE`. The parser handles FNV SCOL perfectly; only the comment is stale.

## Evidence
Byte-scan of FalloutNV.esm: `SCOL records: 98, ONAM count: 423, SCOL DATA size mod 28: {0: 423}`. Sample `SCOLGoodspringsFenceB01` (`0x0017B667`) parses with 4 ONAM/DATA pairs. The module doc (`scol.rs:8-9`) already says "FNV and FO4 DATA layouts are identical" — the function doc contradicts the module doc.

## Impact
Documentation only. Risk is indirect: this is exactly the kind of stale claim that justified the #1538 gate regression (re-narrowing SCOL dispatch to FO4-only, which silently drops the 1084 FNV SCOL placements) in the first place.

## Related
#1538 (the SCOL-not-FO4-only fix), `scol.rs:8-9` (correct module doc).

## Suggested Fix
Reword to "FO3/FNV and FO4+ (byte-identical 28-byte DATA layout); Oblivion/Skyrim don't emit SCOL" to match the module doc + the dispatch gate.

## Completeness Checks
- [ ] **SIBLING**: Grep the other per-record `parse_*` doc comments for the same FO4-only over-narrowing on records that are actually FO3/FNV-era (e.g. anything gated on `is_fo4_plus || Fallout3NV`)
