# #4330 SCR-D6-2026-09-14-04: `SetLocked` / `SetLockLevel` on a non-resident reference record nothing, unlike `Enable` / `Disable`

**Labels**: low,scripting,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/fragment/effects.rs` `Effect::SetLocked` / `Effect::SetLockLevel` arms (`resolve_object(..)?` runs before `lock_ledger_key`)
- **Status**: NEW
- **Description**: `lock_ledger_key` could key a direct-property target with no loaded entity, but both arms return early when `resolve_object` fails. #3278 made `Disable` / `Enable` resolve `resolve_property_form_id(..).or_else(entity)` without requiring residency. As a result, a fragment that unlocks a door in a non-resident cell is a no-op (debug log only), and the authored XLOC stands when the player arrives.
- **Impact**: A quest-unlocked door in an unloaded cell stays locked. Vanilla frequency is unmeasured, and whether shipped `ObjectReference.Lock` applies to unloaded persistent refs is **UNVERIFIED**.
- **Related**: #3278, #4136
- **Suggested Fix**: For `locked == false` with a direct-property FormID, push `DeferredLockChange::Unlocked` even without an entity. Keep declining a re-lock (it needs the authored XLOC) and document why.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
