# Performance Audit — Dimension 4: ECS Query Patterns
Date: 2026-05-04
Depth: deep

## Per-Frame System Inventory

Source: `byroredux/src/main.rs:270-310` (Scheduler build).

| Stage | System | Queries (approx) | Resources | Notes |
|---|---|---|---|---|
| Early | `fly_camera_system` | `Transform` (W) | `ActiveCamera`, `InputState`, `RapierHandles`, `PhysicsWorld` | Single entity write, cheap |
| Early | `weather_system` | none directly (resources only) | `GameTimeRes`, `WeatherTransitionRes`, `WeatherDataRes`, `SkyParamsRes`, `CloudSimState`, `CellLightingRes` | Resource mutate only |
| Early | `timer_tick_system` (scripting) | `ScriptTimer` (W) | `DeltaTime` | |
| Update | `animation_system` | `Name`, `AnimationPlayer`, `AnimationStack`, `Transform`, `RootMotionDelta`, `AnimatedVisibility`, `AnimatedAlpha`, `AnimatedUvTransform`, `AnimatedShaderFloat`, `AnimatedMorphWeights`, `AnimatedDiffuseColor`, `AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor`, `AnimationTextKeyEvents` | `AnimationClipRegistry`, `SubtreeCache`, `NameIndex`, `DeltaTime` | Heavy: see findings |
| Update | `spin_system` | `Spinning` (R), `Transform` (W) | `DeltaTime` | Cheap |
| PostUpdate | `make_transform_propagation_system()` | `Transform`, `Parent`, `Children` (R), `GlobalTransform` (W) | none | BFS, scratch reused |
| PostUpdate | `particle_system` (exclusive) | `GlobalTransform` (R), `ParticleEmitter` (W) | `TotalTime` | uses `query_2_mut` |
| PostUpdate | `billboard_system` (exclusive) | `Billboard` (R), `GlobalTransform` (W) | `ActiveCamera` | re-acquires `GlobalTransform` twice |
| PostUpdate | `make_world_bound_propagation_system()` (excl) | `LocalBound`, `Children`, `Parent`, `GlobalTransform` (R), `WorldBound` (W) | none | post-order DFS, scratch reused |
| Physics | `physics_sync_system` | (rapier internals) | `PhysicsWorld`, `RapierHandles`, `Transform` | |
| Late | `log_stats_system` | none directly | `TotalTime`, `DeltaTime`, `DebugStats` | |
| Late | `event_cleanup_system` (excl) | `ActivateEvent`, `HitEvent`, `TimerExpired` | none | end-of-frame markers |
| Late | `DebugDrainSystem` (debug-server feature) | varied (operator-driven) | many | feature-gated |
| Per-frame outside scheduler | `build_render_data` | ~15 component read locks | `ActiveCamera`, `MaterialTable`, `LightAtlas`, `CellLightingRes`, etc. | hot bridge |

Notes on stages:
- `fly_camera_system` runs in Early and writes Transform; `transform_propagation_system` is what produces final GlobalTransform in PostUpdate; `billboard_system` then overwrites the rotation field; `make_world_bound_propagation_system` reads the final GlobalTransform.
- `build_render_data` is called per frame from `App::about_to_wait` after the scheduler runs (entry: `byroredux/src/main.rs` ~ tick handler).

## Findings

(in progress)

## Summary
- (TBD)

### ECS-PERF-01: `lock_tracker::track_read` / `track_write` allocates `Vec<(TypeId, &str)>` on every novel-type acquire — release builds too
- **Severity**: HIGH
- **Dimension**: ECS Query Patterns
- **Location**: `crates/core/src/ecs/lock_tracker.rs:74-84` (read path) + `:115-125` (write path)
- **Status**: NEW
- **Description**: Both `track_read` and `track_write` build a `Vec<(TypeId, &'static str)>` of all currently-held lock types every time a lock is taken on a type whose entry isn't already present (`is_new`). That `held_others` Vec is then handed off to `global_order::record_and_check`, which is a no-op in release builds (the `#[cfg(not(debug_assertions))]` arm just does `let _ = held_others;`). The allocation happens unconditionally before the cfg switch — so release builds pay for a Vec they immediately throw away.
- **Evidence**:
  ```rust
  if is_new {
      let held_others: Vec<(TypeId, &'static str)> = map
          .iter()
          .filter(|(id, _)| **id != type_id)
          .map(|(id, state)| (*id, state.type_name))
          .collect();
      drop(map);
      #[cfg(debug_assertions)]
      global_order::record_and_check(type_id, type_name, &held_others);
      #[cfg(not(debug_assertions))]
      let _ = held_others;  // allocation already happened
  }
  ```
- **Impact**: `build_render_data` alone takes ~15 distinct read locks per frame; `animation_system` adds another ~17; transform/bound propagation each add 4–5; particle/billboard add 2–3. Conservatively 40+ novel-type acquires per frame. Each one allocates a `Vec` whose capacity scales with the count of *other* held locks at that moment (1–14 entries). In `build_render_data` alone the vector grows from 0 to 14 elements over the 15 acquisitions, so the cumulative allocation work is O(N²/2) ≈ 100 small allocations per frame just from that one function. Per allocation is ~50–100 ns (allocator fast path). At 60 fps: ~6 µs/frame, ~12 KB/frame churn. Sub-noise on its own, but every per-frame allocation increases allocator fragmentation and is a paper cut for the parallel-scheduler future where many threads hammer the same allocator.
- **Suggested Fix**: Either (a) gate the entire `held_others` collection inside `#[cfg(debug_assertions)]`, or (b) pass an iterator (`map.iter().filter(...).map(...)`) into a debug-only helper that accepts `impl Iterator`, eliding the Vec entirely. (a) is the one-line fix.

### ECS-PERF-02: `animation_system` rebuilds `NameIndex` HashMap from scratch on every Name-component count change
- **Severity**: HIGH
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:323-343`
- **Status**: NEW
- **Description**: When the count of `Name` components differs from `NameIndex.generation` (i.e. any Name was added or removed), the system allocates a fresh `std::collections::HashMap::new()`, walks every `Name` entity, inserts into the new map, then swaps it in. With cell streaming actively touching Name components every frame during a transition (megaton/wasteland: hundreds to thousands of Names), this fires the rebuild repeatedly and thrashes the allocator. The existing comment claims "only when count changes," but cell streaming and any spawn/despawn under `Name` invalidates that condition every frame during the transition.
- **Evidence**:
  ```rust
  if needs_rebuild {
      let name_query = match world.query::<Name>() { ... };
      let mut new_map = std::collections::HashMap::new();   // fresh alloc
      for (entity, name_comp) in name_query.iter() {
          new_map.insert(name_comp.0, entity);              // grow + rehash
      }
      drop(name_query);
      let mut idx = world.resource_mut::<NameIndex>();
      idx.map = new_map;                                    // drop old map
      idx.generation = current_name_count;
  }
  ```
- **Impact**: For a fully populated cell (~1500 Names per Megaton baseline), each rebuild allocates one HashMap with ~16 incremental rehashes (capacity doubling 0→1→...→2048) plus 1500 entries. ~50–100 µs per rebuild. During an exterior-cell stream-in event multiple cells settle their Names over consecutive frames, so this fires every frame for ~30 frames (~3 ms total). Steady-state interior: zero rebuilds — the check correctly stabilizes. The cost is concentrated at cell transitions where the user already feels a hitch.
- **Suggested Fix**: Reuse the existing `NameIndex.map` instead of allocating: `idx.map.clear(); for (e, n) in name_query.iter() { idx.map.insert(n.0, e); }`. The HashMap retains its allocated buckets across rebuilds, eliminating ~30 µs/rebuild and the per-frame churn during cell streaming. Bonus: pre-size with `idx.map.reserve(current_name_count - idx.map.len())` if the count is growing.

### ECS-PERF-03: `transform_propagation_system` rediscovers root entities every frame by scanning every Transform
- **Severity**: MEDIUM
- **Dimension**: ECS Query Patterns
- **Location**: `crates/core/src/ecs/systems.rs:72-81`
- **Status**: NEW
- **Description**: Phase 1 of the propagation system iterates every entity in the `Transform` storage and calls `parent_q.get(entity).is_none()` to identify roots. The root set is *nearly static* — it changes only on top-level spawn/despawn (cell load, cell unload, NPC spawn). On Megaton baseline the Transform storage holds ~6 000 entities (every NIF subnode gets one); the per-frame scan is therefore ~6 000 iterator steps + 6 000 `Parent` storage lookups, every frame, in steady state inside a static interior. The same scratch (`roots: Vec<EntityId>`) is captured across frames and cleared, so the allocation is amortized — but the work to *fill it* is not.
- **Evidence**:
  ```rust
  // Phase 1: find root entities (have Transform but no Parent).
  for (entity, _) in tq.iter() {
      let is_root = parent_q
          .as_ref()
          .map(|pq| pq.get(entity).is_none())
          .unwrap_or(true);
      if is_root {
          roots.push(entity);
      }
  }
  ```
- **Impact**: At Megaton steady-state (~6 000 Transforms, ~30 roots), the scan does ~5 970 wasted lookups per frame. Each `parent_q.get` is a sparse-set hash + bounds check, ~30–50 ns; total ~250 µs/frame for nothing. At 60 fps that's 1.5 % of the frame budget burned to confirm a list that hasn't changed since cell load. Scales linearly with content — a populated FNV exterior grid (radius 3) can push Transform counts to ~30 000 with the same handful of roots; the wasted scan grows to ~1.5 ms/frame.
- **Related**: ECS-PERF-04 (same pattern in `world_bound_propagation_system`), #791 (E-N2: `unload_cell` victim collection — same family of "scan every X to find a small subset" anti-pattern).
- **Suggested Fix**: Maintain a `RootEntities: Resource<HashSet<EntityId>>` updated by the spawn/despawn path: insert when an entity gets a Transform without a Parent, remove when it gets a Parent or is despawned. The propagation system reads the set in O(roots) instead of O(transforms). Initial population happens at cell-load time (~one walk over the freshly-loaded subtree). Alternative cheaper interim: compare `Transform::len()` and `Parent::len()` against last-frame values; only re-scan if either changed (matches the `NameIndex.generation` pattern already used elsewhere).

### ECS-PERF-04: `world_bound_propagation_system` rediscovers roots every frame by scanning every GlobalTransform
- **Severity**: MEDIUM
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:921-935`
- **Status**: NEW
- **Description**: Identical anti-pattern to ECS-PERF-03, but iterates `GlobalTransform` instead of `Transform`. Both storages have the same population (one entry per ECS entity that participates in the scene graph), so the per-frame cost is the same.
- **Evidence**:
  ```rust
  let Some(tq) = world.query::<GlobalTransform>() else { return; };
  let parent_q = world.query::<Parent>();
  for (entity, _) in tq.iter() {
      let is_root = parent_q
          .as_ref()
          .map(|pq| pq.get(entity).is_none())
          .unwrap_or(true);
      if is_root { roots.push(entity); }
  }
  ```
- **Impact**: Same as ECS-PERF-03 — ~250 µs/frame wasted at Megaton steady-state, scaling linearly with scene-graph population. This system also runs second in the PostUpdate stage right after `transform_propagation_system`, so the wasted work is **doubled** in practice (two systems both scanning the full transform storage to find the same root set).
- **Related**: ECS-PERF-03 (same fix unifies both call sites). The shared `RootEntities` resource would serve both systems.
- **Suggested Fix**: Same as ECS-PERF-03; both systems consume the same maintained `RootEntities` resource. If a `RootEntities` resource is a heavier change than warranted, a cheaper interim is to cache the root set inside the system closure (already a `move` closure with captured `roots: Vec<EntityId>`) and invalidate when `Transform::len() != last_seen_len` — same generation pattern as `NameIndex` and `SubtreeCache`.

### ECS-PERF-05: `animation_system` queries `Name` storage twice during the prelude (SubtreeCache + NameIndex)
- **Severity**: LOW
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:306,324`
- **Status**: NEW
- **Description**: The prelude takes two separate `world.query::<Name>()` read locks back-to-back, both just to call `.len()`. They could share one query handle. Cost is dominated by the lock acquisition (RwLock fast-path read = ~10–30 ns each), but on the per-frame critical path with the lock-tracker bookkeeping in ECS-PERF-01 each acquisition also pays one Vec allocation in release builds.
- **Evidence**:
  ```rust
  // Block 1 — SubtreeCache generation check
  {
      let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);
      let needs_clear = world.try_resource::<SubtreeCache>()
          .map(|c| c.generation != current_name_count).unwrap_or(false);
      // ...
  }
  // Block 2 — NameIndex generation check
  {
      let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);
      // ... identical pattern
  }
  ```
- **Impact**: ~50 ns/frame plus one Vec allocation in release (see ECS-PERF-01). Trivial in isolation; included for completeness because it compounds with ECS-PERF-01. Also a minor correctness smell: the two checks observe `Name::len()` independently, so a Name spawn between block 1 and block 2 would invalidate one cache and not the other for one frame. Today the only path that mutates Names mid-system is `event_cleanup_system` in the Late stage, so the inconsistency is unreachable — but the pattern is fragile.
- **Suggested Fix**: Merge the two blocks into one — query Name once, capture the count, run both generation checks against the same value.

### ECS-PERF-06: `animation_system` allocates fresh `events` and `seen_labels` Vec per AnimationStack entity per frame
- **Severity**: LOW
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:567-568` (inside the `for entity in stack_entities` loop)
- **Status**: NEW
- **Description**: The transform/blend code path explicitly hoisted `channel_names_scratch` and `updates_scratch` out of the per-entity loop (per-comments referencing #251/#252) — but the text-event scratches `events: Vec<AnimationTextKeyEvent>` and `seen_labels: Vec<FixedString>` were left as fresh allocations inside the loop. Every AnimationStack entity therefore pays two `Vec::new()` calls per frame, plus growth-doubling reallocs as labels accumulate.
- **Evidence**:
  ```rust
  for entity in stack_entities {                          // per-frame outer loop
      // ... advance, cache prep ...
      let mut events: Vec<AnimationTextKeyEvent> = Vec::new();   // ← fresh alloc per entity
      let mut seen_labels: Vec<FixedString> = Vec::new();        // ← fresh alloc per entity
      let accum_root: Option<FixedString>;
      // ...
      visit_stack_text_events(stack, &registry, &mut seen_labels, |time, sym| {
          events.push(AnimationTextKeyEvent { label: sym, time });
      });
      // ... mem::take(&mut events) at line 659 hands ownership away,
      //     so the next iteration would need a fresh Vec anyway ──
      //     but only when `events.is_empty()` was false. The empty
      //     case discards the allocation needlessly.
  }
  ```
- **Impact**: Today AnimationStack entities are rare (count = number of NPCs with multi-layer animation; Megaton has ~0). M41 and beyond: ~10–50 NPCs per cell with stacks. At 50 stack entities × 2 Vec allocations × 60 fps = 6 000 small allocations/sec. Negligible CPU but adds allocator churn.
- **Suggested Fix**: Hoist both Vecs to the outer scope alongside `channel_names_scratch` and `updates_scratch`; `clear()` at the top of each iteration. For the `events` vec, replace the `mem::take` insert pattern with `eq.insert(entity, AnimationTextKeyEvents(events.drain(..).collect()))` so the buffer's capacity stays with the scratch — or change `AnimationTextKeyEvents` to accept a `&mut Vec` and drain into its own owned Vec at the storage boundary.

### ECS-PERF-07: `billboard_system` redundantly cycles a read+write lock on GlobalTransform when one write lock would suffice
- **Severity**: LOW
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/systems.rs:755-772`
- **Status**: NEW
- **Description**: The system takes `world.query::<GlobalTransform>()` (read) to copy out the camera's GT, drops it, then re-acquires `world.query_mut::<GlobalTransform>()` (write) for the billboard write loop. Two acquisitions of the same storage when one write query would do — the write query exposes `get` for the camera read just as well as `get_mut` for the billboard writes.
- **Evidence**:
  ```rust
  let Some(cam_gq) = world.query::<GlobalTransform>() else { return; };
  let Some(cam_global) = cam_gq.get(cam_entity).copied() else { return; };
  drop(cam_gq);                                                 // release read lock
  // ... compute cam_pos, cam_forward ...
  let Some(bq) = world.query::<Billboard>() else { return; };
  let Some(mut gq) = world.query_mut::<GlobalTransform>() else { return; };  // re-acquire as write
  for (entity, billboard) in bq.iter() {
      let Some(global) = gq.get_mut(entity) else { continue; };
      // ...
  }
  ```
- **Impact**: One extra RwLock acquire/release pair per frame (~50–100 ns) plus one extra Vec allocation in release (ECS-PERF-01). Trivial. Surface area for a future deadlock if the prelude grows another lock acquisition between the read drop and write re-acquire.
- **Suggested Fix**: Take the write lock first, read camera GT through `gq.get(cam_entity).copied()`, then proceed to the billboard write loop with the same query handle.

### ECS-PERF-08: `unload_cell` victim collection scans every loaded CellRoot entity
- **Severity**: LOW (per-cell-transition cost only, not per-frame)
- **Dimension**: ECS Query Patterns
- **Location**: `byroredux/src/streaming.rs` — see existing issue
- **Status**: Existing: #791
- **Description**: Documented in #791. Not on the per-frame path, but worth noting in the inventory for context.
- **Suggested Fix**: See #791.

## Summary
- **7 NEW findings**: 0 CRITICAL, 2 HIGH, 3 MEDIUM, 2 LOW (+ 1 Existing reference)
- HIGH findings concentrate on per-frame allocation patterns (lock-tracker Vec, NameIndex HashMap rebuild) that fire during the cell-streaming spike where the user already feels a hitch.
- MEDIUM findings (ECS-PERF-03/04) are the same root-discovery anti-pattern in two propagation systems, sharing a single fix.

### Top 3 Quick Wins (low effort, high impact)
1. **ECS-PERF-02** — Replace `idx.map = HashMap::new(); ... idx.map = new_map` with `idx.map.clear(); ... idx.map.insert`. ~5-line change. Eliminates ~30 µs/rebuild and ~3 ms total during cell-stream-in spikes.
2. **ECS-PERF-01** — Move the `held_others: Vec<...>` collection inside `#[cfg(debug_assertions)]`. ~3-line change. Eliminates ~100 small per-frame allocations in release.
3. **ECS-PERF-05** — Merge the two `world.query::<Name>()` blocks in `animation_system`. ~10-line refactor. Eliminates one redundant lock per frame.

### Top 3 Architectural Changes (high effort, high impact)
1. **ECS-PERF-03/04 unified fix** — Introduce `RootEntities: Resource<HashSet<EntityId>>` maintained by spawn/despawn hooks. Eliminates ~500 µs/frame steady-state at Megaton scale, ~3 ms/frame at exterior-grid scale. Touches the spawn/despawn API surface.
2. **`build_render_data` extract stage** — Already documented as deferred (#501) in the source. The 13-storage read-bundle held across the entire build window is the single biggest blocker for the parallel-scheduler future. Implementation should wait until M40 lands so the actual contention pattern informs the design.
3. **ECS-PERF-06 generalisation** — Audit every per-frame system for hoist-able scratch buffers, then introduce a `SystemScratch<T>` resource pattern that owns and recycles Vec/HashMap scratches with `ScratchTelemetry` integration (today only renderer-side scratches are tracked). Would close the gap between renderer-side scratch hygiene and ECS-side ad-hoc Vec allocation.

