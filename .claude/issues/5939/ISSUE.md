# PHYS-D3-2026-10-16-02: `physal.md` / `physics.md` still describe ragdoll activation as the console trigger only and say nothing of the XRGD pose or the death-reconciliation path

Labels: low,physics,documentation,doc-rot
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `docs/engine/physal.md:213-219` ("On the `ragdoll <id>` console trigger, `activate_ragdoll` reads each bone's current `GlobalTransform`..."), `docs/engine/physics.md` (no mention)
- **Status**: NEW
- **Description**: Production activation is `combat.rs::reconcile_dead_actor` (live kills, Starts-Dead actors, save-load sweep); the console `ragdoll` command is the debug path. Since `18b3d87a7` that path first poses a Skyrim humanoid corpse from its XRGD via `ragdoll_pose.rs`, a fourth data interpretation (entry k = body k, Gamebryo XYZ negated-angle Euler, COM at placement) that is neither one of the three PHYSAL §3 seams nor described anywhere under `docs/engine/` (`grep -i "xrgd\|ragdoll_pose\|authored corpse" docs/engine/*.md` is empty).
- **Evidence**: `docs/engine/physal.md:218`; `byroredux/src/combat.rs:1040-1063`; `byroredux/src/ragdoll_pose.rs:10-50` (module docs hold the only description).
- **Impact**: The "ground truth" doc used by this audit under-describes where per-skeleton knowledge lives, so a future audit could either miss the fifth entry point for ragdoll seeding or mis-flag the bone-name table as a seam violation.
- **Related**: #5701 (previous physal.md sync, closed), #5015
- **Suggested Fix**: Add a short "Authored corpse pose (#5015)" paragraph to physal.md's Translate section (death-reconciliation activation, the humanoid-only pin, refusal -> collapse, not saved) and note that it is skeleton-keyed data, not a `GameKind` branch.



Severity summary: 0 CRITICAL / 0 HIGH / 0 MEDIUM / 3 LOW (defence-in-depth and cosmetic). No rule in the skill text regressed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
