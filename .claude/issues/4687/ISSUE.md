# PHYS-D2-2026-09-21-02: Three edges of the new explosion recovery are uncovered

**Issue**: #4687
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: LOW
**Dimension**: Step & Sync
**Location**: `crates/physics/src/world.rs:223-265`, `:680-690`, `:768-787`, `:792-795`; `crates/physics/src/ragdoll.rs:278-293`; `byroredux/src/ragdoll.rs:325-330`; `crates/physics/src/sync.rs`

## Description
(a) A dynamic body already non-finite when a substep starts is filtered out of the snapshot and never sleeps (NaN `y` fails the kill-plane check too), so it stays active forever with the fast path permanently off. Seeds from `activate_ragdoll`/`build_ragdoll`/`register_newcomers` are never validated.
(b) Rapier defers collider sync on `set_position` to the next pipeline step; the repo never calls `propagate_modified_body_positions_to_colliders`, so the post-loop QBVH rebuild indexes restored bodies at their exploded/NaN pose for at least one frame.
(c) `restore_invalid_dynamic_bodies` only detaches the first invalid body's multibody per substep; a second simultaneously-invalidated articulation keeps stepping with a corrupt pose.

## Evidence
Code and rapier source as cited; re-read at HEAD confirms all three gaps unchanged.

## Impact
(a) permanently awake body, fast path off for good. (b) stale query geometry for ≥1 frame post-recovery. (c) a second error log and forfeited backlog per extra articulation. No bad pose reaches ECS; save state uncorrupted.

## Related
PHYS-D3-2026-09-21-01 (#4683); #2337.

## Suggested Fix
Sleep/zero/log dynamic bodies already non-finite before the step (or reject non-finite seeds); call `propagate_modified_body_positions_to_colliders` after a restore; collect every distinct multibody instead of the first.
