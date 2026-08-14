# PERF-D1-02: animation_system_inner takes the Transform write query twice per animated entity per frame

- **Issue**: [#2924](https://github.com/matiaszanolli/ByroRedux/issues/2924)
- **Finding ID**: `PERF-D1-02`
- **Labels**: `low,performance,ecs,bug`
- **Source report**: [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](../../../docs/audits/AUDIT_PERFORMANCE_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2924 --json state`.

---

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/animation.rs` — `animation_system_inner`, the Phase-2 per-playback-entity body: the transform-channel block, `write_root_motion`, and the accum-root grounding block that follows it
- **Status**: NEW
- **Description**: CLOSED #53 restructured this system specifically so that per-entity
  component queries are *acquired once and held for the whole batch* ("float/color/visibility
  channel queries now held for entire channel batch per entity instead of re-acquired per
  channel"). The accum-root grounding block — added later, for the Gamebryo accum/non-accum
  model — re-acquires `world.query_mut::<Transform>()` **after** the batch guard has already
  been dropped, so the common path now takes the `Transform` write lock twice per animated
  entity per frame. The only statement between the two acquisitions is
  `write_root_motion(world, entity, root_motion)`, which itself early-returns on
  `motion == Vec3::ZERO` — the ordinary case for a non-locomoting actor — and so is usually
  a no-op function call separating two acquisitions of the same lock.

  The second acquire fires on the **common** branch, not a rare one: the block is gated on
  `!accum_root_animated`, and the code's own comment states the reason it exists is that
  "most idle clips animate only `Bip01 NonAccum` … and leave the accum root untouched".

  Each ECS query acquisition is not free even in release builds: `World::query_mut` does a
  `TypeId` probe into `storages`, and `lock_tracker::track_write` / `untrack_write` do two
  more `TypeId` probes each into a thread-local `HashMap` (all std default hasher), around
  the real `RwLock`. So this is ~5 `TypeId` hash probes plus a write-lock round-trip per
  animated entity per frame, purchased for a single `Vec3::ZERO` store.

  A secondary instance of the same shape sits immediately above it: `ensure_subtree_cache`
  acquires and drops `SubtreeCache` via `try_resource`, and the very next statement acquires
  `SubtreeCache` again for the `scoped_map` lookup — two resource acquisitions per entity per
  frame where the first could hand its guard forward.
- **Evidence**: `animation_system_inner`, Phase 2, per playback entity:
  ```rust
  let mut transform_query = world.query_mut::<Transform>().unwrap();   // acquire #1
  for (channel_name, channel) in &clip.channels { /* … */ }
  drop(transform_query);

  write_root_motion(world, entity, root_motion);   // early-returns when motion == Vec3::ZERO

  if !accum_root_animated {                        // the common branch, per the code's comment
      if let Some(accum_entity) = clip.accum_root_name.as_ref().and_then(&resolve_entity) {
          if let Some(mut tq) = world.query_mut::<Transform>() {       // acquire #2
              if let Some(t) = tq.get_mut(accum_entity) { t.translation = Vec3::ZERO; }
  ```
  and, a few lines earlier in the same iteration:
  ```rust
  ensure_subtree_cache(world, root);                       // acquires+drops SubtreeCache
  let subtree_ref = world.try_resource::<SubtreeCache>();  // acquires SubtreeCache again
  ```
- **Impact**: One extra `RwLock` write acquisition plus ~5 `TypeId` hash probes per animated
  entity per frame, scaling with actor count (`skin_pool_live` is 677 on the checked-in
  `fnv-FreesideAtomicWrangler.tsv` baseline, 124 on `fo4-InstituteBioScience.tsv`). No
  correctness effect and no allocation. Reported at LOW because the magnitude is small; it is
  reported at all because it is a measurable *erosion of a landed invariant* — #53's whole
  point was one guard per entity per component — rather than a new proposal, and because
  taking the same write lock twice inside one loop iteration is also a wider surface for the
  ABBA hazards the surrounding comments (#313 / #827 / #1410) go to some length to avoid.
  **No quantitative guard exists for this site.**
- **Related**: #53 (CLOSED — the per-entity batching this erodes; note its landed shape
  *does* accept one acquisition per entity, which is why only the duplicate is reported),
  #271 / #287 (CLOSED — the same consolidation for the `AnimationStack` path),
  #2400 (OPEN — `animation_system_inner` holding `AnimationClipRegistry` + `NameIndex` read
  guards across every component acquisition; same function, concurrency angle, distinct
  issue), #1372 / #1725 (the `AnimScratch` guards, intact).
- **Suggested Fix**: Move the accum-root grounding into the still-live `transform_query`
  guard and run `write_root_motion` after that block closes — a pure reordering that
  introduces no new held-while-acquiring lock edge, since `RootMotionDelta` would then be
  taken with nothing else held. Separately, have `ensure_subtree_cache` return the
  `SubtreeCache` read guard (or fold the "build if missing" step into the existing read) so
  the resource is acquired once per entity rather than twice.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
