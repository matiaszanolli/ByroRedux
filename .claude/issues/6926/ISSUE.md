# #6926 — INC-2026-11-14-14: `apply_effect`'s nested-lock residual list misses the three types `MoveTo` now reaches through `created_references.rs`

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-14). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `crates/scripting/src/fragment/effects/placement.rs` (`MoveTo` / `MoveToPackageLocation`); residual list in `crates/scripting/src/fragment/effects.rs`; scan `the_nested_lock_residual_list_names_every_type_apply_effect_acquires` in `crates/scripting/src/fragment/tests.rs`
- **Changed in**: commit `4b71155ca`

## Description
`apply_placement_effect` now reaches `PendingActorMoves` (W) via `queue_actor_move` and `UnloadedReferenceStub` / `RemoteSceneActorStub` (R) via `is_logical_stub`, both in `created_references.rs`. The #3949/#5986 scan reads only the fragment sources, `magic.rs`, and helpers in the fragment sources / `equipment.rs`, so it stays green while the residual list names none of the three. No deadlock today (every quest-resource system is exclusive); the gap is in the guard that enforces the nested-lock contract.

## Evidence
`grep -c 'PendingActorMoves\|UnloadedReferenceStub\|RemoteSceneActorStub'` over the residual-list doc block → 0; the scan's sources are `crate::fragment::SOURCES`, `../magic.rs`, `../equipment.rs`.

## Suggested Fix
Add the three types to the residual list and extend the scan to the `created_references.rs` helpers `apply_effect` calls.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (run the detector: `BYRO_LOCK_ORDER_CHECK=1`)
- [ ] **TESTS**: A regression test pins this specific fix
