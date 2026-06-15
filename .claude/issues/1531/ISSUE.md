**Severity**: HIGH · **Dimension**: 3 — Memory & Resource Leaks (per-cell)
**Location**: `byroredux/src/cell_loader/unload.rs:186` + `:365-388` (`release_victim_rapier_bodies`); cleanup fn at `crates/physics/src/ragdoll.rs:319-323` (`PhysicsWorld::remove_ragdoll`); ragdoll bodies attached at `byroredux/src/ragdoll.rs:229-236`
**Source**: `docs/audits/AUDIT_SAFETY_2026-06-14.md` (SAFE-D3-NEW-01)

## Description
The PHYSAL ragdoll path (landed #1528/#1529) attaches a `Ragdoll` component to an actor on `ragdoll <id>` activation. `Ragdoll` carries its own `Vec<(EntityId, RigidBodyHandle)>` (`crates/physics/src/components.rs`) — these bodies are inserted directly into `PhysicsWorld::{bodies, colliders, multibody_joints}` by `build_ragdoll`, **not** via the `RapierHandles` component that the character/physics-sync path uses. The cell-unload leak guard `release_victim_rapier_bodies` only sweeps victims carrying `RapierHandles` (`unload.rs:370-377`); it never inspects the `Ragdoll` component. So when a cell unloads with a ragdolling actor in it, `world.despawn(eid)` drops the `Ragdoll` ECS row and **orphans** its Rapier bodies + colliders + multibody joints in the solver's sets and broad-phase / query-pipeline BVH — the exact unbounded-leak shape #1520 was filed to close, re-introduced for the new component.

## Evidence
- `grep -rn remove_ragdoll byroredux/src/` → **zero call sites.** The cleanup function `crates/physics/src/ragdoll.rs:319` (`pub fn remove_ragdoll`, whose own doc-comment says *"Mirrors the #1520 no-leak discipline so a cell unload mid-ragdoll doesn't strand bodies"*) is dead code.
- `unload.rs:365-388` removes only `RapierHandles` rows; there is no `Ragdoll` branch and no `remove_ragdoll` call in the unload sequence.
- There is **no deactivation path at all**: `activate_ragdoll` (`byroredux/src/ragdoll.rs:170`) inserts `Ragdoll` + `RagdollActive`; nothing in the binary ever removes either, so even an in-place re-load or actor death can't reclaim the bodies in-session.

## Impact
Per-cell leak of N rigid bodies + colliders + multibody joints (N = ragdoll bone count, ~10-20 for a humanoid) into the Rapier sets and BVH for every ragdolling actor present at unload. Multibody joints accumulating in the solver also degrade step cost over the app lifetime. The trigger today is manual-only (`ragdoll <id>` console command, no automatic death-ragdoll yet), so it is not yet a steady streaming leak in ordinary play — but it leaks deterministically the moment any manually-ragdolled actor's cell unloads, and becomes a continuous exterior-streaming leak the instant ragdoll-on-death is wired (the obvious next PHYSAL step). Rated HIGH per the #1520 precedent (same leak class, same severity) and because the fix already exists and is simply unconnected.

## Related
#1520 (CLOSED, `34c7a218` — the `RapierHandles` sibling of this exact leak); `crates/physics/src/ragdoll.rs:314-324`; the `Ragdoll` component `crates/physics/src/components.rs`.

## Suggested Fix
In `release_victim_rapier_bodies`, also collect each victim's `Ragdoll` component and call `pw.remove_ragdoll(&ragdoll)` (it already cascades colliders + multibody joints via `remove_body`). Add the `Ragdoll` row to the same victim sweep, and extend the `rapier_release_tests` guard to assert the body/collider/joint sets are empty after unloading a ragdolling actor. (Separately, wire a deactivation command/path so manual ragdolls can be reclaimed without a cell crossing.)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `RapierHandles` release branch is the sibling — both must sweep)
- [ ] **LOCK_ORDER**: If a RwLock scope changes in `release_victim_rapier_bodies`, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (body/collider/joint sets empty after unloading a ragdolling actor)
