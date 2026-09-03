# #3747 — TD8-2026-08-30-02: two dead `pub fn` NPC-spawn compatibility shims

**Severity**: LOW · **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
**Location**: `byroredux/src/npc_spawn.rs` — `spawn_npc_entity` and `spawn_prebaked_npc_entity`
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD8-2026-08-30-02`)

## Premise, re-verified

Both `pub fn`s carried `#[allow(dead_code)]` and had zero real call sites —
re-confirmed at HEAD before touching anything (`grep -rn 'spawn_npc_entity\b'`
/ `'spawn_prebaked_npc_entity\b'` across the whole tree returns only doc/code
*comment* hits, no calls). Each was a ~30-line synchronous wrapper around
`NpcSpawnJob` (`npc_spawn/resumable.rs`) with an unlimited frame-time budget
— superseded by the resumable job, never deleted.

## Fix

- Deleted both functions (and their `#[allow(dead_code)]` /
  `#[allow(clippy::too_many_arguments)]` attributes) from `npc_spawn.rs`.
  `prebaked_facegen_nif_path`/`prebaked_facegen_tint_path`, which sat between
  them and are genuinely still called from `resumable.rs`, are untouched.
- **Doc content wasn't just deleted** — each wrapper's doc comment carried
  real information (kf-era vs pre-baked path semantics, the budget/yield
  contract, `CellRoot` ownership split) that had no other home. Migrated it
  onto `NpcSpawnJob::runtime` and `NpcSpawnJob::prebaked`
  (`npc_spawn/resumable.rs`) — the actual live constructors — instead of
  losing it.
- `tag_descendants_as_actor`'s doc, which named both dead functions as its
  callers, now correctly names `NpcSpawnJob::advance`'s success paths
  (its real callers, confirmed via grep — `resumable.rs:965,1306,1369`).

## SIBLING (issue's own checklist item)

Swept `#[allow(dead_code)] pub fn` workspace-wide for the same pattern.
Found exactly one other hit: `layer_affinity`/`layer_affinities` in
`groundcover_translate.rs`. **Not the same pattern** — their own doc
comments self-describe as forward-looking infrastructure
("`#[allow(dead_code)] // see DEFAULT_AFFINITY — Phase 1 scatter is the
consumer`") for the still-open #3807 (ground-cover density streaming, EX-14
item A), and they're exhaustively exercised by
`groundcover_translate_tests.rs` today. A superseded compat shim with zero
test coverage is a different situation from tested infrastructure awaiting
its production caller — left alone.

## TESTS (issue's own checklist item)

`byroredux/src/npc_spawn/tests.rs`'s `apply_ai_package_behavior_*` tests
already exercised the *extracted* helper, not the deleted wrappers
themselves (neither wrapper had direct test coverage to begin with — the
zero-call-site premise held for tests too). Re-pointed the provenance
comment above those tests to name `NpcSpawnJob` and cite this issue instead
of the now-nonexistent `spawn_npc_entity`, per the issue's "re-point rather
than dropping coverage" instruction — no coverage was ever there to drop.

Also re-pointed 8 more stale doc/comment references across the tree that
named the dead functions in historical or descriptive prose (not calls):
`scene.rs`, `save_io/round_trip_tests.rs` (×2), `systems/animation.rs`,
`cell_loader/references/mod.rs`, `crates/core/src/ecs/systems.rs`,
`crates/plugin/src/esm/records/misc/pack.rs`,
`crates/plugin/src/esm/records/actor/mod.rs`,
`crates/plugin/src/esm/records/actor/tests.rs` — each now names
`NpcSpawnJob` (or, for two purely historical bug narratives whose value is
the *bug*, not the *symbol name*, "the NPC spawn path") instead of a symbol
that no longer resolves. This is the "amplifying detail" the issue itself
called out: `spawn_npc_entity` was one of the most-cited names in the
codebase's prose, so a reader following any of those references would have
landed on a dead wrapper.

## Verification

- `cargo check -p byroredux -p byroredux-core -p byroredux-plugin --tests`:
  clean (one pre-existing, unrelated `unused_mut` warning in
  `esm/records/grup_walker.rs` predates this fix, not introduced by it).
- `cargo test -q -p byroredux -p byroredux-core -p byroredux-plugin`: all
  passing, 0 failing.
- `cargo test -q --no-fail-fast` (full workspace): **7074 passing, 0
  failing** — same count as before (no coverage lost, none gained; this was
  a pure dead-code removal + doc re-pointing pass).
