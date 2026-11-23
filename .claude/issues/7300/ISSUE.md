# #7300: SCR-D2-06: apply_effect's lock inventory omits the `Sneaking` marker and the `sneaking_enabled` pair that the M65 effect/guard take under the quest pair

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/effects/player_control.rs:35-43` (`Effect::StartPlayerSneaking` → `crate::stance::toggle_sneaking`), `fragment/effects/guards.rs:88-98` (`FragmentGuard::ActorSneaking` → `stance::is_sneaking`), `crates/scripting/src/stance.rs:40-79`, doc at `fragment/effects.rs:78-155`, scan at `fragment/tests.rs:4728-4788`
- **Status**: NEW
- **Description**: `toggle_sneaking` write-locks `Sneaking` and `sneaking_enabled` reads `PlayerControlState` and `InputLayers`; `is_sneaking` read-locks `Sneaking`. All run under the paired guards. `Sneaking` appears nowhere in the apply_effect doc block. The #3949 scan reads `fragment::SOURCES`, `magic.rs`, `equipment.rs` and `created_references.rs` (the `IN_CREATED_REFERENCES` precedent of #6926 shows the mechanism for adding a call-out module); `stance.rs` is not among them, so the test stays green. The same commit range did keep `SightQuery` current in the doc, so this is an isolated omission.
- **Evidence**: `grep -n Sneaking crates/scripting/src/fragment/effects.rs` returns no line in the doc block; `player_control.rs:39` `crate::stance::toggle_sneaking(world, player)`.
- **Impact**: The running lock inventory (the re-derivation baseline for any ABBA analysis) is incomplete; `Sneaking` ↔ `Awareness`/detection ordering was not reviewed against it.
- **Related**: #6910 (M65 Phase 1), #6926.
- **Suggested Fix**: Name `Sneaking` (write/read), `PlayerControlState`/`InputLayers` (read) under the "via `crate::stance`" bullet and add `stance.rs` to the scan inputs.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
