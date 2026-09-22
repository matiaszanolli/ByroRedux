# PHYS-META-2026-09-21-01: audit-physics/SKILL.md drift found while running it

**Issue**: #4692
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: LOW
**Dimension**: Queries & Diagnostics (audit infrastructure; cross-dimension)
**Location**: `.claude/commands/audit-physics/SKILL.md` — known-open register, Dim 1 guards, Dim 2 recovery bullet, Dim 3 last bullet, Dim 4 NPC/player bullets, Dim 5 samplers bullet, Dim 6 cost bullet

## Description
Eight statements in the skill are stale, re-verified at HEAD `ee6d3fb39`:
1. #4407 listed as open; it is CLOSED (`726a2e930`).
2. `non_finite_cuboid_extent_clamps_instead_of_reaching_rapier` listed as default-lane; it is release-only (PHYS-D1-2026-09-21-01, #4686).
3. "Multibody joints are restored with their bodies": by design the recovery detaches them.
4. "Writeback drives `Transform`": it writes `GlobalTransform`.
5. `snap_character_body_to_camera` "takes `&mut World`": it takes `&World`.
6. "NPCs shove clutter": nothing pushes clutter (PHYS-D4-2026-09-21-02, #4690).
7. "Plane flow wins over the marker in both": the two samplers disagree (PHYS-D5-2026-09-21-01, #4691).
8. "Budget from the rebuild": the rebuild is avoidable (PHYS-D6-2026-09-21-02, #4685); the snapshot walk adds its own per-substep cost (PHYS-D2-2026-09-21-01, #4682).

## Evidence
Each item verified at `ee6d3fb39` at the cited lines.

## Impact
The next `/audit-physics` run would start from wrong premises on three live mechanisms (recovery, NPC sweep, sampler parity).

## Related
ECS-2026-09-21-D2-01 (#4575), NIF-D3-2026-09-21-04 (#4630) — same audit-skill-drift class.

## Suggested Fix
Update the skill and run `.claude/commands/_audit-validate.sh`.
