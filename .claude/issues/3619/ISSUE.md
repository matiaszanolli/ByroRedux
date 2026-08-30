# #3619: OBL-D3-06: SBSP (33) and ROAD (2) have no code references anywhere

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 3 (ESM Record Coverage)
**Severity**: LOW
**Location**: `crates/plugin/src/record.rs`, `crates/plugin/src/esm/records/mod.rs`

## Description

Two Oblivion record types have no code references anywhere: `SBSP` (subspace — an
Oblivion-only collision volume) and `ROAD` (worldspace road path).

## Evidence

Verified 2026-08-30:

```
$ grep -rn '"SBSP"\|"ROAD"' crates/plugin/src byroredux/src --include='*.rs'
(no hits)
```

Neither is in `RecordType`, neither is dispatched. Measured in `Oblivion.esm`: **SBSP = 33,
ROAD = 2**.

The full walk of `Oblivion.esm` found 63 distinct record types with 0 walker errors, of which
exactly four are unhandled: PGRD (8,228 — filed separately), LVSP (306 — filed separately),
SBSP (33) and ROAD (2). Neither of the two here is on the REFR placement path, so neither
blocks exterior or interior cell rendering.

## Impact

Low. SBSP is an Oblivion-only collision-volume concept with no current consumer; ROAD matters
only if worldspace road rendering is ever wanted (2 records in the whole game).

## Suggested Fix

Record both as deliberate non-goals with a one-line note, or add flat parsers if/when a
consumer appears. The value here is that the gap is now measured rather than unknown.

## Related

OBL-D3-01 (PGRD), OBL-D3-05 (LVSP) — the other two members of the unhandled-four.

## Completeness Checks
- [ ] **TESTS**: if parsers are added, a regression test pins the 33 SBSP / 2 ROAD counts against `Oblivion.esm`
