# #3489: SCR-D5-2026-08-27-02: Effect::Disable shipped without an Enable counterpart over a save-persisted resource — a latent one-way door, and 3,005 real Enable() calls decline

**Labels**: medium, scripting, quests, save-load, bug
**Filed**: 2026-08-27 (`/audit-publish` of `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`)

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:476-513` (`EFFECT_PRIMITIVES` — no `prim_enable`); `:803-812` (`prim_disable`); `crates/scripting/src/fragment.rs:63-84` (`ReferenceEnableState`, whose `set_enabled(form_id, bool)` API already supports both directions)
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

`5f38402e` added `Effect::Disable` and the save-serialized `ReferenceEnableState` resource it writes into, but no `Enable` primitive. The resource's own API is symmetric (`set_enabled` takes a `bool` and removes from the `disabled` set when `true`), and it is registered for save persistence (`byroredux/src/save_io.rs:438`, `.register_resource::<ReferenceEnableState>("ReferenceEnableState")`), so the *state model* is complete — only the lowering half is one-directional.

In the real corpus, `Enable()` is the **more common** of the pair:

```
disable  args=1  count=2587
enable   args=1  count=3005
```

Every fragment containing an `Enable()` call therefore declines in full today (the whole-fragment lowering contract), and once `ReferenceEnableState` gains the runtime consumer #3278 asks for, a reference a script disables can never be re-enabled by script — the disable survives save/load by design.

## Evidence

`EFFECT_PRIMITIVES` contains `prim_disable` at `effects.rs:493`; a grep for an `Enable` sibling finds only `prim_enable_player_controls` (`:500`, an unrelated `Game.EnablePlayerControls` primitive) — there is no `ObjectReference.Enable` lowering, and no `Effect::Enable` variant anywhere in `crates/scripting`. `ReferenceEnableState::set_enabled` (`fragment.rs:76-82`) has the `enabled == true` branch that no caller ever reaches:

```rust
pub fn set_enabled(&mut self, form_id: u32, enabled: bool) {
    if enabled {
        self.disabled.remove(&form_id);   // no production caller
    } else {
        self.disabled.insert(form_id);
    }
}
```

The single production caller is `fragment.rs:577` (`state.set_enabled(form_id, enabled)` draining `reference_enable_changes`), fed only by the `Disable` arm.

## Impact

Today, inert — nothing consumes `ReferenceEnableState` (#3278), so neither half does anything observable. **This finding's severity is conditional on #3278 being fixed**: the moment a consumer lands, disabling becomes permanent and unrecoverable across saves, and a `Disable`/`Enable` pair authored to hide a reference for one quest stage will hide it forever. Fixing #3278 without fixing this would ship a strictly worse state than either fix alone. Also caps fragment coverage: 3,005 guaranteed declines.

## Related

#3278 (`Effect::Disable` has no production consumer, and its receiver resolution is narrower than its siblings) — same commit, same effect, must be fixed together. Structurally identical to #3159 (a `Lock` with no `Unlock`), which the 08-20 pass already named as a one-way door.

## Suggested Fix

Add `prim_enable` mirroring `prim_disable` (same `receiver_object` treatment, same optional literal `abFadeIn` argument) and an `Effect::Enable` variant dispatching to `deferred.reference_enable_changes.push((form_id, true))`. Land it in the same change as #3278's consumer, not after.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other one-way effect pairs — `Lock`/`Unlock` per #3159 — and other `EFFECT_PRIMITIVES` entries whose state resource is symmetric)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
