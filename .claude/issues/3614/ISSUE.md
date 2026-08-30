# #3614: OBL-D3-03: three INFO link/condition sub-records are dropped — TCLF (3,792 records), NAME (1,044) and CTDT (45 parse as unconditional)

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 3 (ESM Record Coverage)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/records/misc/dialogue.rs` — `parse_info`

## Description

Three INFO sub-records Oblivion authors have no arm in `parse_info` and are dropped:
`TCLF` (Link From), `NAME` (Add Topics) and `CTDT` (the legacy fixed-layout condition
record). A fourth, `SCHD`, is likewise unread.

## Evidence

Verified 2026-08-30 — `parse_info`'s match arms are `QSTI`, `DATA`, `NAM1`, `NAM2`, `TRDT`,
`TCLT`, `PNAM`, `ANAM`, and `CTDA | CIS1 | CIS2` (routed to `push_ctda`). No `TCLF`, no
`NAME`, no `CTDT`, no `SCHD`.

Measured over all 19,278 Oblivion INFO records (`occurrences/records`):

- **`TCLF` — 4,141 occurrences across 3,792 records (19.7%)**: Oblivion's "Link From" topic
  edge. `TCLT` ("Choose Topic") **is** handled, so the tree is built from one of the two
  available edge kinds.
- **`NAME` — 1,342 occurrences across 1,044 records (5.4%)**: Oblivion's "Add Topics" — the
  DIAL FormIDs a response unlocks. Dropped entirely.
- **`CTDT` — 72 occurrences across 45 records**: the legacy fixed-layout condition
  sub-record Oblivion still uses on a handful of INFOs. Only `CTDA`/`CIS1`/`CIS2` reach
  `push_ctda`.
- (`SCHD`, 47 records — the legacy script header alongside `SCHR` — is also unread, but the
  result-script path is not consumed for Oblivion INFOs anyway.)

## Impact

- Half the topic-graph edges (`TCLF`) are invisible, so the conversation graph is
  structurally incomplete for ~20% of records.
- 1,044 records cannot unlock the topics they were authored to unlock.
- **45 INFOs parse as unconditional** because their only conditions are in `CTDT` — they
  will fire when they should not. That is the one behavioural (not just structural)
  consequence in this set.

## Suggested Fix

Add `TCLF` and `NAME` arms alongside the existing `TCLT` arm, and route `CTDT` into
`push_ctda` through a fixed-layout decoder for the legacy condition shape.

## Related

OBL-D3-02 (PNAM/ANAM absent on the same records), OBL-D3-04 (multi-response overwrite in the
same parser).

## Completeness Checks
- [ ] **SIBLING**: `CTDT` may appear on records other than INFO — check every `push_ctda` caller for the same legacy shape
- [ ] **TESTS**: a regression test pins one of the 45 CTDT-only INFOs parsing as *conditional*, plus a TCLF edge and a NAME unlock
