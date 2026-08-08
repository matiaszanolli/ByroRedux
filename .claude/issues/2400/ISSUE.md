# #2400 — CONC-D3-2026-08-07-02: `animation_system_inner` holds `AnimationClipRegistry` + `NameIndex` read guards across every component acquisition in the system, undocumented as a lock-order constraint

- **Severity**: LOW
- **Domain**: sync, ecs
- **Audit**: `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2400


- **Severity**: LOW
- **Dimension**: 3 — ECS Lock Ordering
- **Location**: `byroredux/src/systems/animation.rs:386-833` (guards taken at `:386` and `:454`)
- **Status**: NEW

**Description**

`registry` and `name_index` are bound to function-scope locals living until `animation_system_inner` returns; every subsequent acquisition in the body (`Name`, `SubtreeCache`, `AnimationPlayer`, `AnimationTextKeyEvents`, `Transform`, `RootMotionDelta`, `AnimationStack`, all eleven animated-channel sinks) happens underneath both — the widest hold-stack in the engine (~15 distinct types deep) — with no comment stating "nothing may acquire `AnimationClipRegistry` or `NameIndex` while holding any animation component storage," unlike the carefully documented `NameIndex`-before-`Name` rule a few lines away. This system is registered in a **parallel** lane (`boot.rs:748`, `Stage::Update`), so the constraint is not backstopped by exclusive scheduling — only by the current fact that it's alone in that lane.

**Evidence** (re-confirmed at publish time against commit `79bfc76e`): `let Some(registry) = world.try_resource::<AnimationClipRegistry>() else { return; };` (`:386`) and `let name_index = world.try_resource::<NameIndex>().unwrap();` (`:454`), no `drop()` anywhere in the function; `registry` still borrowed at `:652`/`:693`, `name_index` at `:540` (inside the Phase 2 loop that takes `Transform`/sink write guards).

**Impact**

No live deadlock today. Any future system that reads a clip registry or the name index while already holding `Transform` closes an ABBA cycle against a system that already runs on a rayon worker — the configuration the `add_exclusive` argument in #2153/#2126 does not cover.

**Related**: #2153 (`CHARAL-D3-01`, same class, exclusive-scheduled), #2154 (closed, same class), #2126 (established the doc-comment convention), #827/#824 (the `NameIndex`/`Name` rule this system already documents), CONC-D3-2026-08-07-01.

**Suggested Fix**: Add the same hold-stack comment style the `NameIndex`-before-`Name` block carries, naming these two as the outermost locks and stating they must never be acquired beneath an animation component storage; or narrow `name_index`'s live range to the Phase 2 loop.

## Completeness Checks
- [ ] **LOCK_ORDER**: If narrowing `name_index`'s live range is chosen over documentation, verify no downstream use in the function relies on it staying borrowed past Phase 2
- [ ] **SIBLING**: Check other systems with a resource-then-many-components acquisition shape for the same undocumented outermost-lock convention
- [ ] **TESTS**: N/A beyond the existing `is_clean()`/lock-tracker coverage — this is a documentation/defense-in-depth fix, not a behavior change

---
Filed from `docs/audits/AUDIT_CONCURRENCY_2026-08-07.md` via `/audit-publish`.
