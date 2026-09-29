# #5069 — CONC-D4-2026-09-29-01: `equipment_appearance_system`'s Access row does not declare the mid-life gear-import surface `0182fc5e8` added

**Labels**: low,concurrency,ecs,bug

**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-09-29.md`
**Severity**: LOW
**Dimension**: Scheduler Access Declarations

## Location
- `byroredux/src/boot/schedule/late.rs` — `equipment_appearance_system` Access row
- `byroredux/src/npc_spawn/loot_appearance.rs` — `queue_midlife_imports`

## Description
`equipment_appearance_system` now calls `queue_midlife_imports` unconditionally. That function takes `world.get::<PendingGearImport>` (read), `world.get::<ActorBodyClass>` (read), `try_resource::<LoadedCellIndex>` (resource read) and `query_mut::<PendingGearImport>` (write). None is declared; the row still lists only `EquipmentEventBatch`, `NpcEquipmentPart`, `Dead`, `NpcAppearanceHidden` (write), `Children` and `MeshHandle`. Distinct from GAME-D1-2026-09-29-01 (#5031) (the queue's dropped-request logic) — this is the scheduler declaration only.

## Evidence
`git show 0182fc5e8 -- byroredux/src/boot/schedule/` is empty; the gear-import commit touched no schedule file. Same class as #4821, #4996 and ECS-2026-09-29-D5-01, but a third row that report does not name.

## Impact
None at runtime today (exclusive system; the analyzer never pairs exclusives). The row is the promotion baseline and the operator view (`sys.accesses`); the mechanical guard cannot see it because the system is not in `PARALLEL_SYSTEMS` and is not one of the three scanned exclusives.

## Related
ECS-2026-09-29-D5-01, #4821, #4996; GAME-D1-2026-09-29-01 (#5031) (same feature, different defect).

## Suggested Fix
Add `.reads::<crate::npc_spawn::ActorBodyClass>()`, `.writes::<crate::npc_spawn::PendingGearImport>()` and `.reads_resource::<crate::cell_loader::LoadedCellIndex>()`.

Validated at HEAD 9fcfdc3fc: the `late.rs` row for `equipment_appearance_system` still ends at `.reads::<MeshHandle>()` with no `PendingGearImport`/`ActorBodyClass`/`LoadedCellIndex`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
