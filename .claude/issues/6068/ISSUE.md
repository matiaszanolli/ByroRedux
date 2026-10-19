# #6068 (snapshot as filed)

Split from #5015.

**Split from #5015** (Skyrim half; the humanoid/draugr ragdolls are posed, `18b3d87a7`, and the trailing-marker family is set aside, `2139ae20e`).

## Description
Skyrim corpses on `SkeletonS.nif` (race `SkeletonRace`, the skeleton-pile corpses) are refused by the pose gate: that rig has **19 ragdoll bodies**, not the humanoid 18, so `is_humanoid_ragdoll` fails and they collapse from their spawn pose.

## Evidence (Skyrim.esm, `xrgd_dump` + `xrgd_entry_counts_match_the_humanoid_ragdoll_bodies`)
`meshes\actors\draugr\character assets\skeletons.nif`: 120 corpses (`TreasCorpseSkeleton` 110), entry counts 18/19/20, bodies 19; 53 of 120 entry-count == body-count after the marker family. Trailing entries seen: `(0, 32.45, 104.06)` ×33, `(0, 28.71, 108.18)`, and 20-entry poses carrying a real extra entry (bone id 7, nonzero rotation) before the marker. The first 18 entries use the humanoid bone-id order.

## Approach
Identify the 19th body (name, entry position/order) from the NIF's ragdoll bodies and the 20-entry poses, then verify with the floor-contact and offset-length checks used for the humanoid rig (`scripts/xrgd-analysis.py`). Gate by body-name set as for the other rigs.

## Related
#5015, `byroredux/src/ragdoll_pose.rs`.

## Completeness Checks
- [ ] **SIBLING**: RigidSkeletonRace (Draugr `skeleton.nif`, 18 bodies) is already posed — confirm no regression
- [ ] **TESTS**: body-set test and census pin for the 19-body rig
