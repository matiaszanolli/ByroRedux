# PHYS-D3-2026-10-27-02: `parks_authored_pose` adds a `GameKind` branch on the ragdoll side; `physal.md` section 3 still calls the XRGD path skeleton-keyed

Labels: low, physics, documentation, doc-rot
Source: docs/audits/AUDIT_PHYSICS_2026-10-27.md

# PHYS-D3-2026-10-27-02: `parks_authored_pose` adds a `GameKind` branch on the ragdoll side; `physal.md` section 3 still calls the XRGD path skeleton-keyed

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-27.md` finding `PHYS-D3-2026-10-27-02` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `byroredux/src/ragdoll_pose.rs` (`parks_authored_pose`, ~:571-578); caller `byroredux/src/cell_loader/references/mod.rs` (~:869-874); doc `docs/engine/physal.md` (~:225-232).

## Description
`parks_authored_pose(placement_starts_dead, base_dead, game, pose)` parks a pose for `placement_starts_dead || (base_dead && game == GameKind::Fallout3NV)` (107cc1d79, #6252). The base flag is already translated per game at the record boundary (`base_actor_starts_dead` in `crates/plugin/src/esm/records/dispatch_actor.rs`, plus #5005's FO3/FNV DATA-health rule), so the runtime branch exists only to keep Oblivion's dead-base poses out because Oblivion's rig is unverified. Rig verification is otherwise skeleton-keyed (`is_skyrim_posable_ragdoll`, `is_fo4_posable_ragdoll`, `is_fallout_posable_ragdoll`). The baseline audit recorded `ragdoll_pose.rs` as "data, not `GameKind`", and `physal.md` section 3 still says so; its mapping sentence ("entry k maps to body k in the 18-body humanoid set ... entry zero places the COM at the reference") is also stale: FO4 maps by `pose_bone_id`, and FO3/FNV place the root body at the skeleton root's position in its parent frame (`placement_local`).

## Evidence
`ragdoll_pose.rs:577` `!pose.is_empty() && (placement_starts_dead || (base_dead && game == GameKind::Fallout3NV))`; `physal.md:227` "This mapping is skeleton-keyed, not a `GameKind` branch".

## Impact
No wrong behaviour today. Doctrine drift: the next rig admission (Oblivion) has to edit a game switch in the runtime instead of adding a verified body set. Whether Oblivion's body names would pass `is_fallout_posable_ragdoll` was not checked; if they do, this branch is the only thing keeping Oblivion out and that should be written down.

## Related
#6252, #5013, #5005.

## Suggested Fix
Either carry the decision on the record ("a base-dead flag marks a posable corpse", set where `base_actor_starts_dead` / the DATA rule run), or replace the game check with a rig gate (keep the base-dead path and let `pose_skeleton` refuse unverified rigs, as it already does). Then update `physal.md` section 3's XRGD paragraph to name the per-rig maps and the admission rule.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites

