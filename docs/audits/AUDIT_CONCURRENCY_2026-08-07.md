# Concurrency & Synchronization Audit — 2026-08-07

**Scope**: Vulkan queue/AS sync, compute→AS→fragment barrier chains, ECS lock
ordering, scheduler access declarations, RwLock patterns (Resource↔Storage +
physics step), GPU resource lifecycle/teardown, worker threads & thread-safety
bounds. Full sweep, all 7 dimensions, `--depth deep` (default).

**Method**: 7 dimension passes (each an independent agent read of the live
source against the dimension checklist in `.claude/commands/audit-concurrency/SKILL.md`),
deduplicated against a fresh `gh issue list` pull
(`/tmp/audit/concurrency/issues_fresh.json`, 73 open issues) taken at
compile time, not just the setup-time snapshot. Dimension 3 was run twice
independently (original + duplicate); the duplicate's stronger-justified
severity (HIGH vs. the original's MEDIUM triage for the same finding) is
the version reported below, per the project's severity doctrine (impact,
not likelihood). Dimensions 1 and 2 were also run twice; the duplicate runs
confirmed the same findings with no discrepancy.

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 3 |
| LOW | 4 |
| **Total** | **8** |

All 8 findings are **NEW** (no overlap with any open or closed GitHub issue
found in the dedup pass). Four of seven dimensions (1: Vulkan Queue & AS
Sync, 4: Scheduler Access Declarations, 6: Resource Lifecycle, 7: Worker
Threads) came back **clean** — no findings — which is expected for
Dimensions 1, 4, 6, 7 given their status as heavily-hardened / regression-guard
surfaces from prior audit cycles (M27 scheduler migration closed, #1483/#639
lifecycle hardening, #1167/#1006/#1603 worker-thread hardening). Dimensions
2 (Compute→AS→Fragment Chains), 3 (ECS Lock Ordering), and 5 (RwLock
Patterns) each surfaced new findings.

## Dimension 1: Vulkan Queue & Acceleration-Structure Sync — CLEAN

No findings. Queue-submission mutex scoping, frame-in-flight fence
discipline, acquire→render→present semaphore chains, AS build→read barriers
(static BLAS, skinned BLAS refit, TLAS), the AS-build-input barrier access
flag (#507945d8 regression guard), deferred BLAS/scratch destruction vs.
in-flight reads (#a476b256/#1782 regression guard), swapchain-recreate sync,
and one-time-command blocking placement were all re-read directly against
source (not trusted from comments) and confirmed correct. Full detail in the
dimension scratch file; see also `build_blas_for_mesh` (`context/resources.rs:105`)
noted as dead code (zero call sites) — tech-debt, not a concurrency finding,
not filed here.

## Dimension 2: Compute → AS → Fragment Chains

The full compute→AS→fragment spine (skin palette → skin output →
BLAS build/refit → TLAS build → ray-query consumers) was traced end-to-end
and confirmed intact, including SVGF/TAA/volumetrics/caustic/water-caustic
cross-frame ping-pong indexing, the volumetrics `tlas_written` latch
symmetry (#1105 regression guard), the bloom within-frame RAW chain (#931
regression guard), and the MaterialBuffer SSBO upload ordering. One barrier
gap was found on a newer (`#2219`) fragment-stage consumer of the skin
output buffer, plus two LOW-severity quality/robustness issues.

### CHAIN2-D2-01: Skinned-vertex fragment read has no dedicated COMPUTE→FRAGMENT barrier — visibility rides the cluster-cull pass's trailing barrier
- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:398-405`; `crates/renderer/src/vulkan/context/draw.rs:2186-2220`; consumer at `crates/renderer/shaders/include/ray_hit.glsl:73-82`
- **Status**: NEW
- **Description**: `#2219` added a fragment-stage consumer of the `skin_vertices.comp` output buffer (`GpuInstance.skinnedVertexAddress`, a raw `GL_EXT_buffer_reference` device address dereferenced by `getHitTriWorldPositions` in `triangle.frag`/`water.frag`). The renderer's own skin-chain barrier for that buffer only publishes to the acceleration-structure-build stage (`skinned_blas_refit.rs:398-405`, `COMPUTE_SHADER/SHADER_WRITE → ACCELERATION_STRUCTURE_BUILD_KHR/SHADER_READ`). The follow-on AS barriers are `AS_WRITE → AS_READ` and their access scopes do not overlap the compute `SHADER_WRITE`, so no barrier in the chain makes that write visible to `FRAGMENT_SHADER/SHADER_READ`. The only barrier that happens to cover it is the cluster-cull pass's trailing global `VkMemoryBarrier` (`draw.rs:2186-2220`), emitted only inside `if let Some(ref cc) = self.cluster_cull` — and `ClusterCullPipeline::new` failure is a graceful degrade to `None` (`context/mod.rs:1943-1972`) that does not gate the RT path, so `cluster_cull == None` + skinned RT actors is a reachable configuration where the visibility guarantee silently disappears.
- **Evidence**:
  ```rust
  // skinned_blas_refit.rs:398-405 — the only publish of the skin output
  memory_barrier(
      &self.device, cmd,
      vk::PipelineStageFlags::COMPUTE_SHADER, vk::AccessFlags::SHADER_WRITE,
      vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
      vk::AccessFlags::SHADER_READ,   // AS-build INPUT only — no FRAGMENT dst
  );
  ```
  ```glsl
  // ray_hit.glsl:73-76 — the fragment-stage consumer
  if (hi.boneOffset != 0u && hi.skinnedVertexAddress != 0ul) {
      SkinnedVertexRef ref = SkinnedVertexRef(hi.skinnedVertexAddress);
  ```
  ```rust
  // draw.rs:2186-2220 — the incidental publish, inside a conditional
  if let Some(ref cc) = self.cluster_cull {
      memory_barrier(&self.device, cmd,
          vk::PipelineStageFlags::COMPUTE_SHADER, vk::AccessFlags::SHADER_WRITE,
          vk::PipelineStageFlags::FRAGMENT_SHADER, vk::AccessFlags::SHADER_READ);
  }
  ```
- **Impact**: With cluster-cull absent, every secondary-ray hit on a skinned actor (glass refraction, reflections, GI bounce) reconstructs face normals/tangents from a buffer whose compute write was never made visible to the fragment stage — symptom class is incoherent/garbage shading on skinned actors seen through or reflected in glass and water, varying by driver. Raster is unaffected (`triangle.vert` inline skinning is covered by the palette barrier's `VERTEX_SHADER` dst bit). In the default (cluster-cull-present) configuration this is currently correct by accident, not by a documented or tested dependency.
- **Trigger Conditions**: `ClusterCullPipeline::new` returns `Err` while `device_caps.ray_query_supported` is true and a skinned actor with a live `SkinSlot` is visible to a secondary ray. Also latent against any future reorder of the cluster-cull dispatch relative to the geometry pass, or an early-out on "no lights this frame".
- **Verification Path**: Validation layer. Run with `BYRO_VALIDATION=1` (Khronos + sync-validation) on a scene with a skinned actor beside glass, with cluster-cull forced to `None` (temporary `Err` injection at `context/mod.rs:1943`) — sync-validation should report `SYNC-HAZARD-READ-AFTER-WRITE` on the skin slot output buffer at `FRAGMENT_SHADER`. Not observable via `cargo test`; not observable in the default configuration at all.
- **Related**: `#2219` (added the fragment consumer); prior `/audit-renderer` passes (2026-08-03, 2026-08-07) flag `#2219` generically as "needs a RenderDoc capture on an animated actor beside glass" — this finding names the specific code-verifiable reason.
- **Suggested Fix**: Widen the existing `skinned_blas_refit.rs:398-405` barrier's dst to `ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER` (dst access already `SHADER_READ`) and document that `#2219` made the skin output a fragment-stage consumer. Purely additive synchronization — no reordering — but per the anti-speculation policy it should land together with the `BYRO_VALIDATION=1` confirmation run above, not on reasoning alone.

### CHAIN2-D2-02: Caustic parked-camera EMA counts global frames while each FIF slot only accumulates every other frame
- **Severity**: LOW
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/caustic.rs:743-756`
- **Status**: NEW
- **Description**: The caustic accumulator is per-FIF (`self.slots[frame].image`), not ping-ponged, and never cross-seeded between slots. The decay factor is derived from `self.parked_frames`, a single counter bumped once per `dispatch` call (once per global frame). At `MAX_FRAMES_IN_FLIGHT == 2` a given slot is only visited every other frame, so on its k-th visit it is decayed with `n = 2k-1` and admits new energy with weight `1/(2k)` after only `k` real samples — the estimator converges at roughly `1/√k` instead of the intended `1/k`.
- **Evidence**: `parked_frames` incremented once per `dispatch`; `decay_factor = (n/(n+1)).min(CAUSTIC_DECAY_MAX)` computed from that shared counter but applied to `self.slots[frame].image`, a per-FIF, never-cross-seeded image.
- **Impact**: No bias (still converges), but visible as residual half-rate shimmer on caustic pools for the first ~2 seconds of a parked camera — the exact artifact the EMA was added to remove. No synchronization hazard; the per-FIF fence fully covers the decay→splat read-modify-write chain.
- **Trigger Conditions**: Camera parked with a refractive caustic source on screen; observe the first ~2 seconds of convergence.
- **Verification Path**: Not a validation-layer issue — reproduce visually (`--cornell` or a glass-heavy interior) with a parked camera, or log `parked_frames`/`decay_factor` per frame against per-slot visit count.
- **Related**: `#321` (Option A caustic splat), CHAIN2-D2-01 (same file family).
- **Suggested Fix**: Make the parked counter per-FIF (`parked_frames: [u32; MAX_FRAMES_IN_FLIGHT]`) so `n` counts that slot's own visits, or seed one slot from the other. The former is a two-line change.

### CHAIN2-D2-03: `skinnedVertexAddress` can be emitted from a stale `SkinSlot` when a skinned entity's mesh becomes non-RT-capable
- **Severity**: LOW
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2420-2436`; `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:109-142,196-236,608-649`
- **Status**: NEW
- **Description**: `record_skinned_blas_refit`'s `dispatches` collection skips any skinned draw whose mesh is `!mesh.rt_capable` (`skinned_blas_refit.rs:124-126`); the per-entity capacity-stale reconciliation (`:213-236`) and LRU-liveness bump both live inside that loop. But `GpuInstance`'s builder (`draw.rs:2420-2434`) populates `skinned_vertex_address` from `self.skin_slots.get(&entity_id)` unconditionally for any `bone_offset != 0` draw, with no `rt_capable`/capacity cross-check. For up to `MAX_FRAMES_IN_FLIGHT + 1` frames (until the LRU sweep reaps the orphaned slot), a skinned draw remapped from an RT-capable mesh to a non-RT-capable one with a higher vertex count publishes a raw device address sized for the previous mesh, while `ray_hit.glsl` indexes it with the new mesh's index buffer (no descriptor range check on a `buffer_reference` load).
- **Evidence**: `draw.rs:2420` looks up `self.skin_slots.get(&entity_id)` with no gate; `skinned_blas_refit.rs:124`'s `if !mesh.rt_capable { continue; }` guard never runs for such a draw, so the slot is never invalidated in time.
- **Impact**: If the new mesh has more vertices than the slot's allocation, the fragment shader reads past the end of a device allocation via a raw address — best case garbage normals, worst case a GPU page fault/device loss. Window is bounded (3 frames) and requires a specific remap direction.
- **Trigger Conditions**: A skinned entity's `MeshHandle` is remapped (M41 equip/outfit swap/cell reload) from an RT-capable mesh to a non-RT-capable one (skinned effect-shader proxy or decal) with more vertices, drawn within the next `MAX_FRAMES_IN_FLIGHT + 1` frames.
- **Verification Path**: `BYRO_VALIDATION=1` will not catch this (raw buffer-reference loads are outside descriptor validation). Instrument instead: assert in `draw.rs:2420` that the slot's `vertex_count()` is `>= mesh.vertex_count` for the draw's mesh, and run `docs/smoke-tests/m41-equip.sh` — a `debug_assert` trip confirms reachability. RenderDoc would show the buffer-size mismatch on the instance.
- **Related**: `#1297`/`#1298` (the capacity-stale guard this path bypasses); `#2219`; CHAIN2-D2-01 (same consumer).
- **Suggested Fix**: Gate `slot_address` on the slot's `vertex_count()` matching the draw's live `mesh.vertex_count` (already in hand at that site), falling back to `0`/bind-pose on mismatch — a pure CPU-side guard with no barrier implications.

## Dimension 3: ECS Lock Ordering & Deadlock

TypeId-sorted acquisition (`query_2_mut`/`query_2_mut_mut`/`resource_2_mut`),
`lock_tracker` same-thread reentrancy coverage, and poisoning discipline
across every `world.rs` acquisition path all check out clean. One HIGH,
one MEDIUM, and two LOW findings surfaced in guard-lifetime discipline and
the tracker's own panic-recovery path.

### CONC-D3-2026-08-07-01: Animation channel sinks are lock-acquired in NIF-authored channel order, so the acquisition order between six storages is content-determined
- **Severity**: HIGH
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/animation.rs:139-221` (`apply_color_channels`), `byroredux/src/systems/animation.rs:247-330` (`apply_float_channels`)
- **Status**: NEW
- **Description**: Both helpers lazily acquire one `QueryWrite` per sink on first use (`write_lazy!` → `$cache.get_or_insert_with(|| $world.query_mut::<$Comp>())`) and hold every acquired guard for the rest of the call. Which guard is taken first — and therefore the pairwise acquisition order across the six storages — is decided by the order channels appear in the `AnimationClip`, i.e. by authored NIF/KF content, not by code. `apply_color_channels` can hold up to six guards simultaneously (`AnimatedDiffuseColor`, `AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor`, `LightSource`); `apply_float_channels` up to five. This is exactly the situation the TypeId-sort invariant exists to prevent, and it is the only place in the audited surface where a lock order is not fixed at compile time — materially different from the already-tracked "fixed order, safe only by exclusive scheduling" class (#2153/#2154/#2269), since here both acquisition directions genuinely occur within a single frame, driven by content.
- **Evidence**: Clip A ordered `[Diffuse, Emissive]` records the `AnimatedDiffuseColor → AnimatedEmissiveColor` edge in `lock_tracker::global_order::GRAPH`; clip B ordered `[Emissive, Diffuse]`, processed later in the same `for ps in playback_scratch` loop, then acquires `AnimatedDiffuseColor` while `AnimatedEmissiveColor` is held. `record_and_check` (`lock_tracker.rs:256-270`) tests exactly `new_edges.contains(held_id)` and panics with "ECS cross-thread deadlock risk (ABBA)". The same applies inside `apply_float_channels` for e.g. `[Alpha, UvOffsetU]` vs. `[UvOffsetU, Alpha]`, which is common shader/UV-animation authoring.
- **Impact**: (a) A debug build with `BYRO_LOCK_ORDER_CHECK=1` — set on the `lock-order-check` and `vulkan-validation` CI jobs — aborts the process the first time a cell loads two clips whose channel orders disagree, silently capping the detector's usable coverage at content that happens not to trip it (eroding the guarantee #2137/#2155 were filed to establish). (b) The latent deadlock is real but currently unreachable: `make_animation_system()` is the sole entry in the `Stage::Update` parallel batch (`boot.rs:748`), `animate_lights_system` (the other `LightSource` writer) is `add_exclusive`, and `render/lights.rs` runs main-thread after `scheduler.run`. Adding any second `add_to_with_access` system to `Stage::Update` touching two of these six storages converts (b) into a live hang.
- **Trigger Conditions**: (a) Debug build + `BYRO_LOCK_ORDER_CHECK=1` + any loaded content where two `AnimationClip`s (or two entities' clips) list two of the six sinks in opposite order — no concurrency needed, the graph is process-global. (b) True deadlock additionally requires a second thread in the same stage holding one of the pair; not currently reachable.
- **Related**: #313 (TypeId-sorted acquisition), #1410/#2137 (detector in CI), #2155 (detector coverage is reachability-bounded — this finding is a concrete instance of the tail it warns about), #1785 (established all six colour sinks as live).
- **Suggested Fix**: Make the acquisition order structural rather than data-driven — bucket channels by target in a first pass and acquire the needed sinks in a fixed declared order (the order `boot.rs:753-780` already lists them in), or acquire each sink, drain its channels, and drop it before touching the next.

### CONC-D3-2026-08-07-02: `animation_system_inner` holds `AnimationClipRegistry` + `NameIndex` read guards across every component acquisition in the system, undocumented as a lock-order constraint
- **Severity**: LOW
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/animation.rs:386-833` (guards taken at `:386` and `:454`)
- **Status**: NEW
- **Description**: `registry` and `name_index` are bound to function-scope locals living until `animation_system_inner` returns; every subsequent acquisition in the body (`Name`, `SubtreeCache`, `AnimationPlayer`, `AnimationTextKeyEvents`, `Transform`, `RootMotionDelta`, `AnimationStack`, all eleven animated-channel sinks) happens underneath both — the widest hold-stack in the engine (~15 distinct types deep) — with no comment stating "nothing may acquire `AnimationClipRegistry` or `NameIndex` while holding any animation component storage," unlike the carefully documented `NameIndex`-before-`Name` rule a few lines away. This system is registered in a **parallel** lane (`boot.rs:748`, `Stage::Update`), so the constraint is not backstopped by exclusive scheduling — only by the current fact that it's alone in that lane.
- **Evidence**: `let Some(registry) = world.try_resource::<AnimationClipRegistry>() else { return; };` (`:386`) and `let name_index = world.try_resource::<NameIndex>().unwrap();` (`:454`), no `drop()` anywhere in the function; `registry` still borrowed at `:652`/`:693`, `name_index` at `:540` (inside the Phase 2 loop that takes `Transform`/sink write guards).
- **Impact**: No live deadlock today. Any future system that reads a clip registry or the name index while already holding `Transform` closes an ABBA cycle against a system that already runs on a rayon worker — the configuration the `add_exclusive` argument in #2153/#2126 does not cover.
- **Related**: #2153 (`CHARAL-D3-01`, same class, exclusive-scheduled), #2154 (closed, same class), #2126 (established the doc-comment convention), #827/#824 (the `NameIndex`/`Name` rule this system already documents), CONC-D3-2026-08-07-01.
- **Suggested Fix**: Add the same hold-stack comment style the `NameIndex`-before-`Name` block carries, naming these two as the outermost locks and stating they must never be acquired beneath an animation component storage; or narrow `name_index`'s live range to the Phase 2 loop.

### CONC-D3-2026-08-07-03: A `global_order` ABBA panic leaves a stale row in the thread-local `LOCKS` map — the #137/#2149 leak class at the one site those fixes don't cover
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering
- **Location**: `crates/core/src/ecs/lock_tracker.rs:58-95` (`track_read`), `:99-137` (`track_write`)
- **Status**: NEW
- **Description**: Both tracker functions mutate the thread-local `LOCKS` map (insert entry, bump `read_count`/set `has_write`) **before** calling `global_order::record_and_check`, which can panic. The RAII scope guard that would undo the row is constructed only *after* `track_*` returns, so at the moment of an ABBA panic nothing owns the row. If that panic is caught (`catch_unwind`), the thread's `LOCKS` map permanently claims the type is still held, and the very next `world.query::<T>()` on that thread panics with a spurious "ECS deadlock detected" — exactly the failure mode #2149's comments describe and were written to prevent at *other* sites.
- **Evidence**: `entry.read_count += 1;` happens before `global_order::record_and_check(...)`, which can panic. The module's own test acknowledges the leak and works around it rather than fixing it: `lock_tracker.rs:543-545`, `// catch_unwind leaves the thread-local tracker in whatever state the panic interrupted. // Wipe it for the next scenario` followed by `LOCKS.with(|l| l.borrow_mut().clear());`. Note the same-thread deadlock panics do **not** leak (they fire before the mutation) — this is specific to the `global_order` path.
- **Impact**: Debug builds with `BYRO_LOCK_ORDER_CHECK=1` only. Any harness or future per-frame recovery scheme that catches a detector panic gets a poisoned tracker and a cascade of misleading "deadlock detected" reports pointing at innocent call sites, making the first real report unreadable. Blast radius is one thread's tracker state.
- **Trigger Conditions**: Debug build, `BYRO_LOCK_ORDER_CHECK=1`, an ABBA edge observed, and the resulting panic caught rather than fatal.
- **Related**: #137 (original scope-guard design), #2149 (same leak class fixed in `World::query`/`query_mut`/`query_2_*`), #313, #2155.
- **Suggested Fix**: Move `global_order::record_and_check` out of the mutating section (collect `held_others` first, call it before the `LOCKS` mutation), or wrap it so a panic rolls the row back before resuming the unwind.

### CONC-D3-2026-08-07-04: Two per-frame `Mutex` acquisitions silently recover from poison with `into_inner()` and no rationale comment
- **Severity**: LOW
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/metrics.rs:79` (`state.sys.lock()`), `byroredux/src/systems/metrics.rs:96` (`alloc_res.0.lock()`)
- **Status**: NEW
- **Description**: `metrics_sample_system` acquires two inner `Mutex`es (`MetricsState::sys`, the `sysinfo::System` handle; `AllocatorResource.0`, the gpu-allocator) as `.lock().unwrap_or_else(|e| e.into_inner())` — deliberately continuing on a poisoned lock, the exact inverse of the `#466` fail-fast lock-poison doctrine applied to every ECS storage/resource, with no comment explaining the deviation at either site. For `AllocatorResource` specifically, recovering means calling `generate_report()` on a `gpu_allocator` instance whose invariants were mid-update when some other thread panicked.
- **Evidence**: `let mut sys = state.sys.lock().unwrap_or_else(|e| e.into_inner());` / `let alloc = alloc_res.0.lock().unwrap_or_else(|e| e.into_inner());`. Contrast `world.rs:22-28`/`:45-51`, where a poisoned ECS lock re-panics with the type name specifically so "a post-panic access fails loud, never silently reads torn state."
- **Impact**: Bounded — feeds `MetricsSnapshot` (debug overlay/`byro-dbg`), so worst case is a wrong or torn diagnostics number, not gameplay state. The real cost is doctrine drift: the poison policy is stated as absolute in the ECS layer and quietly not followed two crates over.
- **Related**: #466 (fail-fast poison doctrine), #1837 (the `insert_resource` follow-up removing the last `.ok()`-swallow in `world.rs`).
- **Suggested Fix**: Either fail loud like the ECS layer, or keep `into_inner()` and add a one-line comment stating this metric is diagnostics-only and a torn read is preferred to losing the overlay.

## Dimension 4: Scheduler Access Declarations — CLEAN (regression guard)

No findings. The `AccessConflict` three-variant model (`None`/`Unknown`/`Conflict`,
no `Parallel` variant), the migration KPIs (`undeclared_parallel_count()` etc.,
pinned by `debug_assert_eq!` guards at `boot.rs:1104-1132`), the exclusive-phase
placement of `audio_system`/`spin_system` and the `player_controller_system`
merge (M27 Phase 3), and the re-entry/panic-policy invariants (`Scheduler`
never a `Resource`; fail-fast on panic, no `catch_unwind`) all verified intact.
`cargo test -p byroredux-core --lib ecs::scheduler::` (24/24) and
`ecs::access::` (10/10) both pass.

## Dimension 5: RwLock Patterns — Resource↔Storage & Physics Step

`collect_newcomers`→`register_newcomers`, `apply_buoyancy`,
`release_victim_rapier_bodies` (#1520), and the `dump_awake_fallers`/
`dump_spawn_collider_census` #2136 regression guard were all confirmed to
correctly separate storage-guard and `PhysicsWorld`-resource-guard scopes
(collect into an owned `Vec`, drop the storage guards, then take the resource
guard). `physics_sync_system`'s sole-occupant placement on `Stage::Physics`
and the scheduler's declared-access coverage for `player_controller_system`
were confirmed. One MEDIUM finding on the crate's two remaining physics-step
helper functions.

### CONC-D5-01: `push_kinematic`/`pull_dynamic` hold Storage read guards across a `PhysicsWorld` resource guard, relying on an unenforced, single-comment convention rather than any structural lock-order guard
- **Severity**: MEDIUM
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `crates/physics/src/sync.rs:688-740` (`push_kinematic`), `crates/physics/src/sync.rs:744-795` (`pull_dynamic`)
- **Status**: NEW
- **Description**: TypeId-sorting does not cover the Resource↔Storage pair, so no `resource_mut::<PhysicsWorld>()` guard may be held across a `query`/`query_mut` iteration and vice-versa. `collect_newcomers`→`register_newcomers` and `apply_buoyancy` both honor this literally (storage reads collected into an owned `Vec`, guards dropped, *then* the `PhysicsWorld` guard is taken). `push_kinematic` and `pull_dynamic` do not: both acquire `RapierHandles`/`RigidBodyData`(/`GlobalTransform`) read guards at the top of the function and keep them alive for the entire body/loop while a `PhysicsWorld` guard (`resource_mut` in `push_kinematic`, `resource` in `pull_dynamic`) is also held — the two lock domains overlap in scope. The only place in the crate documenting "storage-before-resource, consistently" as the actual deadlock-avoidance convention is a comment inside the unrelated `dump_awake_fallers` diagnostic (sync.rs:240-245); it is not stated anywhere `push_kinematic`/`pull_dynamic` themselves live, is not enforced by `lock_tracker`'s always-on same-thread reentrancy check (which is same-lock reentrancy, not cross-lock ordering), and is only checked by the global lock-order graph when `BYRO_LOCK_ORDER_CHECK=1` is explicitly set.
- **Evidence**:
  ```rust
  // sync.rs:688-740 (push_kinematic) — storage reads held across resource_mut
  fn push_kinematic(world: &World) {
      let Some(handles_q) = world.query::<RapierHandles>() else { return; };
      let Some(body_q) = world.query::<RigidBodyData>() else { return; };
      let Some(global_q) = world.query::<GlobalTransform>() else { return; };
      let mut pw = world.resource_mut::<PhysicsWorld>();   // ← taken while the three above are still alive
      for (entity, handles) in handles_q.iter() { ... }
  }
  ```
  ```rust
  // sync.rs:744-795 (pull_dynamic) — same pattern, resource read this time
  fn pull_dynamic(world: &World) {
      let Some(handles_q) = world.query::<RapierHandles>() else { return; };
      let Some(body_q) = world.query::<RigidBodyData>() else { return; };
      let mut updates = Vec::new();
      { let pw = world.resource::<PhysicsWorld>(); for (entity, handles) in handles_q.iter() { ... } }
      drop(handles_q); drop(body_q);
  }
  ```
  Contrast the compliant sibling in the same file, `register_newcomers` (sync.rs:661): `drop(pw);` happens *before* `world.query_mut::<RapierHandles>()` is taken — full separation, not merely consistent ordering.
- **Impact**: Not currently exploitable — `physics_sync_system` is the sole system on `Stage::Physics` (parallel or exclusive), stages execute with a hard barrier between them, the one other `PhysicsWorld`-writing parallel system (`player_controller_system`, `Stage::Early`) fully declares its access so the conflict analyzer would flag a future same-stage collision, and the one other `PhysicsWorld`-touching Late-stage consumer (`ragdoll_writeback_system`) is `add_exclusive`, never in the parallel batch. The risk is latent: nothing but an implicit, single-comment convention prevents a future second parallel `Stage::Physics` system (or a `ragdoll_writeback_system` promoted to parallel) from acquiring `PhysicsWorld` first and then opening `RapierHandles`/`RigidBodyData`/`GlobalTransform`, completing the ABBA cycle against `push_kinematic`/`pull_dynamic` — the same failure class `dump_awake_fallers` was fixed for under #2136, just not generalized.
- **Trigger Conditions**: A future scheduler change adds a second parallel `Stage::Physics` system, or promotes a `PhysicsWorld`-touching exclusive system to parallel, without mirroring the storage-before-resource order; only observable as a hang, and only reliably caught under `BYRO_LOCK_ORDER_CHECK=1` (off by default in normal `cargo test`/CI).
- **Related**: #2136 (the `dump_awake_fallers` fix confirming this is a known, previously-debugged concern in this exact file); thematically parallel to the already-open #2270 ("scripting's 'snapshot before iterate' lock discipline is undocumented as a house rule") — same failure class, different subsystem.
- **Suggested Fix**: Either (a) mirror `apply_buoyancy`'s pattern — collect `(entity, handles)`/`(entity, body_data)` into an owned `Vec` under the read guards, drop them, then take `PhysicsWorld` alone; or (b) if the overlap is kept for its performance benefit, promote the "storage always acquired and held before `PhysicsWorld`" rule out of the one `dump_awake_fallers` comment into a crate-level doc comment on `PhysicsWorld` (`world.rs`) plus a debug-only assertion/`lock_tracker` extension that can detect the reversed order outside `BYRO_LOCK_ORDER_CHECK` runs.

## Dimension 6: Resource Lifecycle (GPU teardown ordering) — CLEAN

No findings. Reverse-order destruction and the #1483 allocator-independent
hoist in `VulkanContext::drop`, the `Arc::try_unwrap` failure/leak fallback
(#665), swapchain-recreate use-after-destroy freedom (including the #2141/
#2142 placeholder-rebind fallbacks), AS shutdown cleanup (`#639`/LIFE-H1
drain-first ordering), per-entity skin-buffer lifetime tied to despawn via
`pending_skin_unload_victims`, and per-frame leak freedom (including
`StagingPool`'s own Drop-time leak detector) were all traced against live
code and confirmed correct — a heavily audited, currently clean surface.

## Dimension 7: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds — CLEAN

No findings. Streaming Drop ordering (#1167, explicit `take()`+join rather
than relying on field-drop order), worker↔main data flow (no `&mut World`
crosses the channel; `assert_send::<PartialNifImport>()` compile-time guard;
`merge_external_material` confirmed main-thread-only at all four call sites;
`NifImportRegistry` write-back deferred to the main-thread drain phase),
the debug server's bounded command queue (`MAX_QUEUED_COMMANDS = 64`) and
screenshot-readback race freedom (#1006/#1007/#1011/#1603), `SharedAllocator`
lock scoping (never held across a queue submit), and `Send`/`Sync` trait
bounds on `Component`/`Resource` were all confirmed against live source.

## Coverage Note

This audit's Dimension 5 explicitly scoped Physics/PHYSAL lock ordering,
which per `.claude/commands/_audit-common.md`'s "un-owned subsystems" table
has no dedicated owner audit skill beyond this one (locks only, not solver
correctness). Ray tracing / AS synchronization (Dimensions 1-2) and ECS core
(Dimension 3-4) are the two areas with the deepest existing regression-guard
history in this codebase and remain so after this pass.
