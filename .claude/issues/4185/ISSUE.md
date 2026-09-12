# CONC-D4-01: `footstep_system`'s #3652 stage placement is the only half of that fix with no regression pin

Labels: low,concurrency,test-gap,bug

**Description**: #3652 moved both `make_billboard_system` and `footstep_system` out of `Stage::PostUpdate` into the `Stage::Late` exclusive lane because both read the camera's `GlobalTransform`, authored by `camera_follow_system` in the Late parallel batch. `make_billboard_system` got a dedicated pin (`billboard_runs_after_camera_follow_in_late`) asserting both its stage/order and absence from every other stage. `footstep_system` got neither. Since `analyze_pair` is intra-stage only, a regression that moves `footstep_system` back to `PostUpdate` (or into Late's parallel batch) is invisible to every existing counter and test.

**Evidence**:
`byroredux/src/boot/schedule/late.rs` (confirmed): registers `footstep_system` via `add_exclusive_with_access` in `Stage::Late`; grepping the crate for a stage assertion on it returns nothing beyond unrelated resource-catalog/distance-accumulation tests.

**Impact**: No live defect — placement is currently correct. The exposure is that the *reason* (a comment) is enforced by prose only; a future stage reshuffle could reintroduce the one-frame-stale spatial-audio trigger position #3652 fixed, with a fully green test suite.

**Related**: #3652, #3180 (the `submersion_system` precedent, which *is* pinned), #848.

**Suggested Fix**: Extend `billboard_runs_after_camera_follow_in_late` (or add a sibling) to assert the same three properties for `footstep_system`.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
