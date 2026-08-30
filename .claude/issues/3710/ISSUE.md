# #3710 — ECS-2026-08-30-P2-07: two distinct PlayerEntity resources describe the same fact with different null semantics

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW · **Dimension**: P2 Gameplay Slice / Resource shape
**Location**: `byroredux/src/systems/character.rs:42` (`PlayerEntity(pub Option<EntityId>)`) and `crates/scripting/src/papyrus_demo/mod.rs:71` (`PlayerEntity(pub EntityId)`); consumed side-by-side at `byroredux/src/combat.rs` (~:106-108) vs `byroredux/src/interaction.rs` (~:788-790)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-P2-07, `[P2-gameplay]`)

> **Coverage note**: this file has no owner audit skill. The finding comes from the `/audit-ecs` run's explicit P2-gameplay slice sweep and is the only audit coverage it received.

## Description

`crate::systems::PlayerEntity(pub Option<EntityId>)` and `byroredux_scripting::papyrus_demo::PlayerEntity(pub EntityId)` hold the same canonical fact and are used interchangeably for the same purpose in the slice: `combat_input_system` excludes the aggressor's Rapier body using the first, `target_has_line_of_sight` excludes the player's body using the second. Both are written from one site.

## Impact

No divergence is reachable today — character mode binds both to the same body, and the fly-cam placeholder has no `RapierHandles`, so LOS exclusion degrades correctly. LOW because the two have different null encodings (`None` vs a live placeholder entity): a consumer migrating from one to the other silently changes its "no player" behaviour. It also makes the save registry-completeness guard ambiguous — it keys on the short type name, so one allowlist row covers both types.

## Suggested Fix

Keep one canonical `PlayerEntity` and have the scripting crate re-export it; at minimum pick one for all self-exclusion ray casts so combat and activation cannot disagree.

## Completeness Checks
- [ ] **SIBLING**: Every consumer of either `PlayerEntity` audited for null-semantics assumptions
- [ ] **TESTS**: A regression test pins that the save registry-completeness allowlist distinguishes the two (or that only one type remains)
