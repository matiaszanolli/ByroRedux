# #6067 (snapshot as filed)

Split from #5015.

**Split from #5015** (FO4 ragdoll decode, `e1bd40bcf`).

## Description
`decode_np_ragdoll` reads `hknpCapsuleShape` and `hknpSphereShape` bodies and `hkpRagdollConstraintData` / `hkpLimitedHingeConstraintData` joints. Everything else is dropped, together with the joints that touch a dropped body, so some FO4 rigs build a partial or no ragdoll.

## Evidence (75 vanilla ragdoll blobs, 986 bodies, 847 joints)
- 64 bodies have no readable shape: `hknpConvexPolytopeShape` (turret workshop/standing skeletons: 6 and 8 hulls), `hknpDynamicCompoundShape` (radstag, turret workshop), and others on `powerarmor`, `mirelurkhunter`, `mirelurkking`, `vertibird`, `libertyprime`.
- 3 joints are `hknpBreakableConstraintData` wrappers (`turretstandingskeleton`), counted as `skipped_constraints`.
- 64 of 847 joints do not meet at one world point at the stored bind pose (turrets, vertibird, `libertyprime_furniture`, a few robot parts, 4 on the deathclaw, 1 on the mirelurk queen): the same files where bodies sit off their bones. Whether that is authored rest-pose difference or a decode gap for those classes is not established, and they are not in the `EXACT` list of `crates/nif/tests/np_ragdoll_corpus.rs`.

## Suggested approach
Decode the polytope and compound shape classes from the corpus the same way the capsule was (radius/vertex fields, pinned by mass = volume x density); unwrap the breakable wrapper to its inner constraint data. Then characterise the pivot-disagreement skeletons (deathclaw first: a character rig) before trusting their ragdolls.

## Related
#5015, `docs/engine/physal.md` (precombine `hknpCompressedMeshShapeData` is a separate gap).

## Completeness Checks
- [ ] **SIBLING**: shape resolution parity (`resolve_shape` vs the NP path) for any new class
- [ ] **TESTS**: synthetic decode test per new class; corpus count of dropped bodies/joints pinned
