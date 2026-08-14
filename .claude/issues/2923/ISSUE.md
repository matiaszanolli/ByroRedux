# PERF-D1-01: Skinning path still SipHashed — pose_dirty's HashSet type is pinned into FrameInputs

- **Issue**: [#2923](https://github.com/matiaszanolli/ByroRedux/issues/2923)
- **Finding ID**: `PERF-D1-01`
- **Labels**: `low,performance,tech-debt,bug`
- **Source report**: [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](../../../docs/audits/AUDIT_PERFORMANCE_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2923 --json state`.

---

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/ecs/resources/skin_slot_pool.rs` — `SkinSlotPool` fields `entity_to_slot`, `last_seen_frame`, `last_pose_hash`, `pose_dirty`, `rollback_pose_hash`; `byroredux/src/main.rs` — the `App::skin_offsets` field; `byroredux/src/render/mod.rs` — the `skin_offsets` parameter of `build_render_data`; `byroredux/src/render/static_meshes.rs` — the `skin_offsets.get(&entity)` probe in `collect_static_mesh_draws`; `crates/renderer/src/vulkan/context/draw.rs` — the `pose_dirty` field of `FrameInputs`
- **Status**: NEW
- **Description**: The repo has twice decided that std's SipHash-1-3 default is the wrong
  hasher for maps probed once per draw or once per entity per frame, and twice fixed it —
  #1368 (per-draw `material_hash` + the descriptor dirty-gates, now `rustc_hash::FxHasher`
  / `FxHashMap`) and #2174 (`previous_rigid_models` / `current_rigid_models_scratch`, moved
  to `FxHashMap` with the guard test `rigid_motion_history_maps_are_not_siphash`). Both
  sweeps stopped at the `crates/renderer` boundary. The skinning path — which crosses that
  boundary — was never revisited, and every one of its per-frame maps is still a std
  `HashMap` / `HashSet`:

  Per frame, with `S` = live skinned entities and `D` = mesh entities surviving to the
  draw-emit block:
  - `skin_offsets.get(&entity)` in `collect_static_mesh_draws` fires for **every** mesh
    entity, not just skinned ones — `D` probes, of which `D − S` are misses.
  - `build_skinned_palettes` adds `S` inserts (Pass 1) + `S` probes (Pass 3) on the same map.
  - `SkinSlotPool::allocate` probes `entity_to_slot` and inserts into `last_seen_frame`
    once per skinned entity (`2S`).
  - `try_mark_pose_dirty` probes `last_pose_hash` once per skinned entity, plus an `entry`
    on `rollback_pose_hash` and an insert into `pose_dirty` for each dirty one.
  - `sweep` iterates the whole `last_seen_frame` map every frame.
  - On the renderer side, `record_skinned_blas_refit` calls `pose_dirty.contains(&entity_id)`
    per skinned BLAS entry — inside the crate that already imports `FxHashMap`, but forced
    back onto SipHash because `SkinSlotPool::pose_dirty()` returns
    `&std::collections::HashSet<EntityId>` and `FrameInputs.pose_dirty` pins that exact type
    in its public signature.

  Against the repo's own checked-in baselines that is roughly `D + 5S` SipHash probes per
  frame on this path alone: `2553 + 5×677` on `fnv-FreesideAtomicWrangler.tsv` and
  `3440 + 5×124` on `fo4-InstituteBioScience.tsv`. #2174 was landed on a materially
  smaller argument shape ("probed twice per rigid draw per frame") and rated the same LOW.
- **Evidence**:
  ```rust
  // crates/core/src/ecs/resources/skin_slot_pool.rs — all five, std default hasher
  entity_to_slot:     std::collections::HashMap<EntityId, u32>,
  last_seen_frame:    std::collections::HashMap<EntityId, u64>,
  last_pose_hash:     std::collections::HashMap<EntityId, u64>,
  pose_dirty:         std::collections::HashSet<EntityId>,
  rollback_pose_hash: std::collections::HashMap<EntityId, Option<u64>>,

  // and the accessor that pins the type across the crate boundary:
  pub fn pose_dirty(&self) -> &std::collections::HashSet<EntityId> { &self.pose_dirty }
  ```
  ```rust
  // byroredux/src/render/static_meshes.rs — inside collect_static_mesh_draws' per-entity loop
  let bone_offset = skin_offsets.get(&entity).copied().unwrap_or(0);
  ```
  ```rust
  // crates/renderer/src/vulkan/context/draw.rs — FrameInputs
  pub pose_dirty: &'a std::collections::HashSet<EntityId>,
  ```
  The precedent and the remedy are both already in tree:
  `crates/renderer/src/vulkan/context/mod.rs` declares
  `previous_rigid_models: FxHashMap<u32, [f32; 16]>` and pins it with
  `rigid_motion_history_maps_are_not_siphash`; `rustc-hash` is already a workspace
  dependency (`Cargo.toml`).
- **Impact**: Bounded and small — a constant-factor probe cost on the skinning and
  static-draw loops, no allocation change and no correctness effect. It matters because it
  is the last cluster of the pattern two closed issues removed everywhere else, and because
  the existing guard test only pins the two renderer maps, so nothing would flag a *third*
  std map being added here. **No quantitative guard exists for this site** — dhat covers
  only the NIF parse path, so this cannot be bounded by a test today, only counted.
- **Related**: #1368 (CLOSED, FxHash on the render hot path), #2174 (CLOSED, the identical
  fix for the rigid motion-history maps, with its guard test), #1195 / #1196 (the
  `pose_dirty` gate these maps implement), #1379 (the `next_slot` contraction in the same
  `sweep`).
- **Suggested Fix**: Move all five `SkinSlotPool` collections and `App::skin_offsets` to
  `rustc_hash::FxHashMap` / `FxHashSet` (adding the existing workspace `rustc-hash` dep to
  `crates/core` and `byroredux`), change `SkinSlotPool::pose_dirty()`'s return type so
  `FrameInputs.pose_dirty` can follow, and extend
  `rigid_motion_history_maps_are_not_siphash` — or add a sibling in
  `skin_slot_pool.rs` — to cover the new fields so the pattern stays pinned.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_PERFORMANCE_2026-08-14.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
