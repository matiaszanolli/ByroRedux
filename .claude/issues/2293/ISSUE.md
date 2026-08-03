# SAVE-D1-10: Dead actor-lifecycle marker unregistered — forward-latent, not yet exploitable

**Filed from**: `docs/audits/AUDIT_SAVE_2026-08-03.md`
**Labels**: medium, ecs, bug

**Severity**: MEDIUM
**Dimension**: Snapshot Completeness & Determinism
**Data-Loss Class**: silent-drop (no live trigger today — forward-latent)
**Location**: `crates/core/src/ecs/components/actor_state.rs:8-15` (new file this cycle), `crates/scripting/src/condition.rs:484-485` (`GetDead` CTDA reads it), `byroredux/src/boot.rs:414` (`world.register::<Dead>()`), `byroredux/src/save_io.rs:188-249` (absent)

## Description
`Dead` is a sparse marker ("absence means alive") registered as an ECS type and consumed by `GetDead`, but nothing in the live codebase currently inserts it outside of `condition.rs`'s own `#[cfg(test)]` (`get_dead_tracks_sparse_actor_lifecycle_marker`). `crates/core/src/combat.rs` contains only pure damage-formula helpers, no death-resolution system. Verified no combat/kill system exists anywhere in `crates/` or `byroredux/src/systems/`.

## Impact
None today — no live path sets `Dead`. Forward-latent: the moment a death-resolution system lands, a dead NPC reviving on every load is a worse variant of SAVE-D1-08's (#2291) bug class, and there was no tracking issue reserving the follow-up before this one.

## Suggested Fix
No urgent code change required. This issue is the tracking reservation: register `Dead` in `build_save_registry` in the same commit that ships the first system to insert it during real gameplay.

Classification at filing time: NEW, CONFIRMED against current HEAD — `grep -rn "insert(.*Dead)"` across the whole tree returns exactly one site, and it is `#[cfg(test)]`.
