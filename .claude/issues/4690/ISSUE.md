# PHYS-D4-2026-09-21-02: The documented M42.10 sweep contract ("own bones masked", "shove clutter") is not what the query does

**Issue**: #4690
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: LOW
**Dimension**: Character & NPC Controller
**Location**: `crates/physics/src/world.rs:101-111`, `:862-871`, `:1306-1366`; `byroredux/src/systems/locomotion.rs:100-118`

## Description
All actors' bones are masked, not just the walker's own — `ACTOR_BONE_GROUP` is a group mask, not an ownership filter, so every live actor's bones and every FO4 fallback capsule are invisible to every walker. `move_character` never calls `solve_character_collision_impulses` (no push), and autostep's `include_dynamic_bodies: false` refuses a dynamic "stair", so a walker is blocked by clutter but can neither push it nor step over it.

## Evidence
Re-verified at HEAD: no `solve_character_collision_impulses` call in `crates/physics/src/world.rs`; `include_dynamic_bodies: false` at `world.rs:1313`. The `actor_move_interaction_groups` doc still claims "blocked by walls and shove clutter".

## Impact
Walking NPCs pass through each other and FO4 shape-less actors. A dynamic floor item tall enough (~7-9 BU) blocks a walker outright. Wander/Patrol recover via re-pick; Travel/Follow/Escort/Guard have none. Gameplay-visible consequence routed to `/audit-gameplay`.

## Related
#2873, M42.10 (`913fd39d8`).

## Suggested Fix
Fix the three doc sites. If NPC-vs-NPC blocking is wanted, exclude only the walker's own bones via a `QueryFilter` keyed on `ActorColliderOwner`. Decide whether walkers should apply character-collision impulses to dynamic bodies.
