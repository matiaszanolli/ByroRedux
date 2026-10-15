# #5865: GAME-D2-2026-10-15-01: The carried-key unlock runs before the activation refusal, so a script-blocked or Parent-Activate-Only locked reference is unlocked with nothing opened

- **Labels**: low,bug,gameplay,scripting
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/interaction.rs:1423-1453` (`unlock_with_carried_key`), called at `:1477-1500` before the `default_activation_blocked` / Parent-Activate-Only refusal (`:1535-1559`); target selection `:1229-1395`
- **Status**: NEW (from `9b5a3e29b` and `745d64830`). Merged from two agents.
- **Trigger**: a locked reference with `BlockActivation` set by script, or flagged Parent-Activate-Only, while the player carries its key.
- **Description**: `activate_target` unlocks first: it removes `Locked`, writes the saved `ReferenceLockState` and notifies "Unlocked with key". Only then is the activation refused. Parent-Activate-Only references are also not filtered from target selection or the HUD prompt, and each press logs at `log::error!`. Loot itself stays correctly blocked in `container_loot_system`.
- **Suggested Fix**: Evaluate the refusal before the key unlock, and filter Parent-Activate-Only references out of target selection.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
