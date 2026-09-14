# #4329 SCR-D6-2026-09-14-03: `SetLockLevel` on an untouched keyed reference records `key_form_id: None` while the live component keeps the key — ledger and component drift, and the drift is saved

**Labels**: low,scripting,save-load,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: LOW (latent: `Locked::key_form_id` has no reader; `activation_is_blocked` does no key check)
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/state.rs` `ReferenceLockState::set_lock_level` (`None => self.set_locked(form_id, lock_level, None)`); `crates/scripting/src/fragment/effects.rs` `Effect::SetLockLevel` arm (pushes only the requested level)
- **Status**: NEW (residual of #4136)
- **Description**: #4136's commit says the effects record the *outcome*, so ledger and component can't drift. The `SetLocked` arm does; `SetLockLevel` pushes only the level. On an authored-keyed door with no prior override, the live `Locked` stays `{level, Some(K)}`, the ledger gets `{level, None}`, and on the next load `scripted_lock_override` wins and the key is gone. The test `set_lock_level_on_an_untouched_reference_records_the_authored_lock` asserts `None`, which is only correct for keyless locks.
- **Impact**: None today. Once key checks land, a script-re-levelled keyed door can't be opened with its key after a revisit or load, and saves written before the fix keep the keyless override.
- **Related**: #4136, #3159
- **Suggested Fix**: In the `SetLockLevel` arm, read the component back and push `DeferredLockChange::Locked { lock_level, key_form_id }` (the outcome), mirroring `SetLocked`.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
