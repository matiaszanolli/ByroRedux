# #7006: UI-D3-2026-11-15-03: PRD GUI-HOST row says 12 of the 15 result-used FO4 `Command` entries are name-prefix guesses; the TSV has 13

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/7006
- **Labels**: low,ui,game:fo4,documentation,doc-rot
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: Catalog & AVM1 Scanner
- **Profile**: Fallout4Avm2
- **Existing**: none
- **Location**: `docs/prd/game-ui-integration.md:786` (the "15 Fallout 4 `Command` entries have their result used (12 of them name-prefix guesses)" sentence) against `docs/data/game-ui/host-classes-fallout4.tsv`.
- **Evidence**: filtering the TSV to `kind == Command` with `sites` ending `result-used 1` gives 15 rows (the PRD's 15 reproduces). Split by `provenance`: `HeuristicNamePrefix` 13 (`AreModsLoaded`, `ConfirmAndCloseMenu`, `EndBodyEdit`, `RequestHelpText`, `RequestHelpTitle`, `RequestInstalledContentText`, `RequestInstalledContentTitle`, `ShowContinueSecondPanel`, `StartBodyEdit`, `attemptCloseManager`, `initMenu`, `populateCaravanList`, `startEditText`) and `Measured` 2. The PRD says 12.
- **Impact**: low. The conclusion ("a handler must return a value whatever `kind` says") is unchanged, but the figure is what a handler author uses to size the heuristic-`Command` review.
- **Suggested Fix**: change 12 to 13 in the PRD (or re-derive both from the TSV in the same edit), citing the TSV.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
