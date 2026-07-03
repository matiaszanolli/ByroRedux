# #1864: SCR-D7-NEW-01: QuestStageAdvanced markers collide on a shared single-entity sink

**Severity**: MEDIUM
**Location**: `crates/scripting/src/papyrus_demo/quest_advance.rs`, `crates/scripting/src/fragment.rs`

`QuestStageAdvanced` was declared `impl Component for QuestStageAdvanced { type
Storage = SparseSetStorage<Self>; }`, which allows exactly one component
instance per entity, overwriting in place on a repeat insert. Both live
producers write multiple events onto the same shared sink entity
(`PlayerEntity`) within one system call whenever two different scripted
doors/triggers fire in the same tick — only the last event survived.

## Fix (approach confirmed with user — batch component over accumulating resource)
- `QuestStageAdvanced` is now plain data (no `Component` impl).
- Added `QuestStageAdvancedBatch(pub Vec<QuestStageAdvanced>)`, which IS the
  `Component`. Every same-frame advance is collected into one `Vec` and
  inserted **once** per frame, instead of looping `insert()` per event.
- `quest_advance_system` phase 3: builds the full `advances_emitted` vec first
  (unchanged), then a single `q.insert(player_entity,
  QuestStageAdvancedBatch(advances_emitted))`.
- `fragment.rs`'s read side (building the dispatch `queue`) and cascade
  emission (`chained`) both updated the same way — iterate/insert the whole
  batch, not a loop of single inserts.
- `cleanup.rs`: `drain_component::<QuestStageAdvanced>` → `drain_component::<QuestStageAdvancedBatch>`
  (the existing per-frame marker + cleanup lifecycle is otherwise unchanged).

This keeps every existing SparseSetStorage/cleanup machinery intact — only the
component's shape changed (single value → batch), not the lifecycle mechanism.

## Completeness Checks
- [x] **TESTS**: Added `two_same_frame_advances_for_different_quests_are_both_observed`
      in `fragment/tests.rs` (two advances, two different quests, one batch —
      both quests' fragments run). Rewrote `two_doors_same_quest_advance_in_one_pass`
      in `quest_advance/tests.rs`, which previously *documented the bug as
      correct behavior* ("the marker collapsed to one... not a bug") — now
      asserts both same-frame advances survive in the batch.
- [x] **SIBLING**: The fragment cascade half (`fragment.rs`) is fixed by the
      exact same `QuestStageAdvancedBatch` mechanism, not a separate fix —
      confirmed by construction (both sites use the same type).

---

# #1865: SCR-D6-NEW-03: Globals resource unconditionally rebuilt on every interior cell load but guarded on exterior

**Severity**: MEDIUM
**Location**: `byroredux/src/cell_loader/load.rs`, `byroredux/src/cell_loader/exterior.rs`

`exterior.rs` guards the `Globals` resource rebuild with `is_none()` (preserving
runtime mutations across streaming); `load.rs`'s interior path rebuilt it
unconditionally on every load, silently discarding any accumulated runtime
`Globals::set` mutation the moment a write path exists (dormant today — no
production `SetGlobalValue` writer exists yet).

## Fix
Extracted a shared `pub(crate) fn ensure_globals_resource(world, records)` in
`load.rs` (guards on `try_resource::<Globals>().is_none()`) and made BOTH
`load.rs`'s interior path and `exterior.rs`'s exterior path call it — a single
shared implementation rather than two independently-maintained copies of the
same guard, so they structurally cannot drift apart again.

## Completeness Checks
- [x] **TESTS**: Added `ensure_globals_resource_preserves_runtime_mutation_across_reload`
      — builds a `Globals` from records, mutates via `Globals::set`, calls the
      helper a second time with the same records (simulating an
      interior-to-interior transition), and asserts the mutation survives.
- [x] **SIBLING**: Guard is now byte-identical between both call sites (a
      shared function, not a mirrored copy) — cannot drift.
