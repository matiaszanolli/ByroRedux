# #5226 — FO3-D3-03: An unmatched `--wrld` silently falls back to the grid heuristic; on FO3 a typo of `MegatonWorld` loads Wasteland's version of the same cell

https://github.com/matiaszanolli/ByroRedux/issues/5226

Source: `docs/audits/AUDIT_FO3_2026-10-03.md` (HEAD `be3cd9468`)

- **Severity**: LOW
- **Dimension**: Cell Loading (exterior selection)
- **Location**: `byroredux/src/cell_loader/exterior.rs:1427-1436` (override arm); `:1503-1560` (tests)
- **Status**: NEW (searched "wrld override", "--wrld")
- **Description**: an unknown EDID makes the override lookup return nothing. Selection then falls through the grid and `PREFERRED_WORLDSPACES` chain with no warning. No test covers the override, whether it matches or not.
- **Evidence**: `MegatonWorld` and `Wasteland` both contain cell (-1,-7). `--wrld MegatonWrld --grid -1,-7` selects `wasteland`. The smoke gate catches this only because it greps the chosen worldspace name (`docs/smoke-tests/m-exteriors.sh:517-521`).
- **Impact**: a CLI/debug mis-load that looks like broken content. The launcher defaults cannot reach it.
- **Related**: #444, #2340.
- **Suggested Fix**: on an unmatched override, return an error, or at least `log::warn!` with the available EDIDs. Add tests for a matching and a non-matching override to `worldspace_selection_tests`.

## Completeness Checks
- [ ] **SIBLING**: Other CLI name overrides (e.g. `--cell`) are checked for the same silent fallback
- [ ] **TESTS**: `worldspace_selection_tests` covers both a matching and an unmatched `--wrld` override
