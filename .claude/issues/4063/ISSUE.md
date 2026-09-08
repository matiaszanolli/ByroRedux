# Issue #4063 — ECS-2026-09-08-D7-01: six of the seven M42 AI procedures are invisible to byro-dbg

Filed: 2026-09-08 from `docs/audits/AUDIT_ECS_2026-09-08.md` via `/audit-publish`
Repo state at filing: `bb8ced68`
Labels: low, ecs, tech-debt, bug

---

**Severity**: LOW · **Dimension**: 7 (component lifecycles / observability)
**Location**: `crates/debug-server/src/registration.rs` — `register_components`, beside the `SandboxBehavior` / `Seated` pair
**Source**: `docs/audits/AUDIT_ECS_2026-09-08.md`

## Description

`register_components` registers 27 types, including `SandboxBehavior` and `Seated`, but none of the six later M42 procedure runtimes:

`WanderBehavior`/`WanderState` · `TravelBehavior`/`TravelState`/`Traveled` · `FollowBehavior`/`FollowState` · `EscortBehavior`/`EscortState`/`Escorted` · `GuardBehavior`/`GuardState` · `PatrolBehavior`/`PatrolState`

Sandbox landed with M42 and got a registration; M42.3–M42.8 each added a pair and none extended this file.

## Evidence

`grep -c "register_component::<X>"` over `crates/debug-server/src/registration.rs` returns 1 for `SandboxBehavior` and `Seated`, and **0** for all twelve of the others.

Every one of them already carries `#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]` — the exact bound `register_component<T: Component + Serialize + DeserializeOwned>` needs — so each is a one-line addition with no type work.

## Impact

`byro-dbg` can inspect a *seated* NPC and nothing else. The six unregistered procedures are the ones with live per-tick state — `WanderState`'s phase, `TravelState`'s frozen destination, `FollowState`'s re-resolved target, `GuardState`'s anchor + leash — which is exactly what an operator needs when an actor misbehaves, and exactly what the seven env-var-gated systems (`BYRO_WANDER`, `BYRO_TRAVEL`, `BYRO_FOLLOW`, `BYRO_ESCORT`, `BYRO_GUARD`, `BYRO_PATROL`) exist to be debugged through.

Observability gap; no runtime effect.

## Related

- The same seven-procedure roster `npc_spawn::ai_package::clear_ambient_behavior` has to stay in step with — that list *is* complete, which is what makes this omission a registry-only oversight.

## Suggested Fix

Add the twelve `register_component::<T>` lines next to the `SandboxBehavior` / `Seated` pair.

Consider a compile-time pin that the behavior roster in the `AmbientBehavior` enum and the debug registry have the same arity, so the next procedure cannot land half-wired.

## Completeness Checks
- [ ] **SIBLING**: check the same roster against every other place it is enumerated (`clear_ambient_behavior`, `boot.rs`'s env-var gates, `build_world`'s `register::<T>` prelude)
- [ ] **TESTS**: a test asserts the `AmbientBehavior` variant count and the registered behavior-component count agree, so a new procedure fails the build rather than landing unregistered
