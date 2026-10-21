# #6295: SAVE-2026-10-21-05: `NOT_SAVED_BY_DESIGN` carries a duplicate `CrimeFactions` row that the guard silently collapses, and `ActiveBarter`'s reason overstates its save gate

**Labels**: low,save-load,test-gap,bug

**Source**: docs/audits/AUDIT_SAVE_2026-10-21.md · **Severity**: LOW · **Dimension**: Snapshot Completeness & the Two Lists · **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:511`

- **Data-Loss Class**: none
- **Location**:
  - `byroredux/src/save_io/registry_completeness_tests.rs:511`, `:984` (two `("CrimeFactions", …)` rows), `:1040-1042` (`allowlisted` is a `HashSet`).
  - `:542` and `byroredux/src/barter.rs:202-204` (`ActiveBarter`).
- **Status**: NEW
- **Description**:
  - `f2d559a79` added a second `CrimeFactions` row with a different reason. The guard builds a `HashSet` of names, so duplicates disappear without a diagnostic. Its own #3710 comment names exactly this hazard ("a reintroduced short-name collision wouldn't show up as a duplicate key error"), but it guards only the two player-entity names.
  - `ActiveBarter`'s reason says every page-leave path ends the session "before a save can run". The remote and overlay console `save` ingresses dispatch with the barter page open. A save taken then captures the stand-in merchant container: a `FormIdComponent` entity with an `Inventory` and no cell root. On load it is benign: the row is unresolved and #5054's `park_unresolved_snapshot_rows` parks it (`reference_state.rs:898-1040`). The reason is still inaccurate.
- **Impact**: guard hygiene only.
- **Suggested Fix**: delete one `CrimeFactions` row, and assert `NOT_SAVED_BY_DESIGN` has no duplicate names (compare `len()` against the set). Reword `ActiveBarter` as "player ingresses are blocked while the page is open; a console save parks the stand-in's stock through the unresolved-row park".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
