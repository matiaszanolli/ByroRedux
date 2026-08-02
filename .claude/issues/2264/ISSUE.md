# TD6-001: ROADMAP.md's 'Completed Milestones' one-liner still calls PACK/QUST/DIAL/MESG/PERK/SPEL/MGEF 'stubs' — all seven fully implemented for months

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2264

**Dimension**: 6 (Stub & Placeholder Implementations) / overlaps Dimension 3
**Location**: `ROADMAP.md:757`
**Status**: NEW

**Description**: The one-liner reads "PACK / QUST / DIAL / MESG / PERK / SPEL / MGEF stubs (#446/#447)." `git blame` pins this line to commit `f3cde4bb8` (2026-04-22) — describing the *original* M24 Phase 1 scope. It was never updated even though the same file's own later, more detailed rows document all seven as implemented: M24.2 states "Phase 2 closed (2026-06-03, `45509f4f`): MGEF full effect struct + flags, SPEL/ENCH EFID/EFIT chain, AVIF PERK list lookup, QUST per-stage CTDA, INFO records all semantically decoded"; M42 and the Known-Issues row both explicitly say "#446 closed" for PACK. Reading the code confirms it: `parse_pack` (1,881 LOC), `parse_qust` (1,283 LOC), `parse_dial`/`parse_mesg` (565 LOC), `parse_perk`/`parse_spel`/`parse_mgef` (1,351 LOC) are all real, substantial, tested parsers — not stubs by any definition.

**Evidence**: `git blame -L 752,758 ROADMAP.md` → `f3cde4bb8 2026-04-22`; cross-reference `ROADMAP.md:487,497,821` (all describing the same seven record types as implemented/closed); `wc -l crates/plugin/src/esm/records/misc/{pack,quest,dialogue,magic}.rs` → 1881/1283/565/1351.

**Impact**: A reader skimming the "Completed Milestones" summary — the document's own intended fast-reference section — would conclude these seven record types are unimplemented, when in fact they're some of the most heavily-developed parsers in the ESM tree.

**Suggested Fix**: Replace line 757 with something like: "PACK / QUST / DIAL / MESG / PERK / SPEL / MGEF now fully parsed (#446/#447 closed; see M24.2/M42 rows for decode detail)," or simply drop the record-type list from this historical one-liner and point to the M24.2 row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
