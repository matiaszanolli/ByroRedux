**Source:** FO4 compatibility audit — Dimension 3 (BA2 reader), `docs/audits/AUDIT_FO4_2026-07-13.md`
**Severity:** LOW (doc rot) · **Status when filed:** NEW, CONFIRMED against current code

## Description
Three comments in `crates/bsa/src/ba2.rs` cite sibling code by hard-coded line number; all three drifted after the #1825 `chunk_hdr_len != 24` `warn!` block was inserted, shifting the referenced sites downward.

## Evidence
- `ba2.rs:541`: "matches the chunk struct decoded at **line ~496**" — chunk decode is now at line 604.
- `ba2.rs:642`: "`num_mips == 0` warn at **lines 512-519**" — the warn is now at lines 591-598.
- `ba2.rs:643`: "`chunk_hdr_len != 24` debug_assert at **lines 490-495**" — it is now at lines 546-551.

`grep -n` confirms the actual sites do not match the cited ranges. Cross-file refs (e.g. `ba2.rs:217` → `archive/open.rs:40-48`) are correct — only the same-file numeric refs drift.

## Impact
None at runtime; a maintainer following the comment lands on unrelated code. Sibling of the recurring TD7-* stale-path class (cf. #1918/#1919 renderer comment drift).

## Suggested Fix
Replace the hard line numbers with symbolic references ("the `num_mips == 0` warn below", "the `chunk_hdr_len` `debug_assert` above") that don't rot.

## Completeness Checks
- [ ] **SIBLING**: sweep `ba2.rs` for any other hard-coded same-file line refs while in there
