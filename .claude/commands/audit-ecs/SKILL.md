---
description: "Deep audit of the ECS — storage backends, queries, world, systems, resources"
---

# ECS Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

## Dimensions

### 1. Storage Correctness
- SparseSetStorage: swap-remove fixes sparse pointer for moved entity
- SparseSetStorage: insert into existing entity overwrites (no duplicate)
- PackedStorage: binary_search maintains sort invariant after insert/remove
- PackedStorage: insert at correct position, remove shifts correctly

### 2. Query Safety
- RwLockReadGuard lifetime doesn't outlive the query
- RwLockWriteGuard is exclusive (no double write-lock)
- query_2_mut / query_2_mut_mut: locks acquired in TypeId order
- Same-type double-lock: panics with clear message (not deadlock)
- QueryRead/QueryWrite Deref impls don't expose unsound references

### 3. World Integrity
- TypeMap: downcast_ref/downcast_mut always correct (TypeId matches)
- Lazy storage init: first insert creates storage, queries before insert return None
- Entity ID monotonic (no reuse, no overflow handling — document limitation)
- find_by_name: no string comparison in scan (FixedString integer equality only)

### 4. Resource Safety
- resource() panics with type name on missing resource
- try_resource() returns None (no panic)
- ResourceRead/ResourceWrite Deref impls type-safe
- Resources usable from systems (interior mutability via &self)

### 5. System & Scheduler
- Blanket impl for Fn(&World, f32) correct (no ownership issues)
- Mutations from system N visible to system N+1 in same run()
- Empty scheduler runs without panic
- system_names() returns correct order

### 5b. M27 — Parallel Scheduler Access Declarations (closed 2026-05-23)
- **M27 Phase 1+2** (`a9810d40`): every system in `byroredux/src/systems/*.rs` declares its component / resource access surface (read / write per type). The declaration is checked by the scheduler at registration time; any system missing a declaration is a regression (`SystemAccess::default()` would conservatively serialize everything)
- **M27 Phase 3** (`05fe2bac`): the four remaining access conflicts between parallel-stage systems were resolved, bringing the scheduler to **0 unknown / 0 conflicts**. Audit pattern: any system pair that re-introduces a conflict on the same parallel stage must either be moved to different stages or have one side's access narrowed
- **Undeclared-access classification (#1394, `a7e1502b`)**: systems with `None` declared access (closures and not-yet-migrated systems) are classified as `AccessConflict::Parallel` — NOT `Unknown`. This was changed so the ABBA detector can still analyse these pairs rather than blocking on `Unknown`. An audit that treats undeclared-access pairs as invisible to conflict analysis has a stale premise. The `unknown_pair_count()` accessor in `AccessReport` will return 0 when all undeclared pairs are now tagged Parallel; only truly undeclared pairings that have not been processed produce Unknown.
- **#1236 + #1237 R7 scheduler access surface extended to exclusives** (`94e78b9f`): exclusive (Late-stage) systems also declare access surface now — the cell-loader drain, debug-server drain, audio prune, and event-cleanup systems all have explicit declarations. Verify no exclusive system regresses to undeclared access
- **#1238 Scheduler stage-order chain pinned across all 5 stages** (`54ea11c0`): Early → Update → ParallelUpdate → Late → LateExclusive ordering pinned by a test. Re-ordering or merging stages without updating the test is the regression pattern
- **Regression guard**: `cargo test -p byroredux` must report **0 unknown** and **0 conflicts** at scheduler init (look for the boot-time log line). Any non-zero count is an audit finding

### 6. Unsafe Code Review
- World::get() uses raw pointer to extend lifetime — is the safety argument valid?
- Any other unsafe blocks: document safety invariants

### 7. Streaming, Scripting & New Component Lifecycles
- M40 streaming (`byroredux/src/streaming.rs`): cell-load attaches components, cell-unload removes them — verify no orphaned components after a load/unload cycle
- M41 NPC spawn (`byroredux/src/npc_spawn.rs`): ACHR REFR → entity dispatch is idempotent (same REFR FormId never spawns twice)
- Scripting events (`crates/scripting/src/events.rs`): transient marker components (ActivateEvent, HitEvent, TimerExpired) are removed by `event_cleanup_system` (Late stage) — verify single-frame lifetime
- ScriptTimer (`crates/scripting/src/timer.rs`): `timer_tick_system` decrements per-frame, fires TimerExpired marker on hit — verify no negative-time accumulation
- Animation controller (`crates/core/src/animation/controller.rs`): controller component lifecycle vs AnimationPlayer — verify no dangling clip references after unload
- AnimationClipRegistry (`crates/core/src/animation/registry.rs`): #790 dedupes by lowercased path so cell streaming doesn't grow it unboundedly. Without case-folding interning, one full keyframe set leaks per cell load — observable as steady RAM growth across exterior streaming
- DebugDrainSystem (`crates/debug-server/src/system.rs`): Late-stage exclusive — verify no World mutation outside drain (per-client TCP threads must enqueue commands, not mutate)
- AudioWorld resource (`crates/audio/src/lib.rs`, M44): `audio_system` runs Late-stage; `OneShotSound` markers are removed by `prune_stopped_sounds` once the kira playback transitions to `PlaybackState::Stopped` — verify no infinite-marker leak path. `AudioListener` / `AudioEmitter` lifecycle: spatial sub-track handle drop must precede listener handle drop (kira invariant)
- Particle emitter component lifecycle (NIFAL typed-block path): `byroredux/src/systems/particle.rs::apply_emitter_params` populates the `ParticleEmitter` component (`crates/core/src/ecs/components/particle.rs`) from `byroredux_nif::import::ImportedEmitterParams` (struct in `crates/nif/src/import/types.rs`, built by `extract_emitter_params` / `extract_emitter_rate` in `crates/nif/src/import/walk/mod.rs` from the typed `NiPSysEmitter` / `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` blocks in `crates/nif/src/blocks/particle.rs`). Pin the override semantics: authored size is `initial_radius × base_scale.unwrap_or(1.0)` (Oblivion has no `base_scale` → multiplier 1.0) and color is NOT clobbered by the translation — see `apply_emitter_params_size_defaults_base_scale_to_one` and `apply_emitter_params_overrides_kinematics_and_size_not_color`. The regression is a typed-block translation that zero-sizes the emitter or overwrites the preset color. See also `/audit-nifal` (NIF→canonical translation boundary)
- Character / light-anim component lifecycle: `byroredux/src/systems/character.rs` owns the player/character KCC state via `byroredux_physics::CharacterController` (+ `RapierHandles`); `byroredux/src/systems/light_anim.rs::animate_lights_system` reads `LightFlicker` (`crates/core/src/ecs/components/light.rs`) against `LightSource`. These systems own components added since this dim-7 list was first written — verify no orphaned `CharacterController` / `LightFlicker` after a cell load/unload cycle, matching the `streaming.rs` orphan invariant above

### 8. ECS Hot-Path Performance Invariants (2026-05-04 batch — regression guards)
- `lock_tracker::held_others` Vec collection is `cfg(debug_assertions)`-gated (#823 ECS-PERF-01). Re-enabling for release rebuilds ~100 small allocs/frame for a no-op
- `NameIndex.map` (struct defined in `byroredux/src/components.rs`) is refilled in place via `HashMap::clear` + `reserve` + reinsert inside `animation_system` (`byroredux/src/systems/animation.rs`, the `idx.map.clear()` block) instead of allocating a fresh map (#824 ECS-PERF-02). The `HashMap::new() + std::mem::swap` pattern costs ~3 ms cell-stream-in spike — a regression test should pin the in-place refill
- `transform_propagation_system` (now in `byroredux/src/systems/animation.rs` post-Session-34 split) caches the root entity set keyed on `(Transform::len, Parent storage len OR 0 when Parent absent, world.next_entity_id())` (#825 ECS-PERF-03; see `crates/core/src/ecs/systems.rs` cache state + invalidation logic — third field is an `EntityId` value, not a count, and the Parent-len has `unwrap_or(0)` for scenes with no parent storage). Recomputing roots every frame is the ~250 µs/frame regression at Megaton scale
- `animation_system` (now in `byroredux/src/systems/animation.rs`) hoists `events` / `seen_labels` scratches out of the per-entity loop and uses `clone` (not `mem::take`) so capacity persists (#828 ECS-PERF-06). Per-iteration allocation is the regression pattern. Helpers `ensure_subtree_cache`, `write_root_motion`, `apply_bool_channels` factored out by `2bdbc36`; `write_lazy!` macro collapses 5 color-target match arms — drift in any of these helpers risks DRY-undo
- `footstep_system` (`byroredux/src/systems/audio.rs`) writes to the `FootstepScratch: Resource` (#932 PERF-CPU-02) using `mem::take` + restore pattern preserving Vec capacity across frames. Per-frame `Vec::new` is the regression
- `World::despawn` poisoned-lock panic uses a `type_names` side-table to name the offending component (#466 E-03). Removing the side-table means panic messages lose the type name — bisecting takes 10× longer

### 9. NIFAL Canonical Material in the Component Layer

The NIFAL canonical-translation tier resolves PBR scalars once, at the single `ImportedMesh → Material` boundary, so the renderer never re-classifies per draw. The ECS-owned `Material` component is the landing zone for that contract; this dimension guards it. See also `/audit-nifal` (the NIF→canonical translation layer audit) for the upstream boundary.

- **Plain-`f32` contract**: `Material` (`crates/core/src/ecs/components/material.rs`) carries `metalness: f32` and `roughness: f32` — fully resolved, NOT `Option<f32>`. The renderer reads `GpuMaterial.metalness` / `.roughness` directly. A regression to `Option`/`None` re-introduces per-draw classification
- **Single mutation site**: `byroredux/src/material_translate.rs::translate_material` is the SOLE boundary that translates `ImportedMesh → Material`. `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`, defined ~line 588) is the only fill-the-gap helper — it runs the shared keyword classifier (`classify_pbr_keyword`) and fills only the unset slot. No per-draw `classify_pbr` fallback survives in the render path (`byroredux/src/render/static_meshes.rs` documents the removal: "no per-draw keyword scan / classify_pbr fallback")
- **`resolve_pbr` is idempotent + preserves translator values**: calling `resolve_pbr` twice yields the same result, and it never overwrites scalars the upstream translator already supplied — pinned by `resolve_pbr_is_idempotent` and `resolve_pbr_preserves_upstream_translator_values` (plus `resolve_pbr_fills_only_missing_slot` / `resolve_pbr_clamps_authored_out_of_range`) in the `material.rs` test module. A change that clobbers authored metalness/roughness or makes resolve non-idempotent is an audit finding
- **ECS-adjacent material producers**: `crates/sfmaterial/` (Starfield CDB consumer) output flows into the canonical `Material`; if that path bypasses `translate_material` / `resolve_pbr` it breaks the single-boundary invariant. `crates/debug-ui/` (egui overlay) must not register or mutate gameplay components — note the boundary if it does. (19 crates total under `crates/`.)

## Process

1. Read each file in `crates/core/src/ecs/`
2. Run `cargo test -p byroredux-core` to verify all tests pass
3. Check each dimension
4. Save report to `docs/audits/AUDIT_ECS_<TODAY>.md`
