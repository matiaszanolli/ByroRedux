# NIF-D3-2026-09-29-02: parse-rate docs still report FO76 at 98.18% after #3461 closed; nif-parser.md's table lacks Skyrim LE and mislabels FO4

**Labels**: low,documentation,doc-rot,nif-parser,nif,game:fo76

**Source**: `docs/audits/AUDIT_NIF_2026-09-29.md`
**Severity**: LOW (doc-rot)
**Dimension**: Block Dispatch Coverage (reporting)
**Game Affected**: Fallout 76, Skyrim LE, Fallout 4
**Location**: `ROADMAP.md` (compat matrix FO76 row and the "Per-game NIF clean-parse rate" summary row); `docs/engine/game-compatibility.md` (FO76 matrix row and the FO76 section "NIF parser: 98.18% clean"); `docs/engine/nif-parser.md` § "Per-game NIF coverage" table.

## Description
- **ROADMAP and game-compatibility.md** still say FO76 is **98.18%** clean, with `GeneratedMeshes02` "0.00% clean" and a known-open tail. #3461 closed on 2026-09-02, and FO76 measures 100% clean over 102,968 NIFs (audit run 2026-09-29).
- **nif-parser.md, Skyrim LE**: the per-game table has no Skyrim LE row, although 22,466 LE NIFs have been gated since fb8173fe0.
- **nif-parser.md, FO4**: the row reads "100% vanilla (254 648 incl. third-party)"; the vanilla figure is 235,082 across 8 archives.
- **nif-parser.md, FO76**: the row still cites the superseded 58 469-file figure.

## Evidence
Per-game coverage table in the source report (100% on all eight titles; FO76 102,968 NIFs; Skyrim LE 22,466; FO4 235,082 vanilla), and the quoted doc lines.

## Impact
The doc CLAUDE.md names as authoritative for parse rates (the ROADMAP compat matrix) tells readers FO76 content is broken.

## Related
NIF-D3-2026-09-29-01, #4440, #3726 (closed, the earlier inverse staleness of the same table), #3461.

## Suggested Fix
- Refresh the FO76 rows in ROADMAP (matrix and summary) and in game-compatibility.md to the measured 100%.
- Add a Skyrim LE row to nif-parser.md.
- Relabel the FO4 row "235,082 vanilla (254,648 incl. third-party)" and update the FO76 file count.

Validated at HEAD 9fcfdc3fc: `grep 98.18` hits `ROADMAP.md` (matrix + summary) and `docs/engine/game-compatibility.md` (two sites); nif-parser.md table has no Skyrim LE row, FO4 row reads "254 648 incl. third-party", FO76 row cites 58 469.

## Completeness Checks
- [ ] **SIBLING**: README / feature-matrix parse-rate mentions checked for the same stale figure
