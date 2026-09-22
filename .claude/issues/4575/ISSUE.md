# ECS-2026-09-21-D2-01: `audit-ecs/SKILL.md` drift found while running it

**Labels**: low, documentation, tech-debt, doc-rot

Filed via /audit-publish from docs/audits/AUDIT_ECS_2026-09-21.md.

**Severity**: LOW (audit tooling) · **Dimension**: 2 / 5 (skill text)
**Location**: `.claude/commands/audit-ecs/SKILL.md`: the Dim 2 "Change tracking" bullet (~:68) and the Dim 5 "What the guard cannot see" paragraph (~:161)
**Verified against**: HEAD `f97775ca8`

## Description

Two statements in the audit-ecs skill no longer match the code:

1. **Dim 2, "Change tracking"**, says `TRACK_CHANGES` is on for `Transform`, `GlobalTransform`, `Parent` and `Children`. It is also on for three more components. All three use sparse storage, so they get only a `structural_generation` bump and no dirty set:
   - `LocalBound` (`crates/core/src/ecs/components/local_bound.rs`), since `ad012f9d6` (2026-05-31);
   - `Material` (`crates/core/src/ecs/components/material.rs`) and `ParticleEmitter` (`crates/core/src/ecs/components/particle.rs`), both since `1d56758ba` (#3836, 2026-09-05).
2. **Dim 5, "What the guard cannot see"**, lists the forms gap: `world.get` / `get_mut` / `has`, `query_2_mut`, `resource_2_mut`, acquisitions without a turbofish, cross-file hops, closures and macros. It leaves out the three blind spots found in ECS-2026-09-21-D5-01 (#4573):
   - read vs write is never compared;
   - type names are matched by substring;
   - comment text inside the registration block satisfies the check.

## Evidence

`grep -rn 'const TRACK_CHANGES: bool = true' crates/core/src/ecs/components/` finds seven components: `transform`, `global_transform`, `hierarchy` ×2 (`Parent`, `Children`), `local_bound`, `particle` and `material`. The skill names four of them.

## Impact

- An auditor following Dim 2 skips the change-tracking contract of three components that feed incremental paths: #3836's `SceneEffectSoftCache` and the incremental world-bound propagation.
- An auditor following Dim 5 trusts the completeness guard for exactly the slips it cannot catch.

## Related

- ECS-2026-09-21-D5-01 (#4573).
- Earlier audit-ecs skill drift, all closed: #4065, #4174, #3035.
- `_audit-validate.sh` reports no stale path and no advisory against this skill. This drift is semantic, which the path gate cannot catch.

## Suggested Fix

1. In Dim 2, list all seven `TRACK_CHANGES` components, noting which are packed (dirty set) and which are sparse (generation bump only). Better still, replace the list with the grep so it cannot rot.
2. In Dim 5, add the three D5-01 blind spots. If D5-01 is fixed first, record them as closed instead.
3. Run `.claude/commands/_audit-validate.sh` after the edit.

Source: docs/audits/AUDIT_ECS_2026-09-21.md (ECS-2026-09-21-D2-01)

## Completeness Checks
- [ ] **SIBLING**: Other skills and `docs/engine/` pages that list `TRACK_CHANGES` components or describe the declaration-completeness guard are updated in the same pass. At filing, a grep found no other list of tracked components.
