# PHYS-D4-2026-09-21-01: The NPC KCC's pinned copies of ContactConfig / fallback-capsule values have no guard

**Issue**: #4689
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: LOW
**Dimension**: Character & NPC Controller
**Location**: `byroredux/src/systems/locomotion.rs:53-77`; sources: `crates/physics/src/config.rs:97-98`, `byroredux/src/npc_spawn.rs:359-377`, `CharacterController::HUMAN`

## Description
Each constant's doc says "matching" or "the invariant is inherited", but nothing checks that. `LOCOMOTION_NPC_CAPSULE_HALF_HEIGHT`/`RADIUS` (32/20), `STEP_HEIGHT`/`STEP_MIN_WIDTH`/`MAX_SLOPE_DEG`, and `KCC_OFFSET_BU` (4.0) are bare `pub(crate) const`s. The player reads the live `ContactConfig` resource; NPCs read the copies.

## Evidence
No `static_assertions`/equality test ties the constants to `ContactConfig::DEFAULT` or `CharacterController::HUMAN`. `kcc_offset_clears_the_combined_contact_skin` pins only the resource default.

## Impact
After a retune (#2885 already moved these once), walking NPCs keep the old numbers with CI green — the offset could drop back inside the combined contact skin (#2193's "blocked but permanently ungrounded" shape).

## Related
#2885, #2193, M42.10 (`913fd39d8`).

## Suggested Fix
Derive the NPC offset from `ContactConfig::DEFAULT`, share one const for the 32/20 capsule, or add an equality pin test.
