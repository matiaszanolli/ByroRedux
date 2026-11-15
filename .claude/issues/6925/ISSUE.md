# #6925 — INC-2026-11-14-13: A save during a door fade restores `Traveled` without `CrossSpaceHop`, so the actor never passes the door

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-13). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/systems/travel.rs` (Pass 2: `reach_door` + `Traveled` insert); `byroredux/src/save_io/registry.rs` (`Traveled` saved); `byroredux/src/save_io/registry_completeness_tests.rs` (`CrossSpaceHop` / `DoorFade` NOT_SAVED)
- **Changed in**: commit `4b71155ca`

## Description
A traveler that reaches its load door gets `DoorFade` (via `reach_door`) and `Traveled`; the pass now waits `fActorTeleportFadeSeconds`, where it used to happen within the frame. `Traveled` is saved; `CrossSpaceHop` and `DoorFade` are not — the allowlist reason says the hop is "chosen again from its package after a load".

That cannot happen: `travel_system` skips actors that already have `Traveled`, and `package_flags::procedure_done` reports a Travel procedure done when `Traveled` is present. Nothing else removes `Traveled` except a package handover. A save inside the fade window therefore restores the actor standing at the door, arrived, with no hop.

## Impact
The actor never crosses to its destination beyond the door; it stands there until its package changes (or a Must Complete package ends without the move).

## Suggested Fix
After a load, drop `Traveled` for an actor whose `CrossSpaceHop` is gone (so the hop is re-chosen, as the allowlist assumes), or resolve pending door fades before save capture.

## Completeness Checks
- [ ] **SAVE**: If a saved type's shape changes, `serde_default_guard_tests.rs` baseline is refreshed / `FORMAT_MAJOR` bumped
- [ ] **SIBLING**: Same pattern checked in related files (`stream_snapshot.rs` also snapshots `Traveled` when a cell streams out)
- [ ] **TESTS**: A regression test pins this specific fix (save mid-fade, load, actor passes)
