# ESM-D8-03: `sf-esm` baseline leaf rows for `MUST` and `VOLI` are stale

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6807
Labels: low, esm-plugin, bug, test-gap
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (doc/baseline rot)
- **Dimension**: Real-Data Validation
- **Record / Sub-record**: `.claude/audit-baselines/sf-esm/Starfield.tsv` (leaf `MUST 235`, `VOLI 42`), `ShatteredSpace.tsv` (leaf `MUST 19`, `VOLI 2`)
- **Location**: `.claude/audit-baselines/sf-esm/Starfield.tsv` lines for `leaf MUST` / `leaf VOLI`; the guard is `crates/plugin/src/esm/records/tests.rs:3396` (reads `top` rows only)
- **Status**: NEW
- **Description**: Commits `03f9d2158` and `3bfcbd58b` regenerated the `top` rows (now `handled=1`) but not the matching `leaf` rows, which still read `handled=0`. A fresh `sf_smoke --tsv --recurse` therefore shows four spurious diffs. The skill says "a drop is a regression", so a stale 0→1 reads as noise and hides a real drop. The test's doc comment says leaf rows "repeat the top-level flags and are not checked", which is no longer true once only half of them are refreshed.
- **Evidence**: `diff` of the checked-in file against a fresh run: `leaf MUST 235 0 0 0 0` vs `… 0 0 1 0`, and the same for `VOLI`; the other three baselines are identical.
- **Impact**: Audit noise only. `handled=1` for Starfield `MUST`/`VOLI` also overstates decode (the arms drop them, see the matrix caveat).
- **Suggested fix**: Regenerate with `sf_smoke … --tsv --recurse`, and extend the test to check leaf rows against the dispatch list too. Put `--recurse` in the skill's First step.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
