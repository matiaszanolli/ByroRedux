# #3726: NIF-2026-08-30-D3-03: nif-parser.md's per-game coverage table understates the swept corpus 3.4x and reports FO76 at 100% clean where it measures 98.18%

**Labels**: documentation, nif-parser, low, nif, doc-rot
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIF_2026-08-30.md` · **Severity**: LOW · **Dimension**: Block Dispatch Coverage
**Game affected**: all seven (documentation)

## Location
- `docs/engine/nif-parser.md` — § "Per-game NIF coverage" and the "Cumulative NIFs swept" row in the stats table

## Description
The table is stamped "summary refreshed 2026-07-11 (#1900 / NIF-D3-02)" and predates the #3041/#3466/#3369 corpus widening, so every row under-counts. Two rows are wrong in **direction**, not merely stale.

## Evidence

| row | documented | measured 2026-08-30 |
|---|---|---|
| Oblivion | 99.93% (8,026 / 8,032), "remaining 6 v3.3.0.13 marker files" | **100% (9,612 / 9,612), 0 truncations** |
| Fallout 3 | 100% (10,989) | 100% (17,172) |
| Fallout NV | 100% (14,881) | 100% (20,746) |
| Skyrim SE | 100% (18,862) | 100% (33,468) |
| Fallout 4 | 100% (34,995 + 124,871) | 100% vanilla (254,648 incl. third-party) |
| **Fallout 76** | **100% (58,469)** | **98.18% clean (165,164 / 168,220)** |
| Starfield | 99.64% aggregate | 99.98% clean (120,836) |
| Cumulative swept | 184,886 | **624,702** |

The FO76 row is the load-bearing error: `GeneratedMeshes02.ba2` parses 0.00% clean and `GeneratedMeshes01` 95.03%, which the `/audit-nif` skill records as known-open and deliberately un-baselined pending #3461. The document a reader consults for per-game compatibility still advertises 100%. The Oblivion row errs the other way, claiming six residual truncations that no longer exist.

## Impact
Documentation only, but this is the code-verified reference audits are instructed to measure findings against; a wrong clean-rate causes an auditor to chase a fixed bug (Oblivion's six files) or miss a real gap (FO76's 3,056 recovered blocks).

## Related
#3461 (the FO76 cause), #3513 (the identical stale-corpus error already filed against the ROADMAP's FO3 cell — note that #3513's own premise has since been verified fixed and it is a close candidate), #3041/#3466/#3369.

## Suggested Fix
Refresh the table from a full sweep, split the FO76 row into per-archive rates so the `GeneratedMeshes` tail is visible rather than averaged away, and note beside the Oblivion row that the #687/#688/#698 marker files now parse whole.

## Completeness Checks
- [ ] **SIBLING**: `docs/engine/game-compatibility.md` and the ROADMAP compat matrix carry the same figures — refresh in the same pass
- [ ] **TESTS**: n/a (documentation), but the refreshed numbers should be taken from the existing corpus gates, not retyped from this report
