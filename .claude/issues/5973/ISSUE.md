# SCR-D1-2026-10-17-05: `DamageValue` / `RestoreValue` accept negative amounts; the Skyrim CK says the sign is discarded

Issue: #5973
Labels: medium, scripting, bug, gameplay

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-05: `DamageValue` / `RestoreValue` accept negative amounts; the Skyrim CK says the sign is discarded

- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/prims/actor_state.rs:211-229`; applier `fragment/effects.rs:517-560`; `crates/core/src/ecs/components/actor_values.rs:196-205`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The amount is lowered through `classify_value` (a negative literal or a run-time number is fine) and applied as `apply_damage(avif, amount)` / `restore(avif, amount)`. A negative `amount` therefore heals on `DamageActorValue` and re-damages on `RestoreActorValue`.
- **Evidence**: CK wiki (`ck-uesp-wiki/(main)/DamageActorValue - Actor.wiki`, `RestoreActorValue - Actor.wiki`): "Negative numbers will be converted to positive so -100 and 100 will have the same effect." `actor_values.rs:196-205` has no `abs`/`max(0)` on `amount`. The FO4 `DamageValue - ObjectReference` page is silent on sign (unverified for FO4/SF); `docs/engine/scripting.md:476-480` cites only the FO4 page.
- **Impact**: Scripted `DamageActorValue("Health", -N)` (a common way to write "damage by N") inverts to a heal on Skyrim.
- **Related**: M47.2 `02f22d793`.
- **Suggested Fix**: Take `abs()` of the evaluated amount in the `DamageActorValue` arm (Skyrim spellings), or decline a negative literal; document the FO4 sign rule as unverified.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
