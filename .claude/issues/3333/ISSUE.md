# FNV-2026-08-26-D9-03

**Issue**: #3333
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/npc_spawn/ai_package.rs:416-437` (`clear_ambient_behavior`) · `byroredux/src/systems/sandbox.rs:281-292` (the park) · called from `ai_package.rs:599` (M42.9 runtime swap) and `byroredux/src/combat.rs:411`

**Premise verified**: `sandbox_seat_system` deliberately overwrites four
`AnimationPlayer` fields to hold the seated end pose:
```rust
p.clip_handle = sit_handle;
p.local_time  = hold_time;   // = clip duration
p.prev_time   = hold_time;
p.playing     = false;       // the enter clip's Reverse cycle would ping-pong back to standing
p.speed       = 1.0;
```
`clear_ambient_behavior` removes `SandboxBehavior`, `Seated`, and the
other 14 AI components, and releases the actor's `SeatReservations`
entries — but touches `AnimationPlayer` nowhere. A repo-wide grep finds
no restore path: the only production `AnimationPlayer` construction for
an NPC is at spawn (`npc_spawn/resumable.rs:836-837`,
`AnimationPlayer::new(handle).with_root(skeleton)` seeded by
`idle_desync`), and nothing re-applies it after unseating.

Before M42.9 this was unreachable — `SandboxBehavior` was attached once at
spawn and never removed. `ambient_ai_package_system` (#2652) made it
reachable: it re-selects the winning package once per in-game minute and,
on a change, calls `clear_ambient_behavior` then installs the new behavior
(`ai_package.rs:595-603`).

**Evidence**: `ai_package.rs:416-437` in full — the list is
`SeatReservations` retain, then `SandboxBehavior`, `Seated`,
`Wander{Behavior,State}`, `Travel{Behavior,State}`, `Traveled`,
`Follow{Behavior,State}`, `Escort{Behavior,State}`, `Escorted`,
`Guard{Behavior,State}`, `Patrol{Behavior,State}`. No `AnimationPlayer`
line, and no `AnimationPlayer` import in the module.

**Impact**: on FNV with `BYRO_SANDBOX_SIT=1`, a saloon patron seated
under a daytime Sandbox package whose schedule hands over to an evening
Travel/Wander/Patrol package is un-seated correctly and starts walking —
in a frozen chair pose, `playing = false`, and it never animates again
for the rest of the session regardless of what package it later wins.
The FNV corpus makes this a live path: 753 Sandbox packages, and 1350 of
4163 PACK records carry a real (non-`0xFF`) PSDT start hour, so
schedule-driven handovers are ordinary content, not an edge case. The
same teardown runs on death (`combat.rs:411` →
`reconcile_dead_actor`), where ragdoll takes over the pose, so that path
is unaffected.

**Fix sketch**: have `clear_ambient_behavior` restore the actor's default
idle player (clip handle + `idle_desync` phase/speed + `playing = true`)
whenever it removes a `Seated` that was present — mirroring how it
already special-cases `SeatReservations` rather than blanket-removing
components; or hold the pre-seat `AnimationPlayer` snapshot on `Seated`
itself so the restore needs no archive access.

---

---


## LOW (22)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
