# PERF-D1-01: Skinned-leaf world-bound refold is the one ungated pass

**Issue**: #2677
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: MEDIUM
- **Dimension**: 1 — CPU Hot Paths
- **Location**: [bounds.rs](byroredux/src/systems/bounds.rs) — `make_world_bound_propagation_system` (skinned block, ~lines 189-203) + `skinned_world_bound` (~lines 31-48)
- **Status**: NEW
- **Description**: `make_world_bound_propagation_system` is otherwise a model of
  incremental design: it drains `GlobalTransform`'s dirty set into a persistent
  `g_dirty` (#1371), early-returns when `g_dirty.is_empty() && !structural_changed`,
  drives Pass 1 off `g_dirty`, and drives Pass 2 off a `dirty_roots` set walked up
  from `g_dirty`. The skinned-leaf block between them has **no per-entity dirty
  check at all** — it iterates *every* `SkinnedMesh` entity and recomputes the full
  bone-palette-enclosing sphere whenever *anything* in the world moved. Because the
  camera's own `Transform` mutation propagates a `GlobalTransform` write (and
  `make_billboard_system` writes GT for every billboard on camera motion), `g_dirty`
  is non-empty on essentially every frame the player moves — so this block runs
  every frame regardless of whether any *bone* moved.
- **Evidence**: The block is bracketed by neither `g_dirty` nor `structural_changed`:
  ```rust
  if let Some(ref sq) = skin_q {
      for (entity, skin) in sq.iter() {
          let Some(local) = lb_q.get(entity) else { continue };
          let bound = skinned_world_bound(local, skin, |bone| {
              g_q.get(bone).map(GlobalTransform::to_matrix)
          });
          ...
      }
  }
  ```
  and `skinned_world_bound` is per-bone `Mat4 × Mat4` plus `transform_sphere`'s
  three `Vec3::length()` square roots:
  ```rust
  for (bone, bind_inverse) in skin.bones.iter().zip(&skin.bind_inverses) {
      let palette = bone.and_then(&mut bone_world)
          .map(|world| world * *bind_inverse).unwrap_or(Mat4::IDENTITY);
      merged = merged.merge(&transform_sphere(local, palette));
  }
  ```
  Scale is not hypothetical: the repo's own checked-in runtime baseline
  `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` records
  `skin_pool_live 677` (vs `skin_pool_max 1364`) — 677 live `SkinnedMesh` entities
  in one FNV interior. `skyrim_se-WhiterunDragonsreach.tsv` records 83 and
  `fo4-InstituteBioScience.tsv` 124. Every one of those entities' full bone list is
  re-walked, per frame, with a matrix multiply and three square roots per bone.
  Secondly, the same frame's `render::skinned::build_skinned_palettes` performs the
  *identical* `gt.to_matrix()` conversion for every bone of every skinned entity
  into `bone_world` — the bone→matrix conversion is done twice per bone per frame by
  two subsystems that never share the result.
- **Impact**: Unbounded-by-dirty-state CPU cost proportional to
  `skin_pool_live × bones_per_skin`, paid on the PostUpdate stage of every frame in
  which anything moved. On the 677-skin FNV baseline this is the largest single
  un-gated per-frame loop found in Dimension 1. The work is *not* redundant for
  actively animating actors (their bones genuinely move), so the win is confined to
  idle/asleep skinned entities and camera-only-motion frames — but that is exactly
  the steady state the rest of this system was rewritten to exploit. On a 7950X
  this is the class of cost the audit charter calls a bug rather than a tuning gap.
  **No quantitative guard exists for this site** — there is no dhat bound and no
  runtime-baseline scalar that would catch it growing.
- **Related**: #1371 (`drain_dirty_into`, intact), #1195 / PERF-DIM7-01
  (`SkinSlotPool::try_mark_pose_dirty` — an already-computed per-entity
  "bones changed?" signal, but produced later in the same frame by
  `build_skinned_palettes`, so not usable as-is without a one-frame lag).
  Adjacent to but distinct from #1794.
- **Suggested Fix**: Gate the skinned block per entity: `g_dirty` is already
  `sort_unstable`'d + `dedup`'d in the incremental branch, so a `binary_search` of
  each `skin.bones` entry against it (plus the mesh entity itself) skips the whole
  matrix/sqrt path for clean skins at a fraction of the cost. Do the same sort in
  the `structural_changed` branch so the gate is uniform. Longer term, consider
  publishing `build_skinned_palettes`' per-bone world matrices as a frame resource
  so bounds and the palette pass stop computing `to_matrix()` twice per bone.
  Quantify with a targeted micro-bench before and after — no existing harness covers
  this loop.

---


---
*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `PERF-D1-01`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or a bench delta vs the checked-in baseline) pins this fix
