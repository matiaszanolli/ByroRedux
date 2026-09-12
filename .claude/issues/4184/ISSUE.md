# CONC-D3-02: `SubtreeCache`'s position in the animation lock order lives only in `systems/animation.rs` comments, not in the canonical-order doc

Labels: low,concurrency,doc-rot,documentation

**Description**: `animation_system_inner` holds a strictly longer chain than documented: `AnimationClipRegistry -> NameIndex -> SubtreeCache -> {AnimationPlayer | AnimationStack | Transform | AnimationTextKeyEvents | RootMotionDelta | Animated*}`, with `SubtreeCache`'s miss path additionally recording `NameIndex -> Children`/`Name`. `docs/engine/ecs.md` never names `SubtreeCache`. Currently there is exactly one consumer (`systems/animation.rs`) and no contradictory direction exists anywhere, so nothing is broken today.

**Evidence**:
`grep` for `SubtreeCache` resource access returns hits only in `systems/animation.rs`; the internals are individually correct (miss path drops its read before the write, no same-type panic risk; `Children`-before-`Name` matches the canonical tail).

**Impact**: Documentation-completeness only. The risk is a future second consumer (e.g. NPC/facial-animation or a debug-inspection path) re-deriving the direction from scratch and picking `SubtreeCache` under `Transform`/`AnimationStack`, closing a cycle against this system's every-frame edges — exactly the class #3651 was filed to close for three other clusters.

**Related**: #3651, #2400, #824/#827 (NameIndex-before-Name), #2924.

**Suggested Fix**: Extend the animation cluster line in `docs/engine/ecs.md` to `AnimationClipRegistry -> NameIndex -> SubtreeCache -> AnimationPlayer/AnimationStack -> Transform`, noting the `Children -> Name` miss-path tail. Pure doc change.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
