# TD6-2026-10-15-02: `AttachPoints` / `ChildAttachConnections` are stamped on every FO4 placement root but have no reader; their docs say the equip system reads them

Labels: low,gameplay,game:fo4,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD6-2026-10-15-02`).

- **Severity**: LOW
- **Dimension**: 6 Stub & Placeholder Implementations
- **Location**: `crates/core/src/ecs/components/attach_points.rs:1-30,95-140`; stamp site `byroredux/src/cell_loader/spawn.rs:551-570`; import `byroredux/src/cell_loader/references/import.rs:169-170`, `attach.rs`; docs `docs/engine/ecs.md:538`
- **Age**: `01b088119` 2026-05-12 (#985; 5 months)
- **Effort**: small (doc) / large (the equip consumer, FO4 OMOD weapon mods)
- **Status**: NEW (drivers #985 and #1594 are CLOSED - they closed the *import->ECS* half only)
- **Description**: The component module doc says "The equip system composes the world transform as `parent_world * attach_point.local_transform * accessory.local`", "The equip system queries this component on the parent", and `ChildAttachConnections` "The equip system reads this when an accessory is being mounted ... or the accessory can't mount". No such system exists. `AttachPoints::find` (the "equip hot path" lookup) has no caller outside its own tests, and nothing in `byroredux/src/` reads either component; the save ledger records them as "write-once NIF-import data, no runtime mutator" (`registry_completeness_tests.rs:587,595`). FO4 OMOD-driven weapon-mod assembly is not implemented, so the components are inert on every FO4 weapon placement.
- **Evidence**: `grep -rnw "AttachPoints\|ChildAttachConnections" byroredux crates --include='*.rs'` minus tests -> builders (`references/attach.rs`), cache fields (`nif_import_registry.rs:317,322`), the stamp (`spawn.rs:561-566`) and `world.insert`; zero `get`/`query`/`find` sites. `ROADMAP.md`/`feature-matrix.md` make no weapon-mod claim (so no roadmap lie), but `ecs.md:538` lists them in the component catalog without the "unconsumed" caveat.
- **Impact**: per-modular-weapon placement carries two inert SparseSet inserts (+ a `FixedString` intern pass at import) for a consumer 5 months overdue; the doc misdirects an FO4 weapon-mod implementer into thinking half the equip path exists. Whether the NIF-side `attach_points`/`child_attach_connections` cache fields are worth their per-import cost is the sibling Dim 8 question.
- **Related**: #985, #1594 (both CLOSED), `registry_completeness_tests.rs:587`.
- **Suggested Fix**: Add a "No consumer yet - FO4 OMOD weapon-mod assembly (unimplemented) will read this" paragraph to `attach_points.rs` and an annotation in `ecs.md:538`, matching the honest style of `audio/src/lib.rs:14-18` (#5612) and `scripting/src/timer.rs:7-17` (#5608). Re-file the consumer as a tracked ROADMAP item rather than leaving it implied by a closed issue.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
