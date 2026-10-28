# RT-5: `p5-f5-f9-quicksave fo3` red: after F9 the body settles 1.63 units below the quicksaved pose

Labels: medium, save-load, bug, game:fo3
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Finding**: RT-5 · **Severity**: MEDIUM · **Dimension**: Gate matrix / save-load (owner `/audit-save`)
**Location**: gate `docs/smoke-tests/p5-f5-f9-quicksave.sh:184-186` (`pose_close ... 1`); restore `byroredux/src/save_io/live_load.rs`
**Game / Cell**: fo3 / MegatonMoriartysSaloon (fixture)
**Note**: runtime-measured by the audit (engine launched headless); not re-executed by the prep pass. FNV passes the same gate, including graceful quit exit 0.

## Description
After F9 the body settles 1.63 units below the quicksaved pose. The pose restore itself is exact (`save load: restored player pose at (1287.2, -62.4, 43.6)`); the body then settles. Everywhere else the gate measured, the saloon floor is at y = -64.01 (pose0 at x 1110, move2 at x 991). So at save time the body stood 1.63 units up on something that is not under it after the reload. That fits a clutter or dynamic collider whose save-time position is not restored. The supporting object is unidentified.

## Evidence
- quicksaved `poseA` body=(1287.17, **-62.38**, 43.65) grounded=true; restored body=(1287.17, **-64.01**, 43.65) grounded=true.
- `session.stderr`: `applied 528 saved deltas across 459 form-id-matched entities`.
- Files in the audit's gate temp dir: `poseA.status`, `pose.restored.status`, `pose0.status`, `pose.move2.status`.

## Impact
A declared FO3 gate is red. If the cause is unsaved dynamic-object placement, a quickload does not fully restore the world the player saved in.

## Related
#5155 (closed, P5 soak grounding); #6237 (RigidBodyData removed from the save registry).

## Suggested Fix
At the save pose, run a `player.status` / physics probe of the support collider, and check whether its entity carries a saved transform delta.

## Completeness Checks
- [ ] **SIBLING**: Same quicksave gate re-run on FNV and any other declared profile after the fix
- [ ] **TESTS**: `p5-f5-f9-quicksave.sh fo3` passes; a save-delta test pins the support object's restore

