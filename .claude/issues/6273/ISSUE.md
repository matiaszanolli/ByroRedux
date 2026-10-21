# #6273: TD2-2026-10-21-03: eviction parking spells each parked component in six places

**Labels**: low,save-load,tech-debt,bug

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: LOW · **Dimension**: 2 · **Location**: `byroredux/src/cell_loader/reference_state.rs:28-96` (`ReferenceState` / `ScriptState` fields), `:172-364` (`capture`, one guarded pass per component), `:439-603` (`restore`), `:898-1080` (`park_unresolved_snapshot_rows`: 18 `column::<T>(snapshot, "Name")` string keys plus the `has_*` keep …

- **Severity**: LOW
- **Dimension**: 2
- **Location**: `byroredux/src/cell_loader/reference_state.rs:28-96` (`ReferenceState` / `ScriptState` fields), `:172-364` (`capture`, one guarded pass per component), `:439-603` (`restore`), `:898-1080` (`park_unresolved_snapshot_rows`: 18 `column::<T>(snapshot, "Name")` string keys plus the `has_*` keep predicate at ~`:985`), `byroredux/src/save_io/registry_completeness_tests.rs:1143-1270` (`PARKED`, `NOT_PARKED`, `FIELDS_WITHOUT_A_COLUMN`)
- **Status**: NEW
- **Age**: grew 842→1117 prod LOC in the window (`611769b16` #6071–4, `3b9060701` #6085 +267 lines, `a44d4a20d`, `dc829fc83`)
- **Effort**: medium
- **Description**: Adding a parked component (#6085 added three) edits the struct, the capture pass, the restore insert, the save-load park column (keyed by a **string** that must equal the save-registry name in `save_io/registry.rs`), the keep-rule predicate and the guard's `PARKED` table. `every_delta_column_is_parked_or_explicitly_not` (#6085) proves the partition and that each named field exists. It does not prove that `capture`, `restore` and `park_unresolved_snapshot_rows` each handle every field. A field added to the struct and to `capture` but missed in `park_unresolved_snapshot_rows` would silently drop that state on a save load. That is the class #6085's own doc says drifted twice (GAME-SD-01/02, SAVE-SD-03). All 17 columns are wired today.
- **Evidence**: `column::<byroredux_scripting::SpellList>(snapshot, "SpellList")` (`:925`) and `("SpellList", "ReferenceState.spells")` (guard `:1150`) are separate literals for the same fact.
- **Impact**: This costs work on every new gameplay component and risks a silent save-load state drop.
- **Related**: SAVE-2026-10-21-04 (nine `UNAUDITED` rows in the same partition), SAVE-2026-10-21-06, *save_registry_order_sensitive* memory.
- **Suggested Fix**: Use one `const PARKED_COLUMNS: &[ParkedColumn]` (name, field accessor, capture/restore/park fns) or a small macro. Generate `capture`/`restore`/`park` loops from it, and have the guard iterate the table instead of a parallel literal list.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
