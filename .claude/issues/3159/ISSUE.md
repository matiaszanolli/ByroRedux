# Issue #3159: SCR-D5-2026-08-20-01: no Lock/Unlock effect primitive exists, and #3098 made that a one-way door — a fragment that unlocks a door declines wholesale, taking its sibling SetStage with it

- **Finding ID**: `SCR-D5-2026-08-20-01`
- **Severity**: MEDIUM
- **Labels**: `medium,scripting,bug`
- **Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3159

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3159 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: 5 — Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs`:57-140 (the `Effect` enum), :398-431 (`EFFECT_PRIMITIVES`) · `byroredux/src/interaction.rs`:936-943 (the new gate) · `byroredux/src/components.rs`:94-107 (`Locked`) · `byroredux/src/cell_loader/spawn.rs`:828-836 (the only insert)
- **Status**: NEW

## Description

`1e9723ab` (Fix #3098) introduced a `Locked` marker, stamped from an authored
`XLOC`, and made `activation_is_blocked` return `true` on its presence — the
commit calls this "the deliberately blunt first policy". The blunt half is
documented.

What is **not** documented, and is the part that lives in this domain, is that
**nothing anywhere in the engine removes `Locked`**. There is one insert
(`spawn.rs`:832) and one read (`interaction.rs`:941), no `world.remove::<Locked>`
on any path, and the `Effect` enum has 33 variants covering quests, objectives,
items, scenes, player control, vehicles, idles and cinematics — but no `Lock`,
`Unlock` or `SetLockLevel`.

The scripting consequence is sharper than "the feature is missing".
`lower_fragment` is a flat-sequence lowerer whose `_ => return None` arm declines
the **entire fragment** on one unmodeled statement. A vanilla `QF_` fragment
shaped like

```
MyDoor.Lock(false)
SetObjectiveCompleted(10)
SetStage(20)
```

therefore contributes **nothing** — the objective and the stage advance are
discarded along with the unlock. That is the *correct* decline (a partial
lowering would be worse), but it means the missing primitive costs far more than
the unlock itself.

## Evidence

```
$ grep -rn "Locked" byroredux/src crates --include="*.rs" | grep -v test
byroredux/src/components.rs:94:        pub(crate) struct Locked {
byroredux/src/cell_loader/spawn.rs:832:            Locked {
byroredux/src/interaction.rs:941:    if world.get::<Locked>(entity).is_some() { return true; }
```

— one insert, one read, no removal.

```
$ grep -in "prim_lock\|SetLockLevel\|Effect::Lock\|Effect::Unlock" \
    crates/scripting/src/translate/effects.rs
(no matches)
```

`EFFECT_PRIMITIVES` (`effects.rs`:398) — 33 entries, none matching `Lock` /
`Unlock` / `SetLockLevel`.

**The scripted-activation path is unaffected and correct** (checked, recorded so
it is not re-investigated): `Effect::Activate` reaches `ActivateEvent` through
`PendingFragmentActivations` → `fragment_activation_flush_system`, which never
consults `activation_is_blocked` — matching Papyrus, where `Activate()` bypasses
lock state. Only the **player's** interaction path is gated, and only the
player's path can traverse a door.

## Impact

Every authored-locked door and container is impassable for the whole session in
every target game — the #3098 commit message counts **378 locked REFRs, 103
keyed, on vanilla `FalloutNV.esm` alone** — with no key check, no lockpick, and
now no scripted escape either.

Any quest whose progression depends on a script unlocking a door is unfinishable,
and the fragment that would have done it silently contributes **zero** effects
rather than partial ones — so the failure presents to the player as *"the quest
stalled"*, not as *"the door is locked"*.

## Related

- **#3098 (CLOSED)** — the interaction half is a documented deliberate deferral;
  the *scripting* half is not mentioned there
- #2289 — new effect primitives lacking decline-path tests

## Suggested Fix

Add `Effect::SetLocked { target: ObjectRef, locked: bool }` behind a `prim_lock`
matching `ObjectReference.Lock(abLock)` / `.SetLockLevel(..)`, with the same
conservative-shape discipline `prim_set_open` uses (literal-only bool via
`bool_arg`, decline on any extra argument), and have `apply_effect`
insert/remove the `Locked` component.

That is a small, self-contained increment and it converts the wholesale fragment
decline into a working one.

If the effect is not wanted yet, at minimum record the coupling in `Locked`'s
docstring so the next reader of `interaction.rs`:941 knows nothing can clear it.

---
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md` (finding `SCR-D5-2026-08-20-01`)

## Completeness Checks
- [ ] **SIBLING**: Any other component inserted by the cell loader and read as a gate, with no remover — same one-way-door shape
- [ ] **TESTS**: A regression test pins this specific fix — both the accept shape and the decline shape (`bool_arg` non-literal, extra argument), per #2289
