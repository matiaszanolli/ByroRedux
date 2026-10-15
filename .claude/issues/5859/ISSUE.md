# #5859: GAME-D4-2026-10-15-01: `Bleedout` is not in the per-reference eviction row; an essential NPC evicted mid-bleedout comes back standing at Health ≤ 0 and never recovers

- **Labels**: medium,bug,gameplay,combat,save-load
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Severity**: MEDIUM
- **Dimension**: Dim 2 (loot/placement persistence) × Dim 4
- **Location**: `byroredux/src/cell_loader/reference_state.rs:27-58` (`ReferenceState`), `:111-254` (`capture`), `:257-310` (`restore`), `:668-760` (`park_unresolved_snapshot_rows`)
- **Status**: NEW (from `dd843f9e6`). Found independently by the Dim 4 and Dim 2 agents.
- **Trigger**: Skyrim, `RiverwoodSleepingGiantInn`. Put Delphine into bleedout, leave through the inn door within `fEssentialDeathTime` (20 s), and return. The save path hits the same gap: a save taken while she sits in the hysteresis band is parked, then reloaded.
- **Description**:
  - A spared actor keeps Health ≤ 0 (`combat.rs:362-371`), and only `bleedout_system` brings it back, by visiting `Bleedout` holders.
  - `capture` and the park path record `actor_values`, `control` and `dead`, but not `Bleedout`, so `restore` re-inserts the ≤ 0 Health with no `Bleedout`.
  - The actor is then neither dead nor unconscious: AI, dialogue and combat resume at ≤ 0 HP, and no NPC health regeneration exists to fix it.
  - The #5017 sibling `ActorControlState` is in the row; `Bleedout` was added to the save registry (`save_io/registry.rs:372`) but not here.
- **Impact**: The essential NPC is stuck in a broken state, and later saves keep it.
- **Suggested Fix**: Add a required `bleedout: Option<f32>` field to `ReferenceState`, carried through `capture`, park and `restore`; no serde default (#4465) and a `FORMAT_MAJOR` bump (`/audit-save`). Test: capture then restore an actor holding `Bleedout` with ≤ 0 Health, and assert it still bleeds out and then recovers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **SAVE**: Any serialized-shape change bumps `FORMAT_MAJOR`, adds no serde default (#4465), and refreshes the shape baseline (`/audit-save`)
