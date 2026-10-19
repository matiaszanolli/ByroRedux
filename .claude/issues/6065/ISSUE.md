# #6065 (snapshot as filed)

Split from #5015.

**Split from #5015** (FO4 half; the human and feral-ghoul rigs landed in `e1bd40bcf`).

## Description
`ragdoll_pose::pose_skeleton` poses an FO4 corpse only when its ragdoll's host bones are exactly the verified human or feral-ghoul set (`FO4_HUMAN_RAGDOLL_BODIES`, `FO4_GHOUL_RAGDOLL_BODIES`). Every other FO4 actor with an authored `XRGD` pose keeps collapsing from its spawn pose.

## Evidence (Fallout4.esm, `xrgd_dump`)
1,086 Starts-Dead ACHRs carry an `XRGD` pose; 987 are on the human/ghoul rigs (972 resolve). The other ~99 by race: BrahminRace 13 (39–42 entries), MoleratRace 12 (20–21), ViciousDogRace 12 (24), ProtectronRace 9, BloatflyRace 8, SuperMutantRace 7 (17), RadStagRace 6 (31), CatRace 6, HandyRace 5, RadRoachRace 5, YaoGuaiRace 5, MirelurkRace 3, StingwingRace 3, a few turrets/assaultrons/power armor.

Also refused within the verified rigs (15 of 987): feral ghouls with 20 entries (two unmatched trailing entries) and a few poses that omit a body.

## What carries over
The bone-id mapping (an entry claims the first unclaimed body whose per-body bone id equals the entry's `bone_id`; `entries_by_bone_id`) is skeleton-independent. What is **not** established per skeleton: that entry rotations are parent-local in the NIF bone frames with the same Euler mode, and what the extra entries are. Do not widen the name gate on an id-multiset match alone.

## Suggested approach
Per skeleton, repeat the two checks that validated human and ghoul (`scripts/xrgd-fo4-analysis.py`): (1) entry offset lengths equal body-to-parent distances; (2) forward kinematics on the real NIF bone tree ranks intrinsic-XYZ-negated first by lying-body extent. Add each verified rig's body set to the gate. Characterise the 20-entry ghoul poses first (largest cheap win).

## Related
#5015 (closed), `docs/engine/physal.md` (FO4 ragdolls), `byroredux/src/ragdoll_pose.rs`.

## Completeness Checks
- [ ] **SIBLING**: each newly gated rig verified with both checks, not by entry count
- [ ] **TESTS**: a body-set / mapping unit test per rig, and the `fo4_xrgd_poses` census extended to it
