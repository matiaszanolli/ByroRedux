# D6-06: D6-06: FNV/FO3 capture's derived-stat Status column marks 7 of its 8 implemented rows LOCKED

- **Labels**: low,character,documentation,doc-rot,game:fo3,game:fnv
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4462

---

**Source**: `crates/core/src/character/fallout.rs:45-85,155-215`; the corpus gate pins `derived_rows = Some(8)` per real master (crates/plugin/tests/parse_real_esm.rs:289,300).

**Description**

The capture's own convention (charal-fnv-fo3-ruleset.md header, lines 6-7) is "BUILT (already implemented), LOCKED (sourced), PENDING". All eight rows of the derived table are implemented and wired (Health, AP, Carry Weight, Melee Damage, Critical Chance, Unarmed Damage, Rad/Poison Resist), yet only Health is marked BUILT; the other seven still read LOCKED (`docs/engine/charal-fnv-fo3-ruleset.md:89-101`). Inverse-direction doc lag: the code is ahead of the capture.

**Evidence**

fallout.rs pushes 8 output keys; the corpus gate expects 8 on Fallout3.esm/FalloutNV.esm. Some rows' adjacent prose half-acknowledges code (AP's #2937 note), but the column the matrix-reader scans was never flipped.

**Impact**

A reader auditing coverage from the Status column undercounts FO3/FNV by seven rows; invites re-implementation of existing rows.

**Related**

#2936, #3092, #3093 (when the rows landed).

**Suggested Fix**

Flip the seven Status cells to BUILT (keeping their scope/sourcing parentheticals), or annotate the table with "all rows BUILT as of the #3092-series; column kept for sourcing provenance".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D6-06, /audit-character 2026-09-19, HEAD `479163836`).*