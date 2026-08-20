# Concurrency Audit — 2026-08-20

**Command**: `/audit-concurrency` (all 7 dimensions, `--depth deep`), run as part
of the `comprehensive` audit-suite sweep.

**Repo state**: HEAD `bb0b92f2`, branch `main`. Delta baseline: 335 commits since
the 2026-08-16 sweep (`85b77371`), overwhelmingly session-70 WATAL water work
plus terrain-LOD streaming, volumetrics and CHARAL wiring.

**Dedup inputs**: `/tmp/audit/issues.json` (400 issues, #2671–#3103, all states),
`docs/audits/AUDIT_CONCURRENCY_2026-08-16.md` and its older siblings.

## Scope and weighting

The dispatch asked for delta weighting, and that is what this sweep did:

- **Dim 1/2 (Vulkan queue + AS sync)** — the session-70 water/volumetrics passes:
  `crates/renderer/src/vulkan/water.rs` (+540), `volumetrics.rs` (+1316),
  `context/draw.rs` (+180), `context/geometry_pass.rs` (+122),
  `context/post_passes.rs` (+63), `context/resize.rs`, `caustic.rs`.
- **Dim 3/4 (lock ordering + access declarations)** — every system function
  touched or added in the delta: `byroredux/src/systems/water.rs` (new),
  `weather.rs`, `billboard.rs`, `character.rs`, `audio.rs`, plus every
  `add_to_with_access` / `add_exclusive_with_access` registration in
  `byroredux/src/boot.rs`.
- **Dim 5 (Resource↔Storage / physics)** — `crates/physics/src/water.rs` (+541,
  the new WATAL buoyancy sink), `crates/physics/src/sync.rs` (+175),
  `byroredux/src/render/water.rs` (the renderer-side `WaterFlow` reader).
- **Dim 7 (worker threads)** — `byroredux/src/streaming.rs` (+148, the #3089
  dedicated rayon pool + LOD-water recenter), plus a fresh whole-workspace
  thread inventory.

**The ECS core machinery is byte-identical to the 2026-08-16 sweep.**
`git diff 85b77371..HEAD -- crates/core/src/ecs/{world,query,lock_tracker,scheduler,access}.rs`
is empty. Every Dimension-3/4 machinery guard that report verified
(TypeId-sorted acquisition in `query_2_mut`/`query_2_mut_mut`, the same-type
`assert_ne!`, always-on same-thread reentrancy detection, `BYRO_LOCK_ORDER_CHECK`
gating of the cross-thread graph, the three-variant `AccessConflict` enum,
per-resource `RwLock`) therefore carries forward unchanged and was not
re-derived here. This sweep's Dimension 3/4 effort went entirely into the
*callers* — which is where the delta is.

## ⚠ Verification status

**No Vulkan device, no captured `BYRO_VALIDATION` run, and no RenderDoc capture
backed any Dimension 1, 2 or 6 verdict.** Those are source-read only, and per
the skill's speculative-fix guardrail **no barrier / stage-mask / layout change
is proposed anywhere in this report**. No `cargo` command was run (suite rule).

## Executive Summary

**0 CRITICAL · 1 HIGH · 1 MEDIUM · 1 LOW** (new), plus 2 LOW carried forward
unchanged from 2026-08-16.

| Dimension | Area | Result |
|---|---|---|
| 1 | Vulkan Queue & AS Sync | CLEAN — all 12 prior guards re-verified at HEAD |
| 2 | Compute → AS → Fragment Chains | CLEAN — latch set is now 3-wide and symmetric; new water-params latch is sound |
| 3 | ECS Lock Ordering & Deadlock | CLEAN — the new water/weather/billboard systems all drop before escalating |
| 4 | Scheduler Access Declarations | **1 HIGH, 1 MEDIUM, 1 LOW** — three incomplete declarations, one with a live parallel writer |
| 5 | RwLock: Resource↔Storage & Physics | CLEAN — and **#2404 is now fixed** |
| 6 | Resource Lifecycle (GPU teardown) | CLEAN — the two new allocator-holding subsystems release their Arc clones in `destroy()` |
| 7 | Worker Threads (Streaming, Debug Server) | CLEAN — **CONC-2026-08-16-01 fixed by #3089**; two prior LOWs unchanged |

### The one HIGH in one line

`player_controller_system` acquired a whole new water-sampling surface this
session and its `Access` declaration was updated to match — except for one
resource it reaches *transitively*, through
`byroredux_physics::weather_wave_adjustment`: **`WindField`**. `weather_system`
sits in the same `Stage::Early` parallel batch and **writes** that resource. The
declared-access analyzer cannot see the read, so `known_conflict_count()` stays
0 and the `debug_assert_eq!` in `install_runtime_registries` never fires — while
two rayon workers genuinely touch the same resource with a write, every frame
the player is anywhere near water.

This is the same defect family as #1787 (`ContactConfig` on
`physics_sync_system`) and #2676 (`PlayerMode` on `camera_follow_system`), both
of which the project filed and fixed. It is strictly worse than either, because
both of those had *no* parallel writer; this one does.

---

## Findings

### CONC-2026-08-20-01: `player_controller_system` reads `WindField` undeclared while `weather_system` writes it in the same `Stage::Early` parallel batch

- **Severity**: HIGH
- **Dimension**: Scheduler Access Declarations (regression guard) — with a live
  Dimension-3 consequence
- **Location**: declaration `byroredux/src/boot.rs:691-724`; the undeclared read
  `byroredux/src/systems/character.rs:912-916` →
  `crates/physics/src/water.rs:323-327`; the parallel writer
  `byroredux/src/systems/weather.rs:690-691`, declared at
  `byroredux/src/boot.rs:726-741`
- **Status**: NEW (same family as #1787 / #2676, both CLOSED; neither names this
  system or this resource)
- **Trigger Conditions**: Any frame in which (a) the player controller is the
  active `PlayerMode` branch, (b) the player capsule's XZ column intersects a
  `WaterVolume` — i.e. standing in or wading through water — and (c) rayon
  schedules `weather_system` and `player_controller_system` onto different
  workers, which is the normal case for a 3-system parallel batch on a 16-core
  machine. The write is only *interesting* on frames where the WTHR wind byte
  changes (weather transition, worldspace entry), but the unsynchronised
  read/write pair exists on every such frame regardless.
- **Description**: `Stage::Early` holds exactly three parallel systems
  (`boot.rs:692`, `:727`, `:743`): `player_controller_system`, `weather_system`,
  `timer_tick_system`. `weather_system` declares
  `.reads_resource::<WindField>().writes_resource::<WindField>()` and does write
  it (`weather.rs:690`). `player_controller_system` dispatches to
  `character_controller_system`, whose new water-sampling helper
  `player_water_state` calls `byroredux_physics::weather_wave_adjustment(world, …)`,
  and that function's first statement is `world.try_resource::<WindField>()`.

  The session-70 commit that added the water sampling *did* extend the
  declaration — `TotalTime`, `ActorVitals`, `WaterPlane`, `WaterVolume`,
  `WaterFlow`, `ActorValues`, `Dead` were all added in this delta
  (`git diff 85b77371..HEAD -- byroredux/src/boot.rs`). `WindField` was missed
  because it is not named anywhere in `character.rs` — it is reached one call
  frame down, inside the physics crate.

  The consequence is not memory-unsafety (the per-resource `RwLock` at
  `world.rs:61` serialises the two accesses) and it is **not a deadlock** — I
  checked for a cycle and there is none: `weather_system` takes the `WindField`
  write guard standalone, holding nothing else, while `player_controller_system`
  holds `WaterPlane`/`WaterVolume`/`WaterFlow` storage reads plus a `TotalTime`
  resource read when it asks for `WindField`. No edge runs the other way.

  What is broken is the invariant the whole parallel scheduler rests on. Had
  `WindField` been declared, `analyze_pair` would classify the
  `(weather_system, player_controller_system)` pair as
  `Conflict { pairs }` on a write/read overlap, and the
  `debug_assert_eq!(report_snapshot.known_conflict_count(), 0, …)` at
  `boot.rs:1449-1454` would abort construction in debug builds with exactly the
  message written for this case. The missing declaration is what keeps that
  assertion green. Every downstream reader of `sys.accesses` — including the
  next audit — is told this batch is conflict-free when it is not.
- **Evidence**:
  ```rust
  // byroredux/src/systems/character.rs:912-916 — inside player_water_state,
  // reached from character_controller_system, reached from
  // player_controller_system (Stage::Early, parallel).
  let wave_height = world
      .try_resource::<TotalTime>()
      .map(|time| {
          let (weather_scroll, wind_wave_scale) =
              byroredux_physics::weather_wave_adjustment(world, time.0);
  ```
  ```rust
  // crates/physics/src/water.rs:323-327
  pub fn weather_wave_adjustment(world: &World, time_secs: f32) -> ([f32; 2], f32) {
      let wind = world
          .try_resource::<WindField>()      // ← the undeclared read
          .map(|field| *field)
          .unwrap_or_default();
  ```
  ```rust
  // byroredux/src/systems/weather.rs:690-691 — Stage::Early, same parallel batch
  if let Some(mut wind) = world.try_resource_mut::<WindField>() {
      *wind = WindField::from_weather_byte(weather_wind_speed, wind.direction);
  }
  ```
  `grep -c WindField` over `boot.rs:691-724` (the whole
  `player_controller_system` `Access::new()` chain) returns **0**. The chain
  declares 12 resources and 10 components; `WindField` is not among them.
- **Impact**: Two failure modes, one structural and one observable.

  *Structural (the reason this is HIGH):* the M27 access model's core promise —
  "`known_conflict_count() == 0` ⇒ no two parallel same-stage systems touch the
  same component or resource with a write" — is false at HEAD, and the guard
  built to enforce it (`boot.rs:1449`) cannot see the violation. Dimension 3 of
  this very audit uses that promise to argue cross-thread ABBA is structurally
  unreachable among parallel systems. Every future system added to
  `Stage::Early` is analysed against an incomplete picture.

  *Observable:* the player's water surface height is computed from wind that may
  be either this frame's or last frame's, non-deterministically per frame and
  per machine, on the exact frames a weather transition is in flight. Magnitude
  is one wave-amplitude step (`wind_wave_scale` spans 1.0–1.5), so it is small
  and transient — but it feeds `swimlevel_reached`, which is a *boolean* state
  transition (walk ↔ swim). At the swim threshold a sub-frame wind difference is
  enough to flip it, and a flip that alternates frame-to-frame is a visible
  controller-state strobe. Blast radius is one system, but it is the player's.
- **Related**: #1787 / CONC-D4-01 (`ContactConfig` undeclared on
  `physics_sync_system` — same shape, no parallel writer, fixed). #2676 /
  CONC-D3-NEW-02 (`PlayerMode` undeclared on `camera_follow_system` — same
  shape, no parallel writer, fixed; the in-code comment at `boot.rs:1276-1290`
  spells out this exact rationale). CONC-2026-08-20-02 and -03 below are the
  same defect on two other systems, neither with a live writer. Not a duplicate
  of any of them — different system, different resource, and the only one of the
  four with a concurrent writer.
- **Suggested Fix**: Add `.reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()`
  to the `player_controller_system` `Access` chain — then **expect the debug
  assertion at `boot.rs:1449` to fire**, because that is the true state of the
  batch. Resolving it needs a scheduling decision, not another declaration; the
  two options the codebase already uses elsewhere are (a) move `weather_system`
  to `add_exclusive_with_access` in `Stage::Early` (it is a once-per-frame
  resource-only system with no measurable parallel benefit — `spin_system` and
  `audio_system` took this route in M27 Phase 3), or (b) hoist the WTHR wind
  update into its own earlier stage so the write is complete before any reader
  runs. (a) is the smaller change and matches the precedent.

---

### CONC-2026-08-20-02: `physics_sync_system`'s WATAL buoyancy phase reads `TotalTime`, `WindField` and `WaterCurrentVolume` undeclared

- **Severity**: MEDIUM
- **Dimension**: Scheduler Access Declarations (regression guard)
- **Location**: declaration `byroredux/src/boot.rs:1233-1290`; the undeclared
  reads `crates/physics/src/water.rs:476` (`TotalTime`), `:478` and `:325`
  (`WindField`), `:378-383` (`WaterCurrentVolume`)
- **Status**: NEW
- **Trigger Conditions**: None for a live race — `Stage::Physics` holds exactly
  one system (`grep -n "Stage::Physics" byroredux/src/boot.rs` → one
  registration at `:1235`), and stages run sequentially
  (`scheduler.rs`), so nothing can currently overlap it. This is a
  latent-contract defect, not a live one.
- **Description**: The WATAL buoyancy phase added in this delta
  (`apply_buoyancy`, `crates/physics/src/water.rs:465`) reads three things the
  `physics_sync_system` `Access` chain does not name. Two are resources
  (`TotalTime` at `:476`, `WindField` at `:478` and again transitively through
  `weather_wave_adjustment` at `:325`); one is a component storage
  (`WaterCurrentVolume`, a real `SparseSetStorage` component —
  `crates/core/src/ecs/components/water.rs:510-517` — read by
  `collect_water_current_volumes` at `:378-383`).

  The declaration was clearly extended for WATAL in this same delta: it gained
  `PhysicsWaterConstants`, `WaterPlane`, `WaterVolume`, `WaterFlow` and
  `WaterContact`, each with a comment explaining why. The three above were
  missed. `WaterCurrentVolume` is the easiest to miss because the placed-XWCU
  current-volume path is a separate collector from the water-surface one;
  `WindField` is missed for the same transitive reason as
  CONC-2026-08-20-01.

  I want to be precise about why this is MEDIUM and not HIGH: there is no
  parallel counterparty today, so nothing races and nothing is non-deterministic.
  The damage is that the declaration is the contract a *future* Physics-stage
  system is analysed against, and the analyzer would clear a wind-writing or
  current-volume-writing sibling as conflict-free. That is precisely the
  argument the codebase itself makes in the `#1787 / CONC-D4-01` comment
  already sitting six lines above the gap ("must be declared so a future
  parallel system that writes it is caught by the conflict analyzer instead of
  silently racing").
- **Evidence**:
  ```rust
  // crates/physics/src/water.rs:474-482 — apply_buoyancy, Stage::Physics
  let surfaces = collect_water_surfaces(world);
  let current_volumes = collect_water_current_volumes(world);   // ← WaterCurrentVolume
  let time_secs = world.try_resource::<TotalTime>().map(|time| time.0);   // ← TotalTime
  let atmospheric_wind = world
      .try_resource::<WindField>()                              // ← WindField
      .map(|wind| *wind)
      .unwrap_or_default();
  ```
  ```rust
  // crates/core/src/ecs/components/water.rs:515-517 — it is a real storage
  impl Component for WaterCurrentVolume {
      type Storage = SparseSetStorage<Self>;
  }
  ```
  The `Access` chain at `boot.rs:1233-1290` declares `PhysicsWorld` (r/w),
  `PhysicsWaterConstants`, `ContactConfig`, `FormIdPool`, and the components
  `CollisionShape`, `RigidBodyData`, `GlobalTransform`, `RapierHandles` (r/w),
  `Transform` (w), `WaterPlane`, `WaterVolume`, `WaterFlow`, `WaterContact` (w),
  `RenderLayer`, `FormIdComponent`, `PhysicsSourceForm`. None of the three
  above appear.
- **Impact**: No live race. `sys.accesses` under-reports this system's read
  surface by three entries, and the analyzer will mis-clear any future
  `Stage::Physics` sibling that writes wind, the engine clock, or placed current
  volumes. Contract/observability defect with a real future-race enabling
  property — which is exactly what #1787 was filed and fixed for.
- **Related**: #1787 / CONC-D4-01, CONC-2026-08-20-01 (same family, live
  writer), CONC-2026-08-20-03 (same family, exclusive system).
- **Suggested Fix**: Add `.reads_resource::<byroredux_core::ecs::resources::TotalTime>()`,
  `.reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()`
  and `.reads::<byroredux_core::ecs::components::water::WaterCurrentVolume>()`
  to the `physics_sync_system` chain, alongside the existing WATAL entries.
  Unlike -01 this addition cannot trip `known_conflict_count()` — the stage has
  no second system to pair against.

---

### CONC-2026-08-20-03: `make_billboard_system` reads `TotalTime` undeclared

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations (regression guard)
- **Location**: declaration `byroredux/src/boot.rs:1182-1190`; the undeclared
  read `byroredux/src/systems/billboard.rs:50-53`
- **Status**: NEW
- **Trigger Conditions**: None — the system is registered
  `add_exclusive_with_access`, and exclusive systems run serially after their
  stage's parallel batch, so they are never paired by the analyzer and never
  overlap anything.
- **Description**: This delta gave the SpeedTree gust phase a shared clock so
  foliage and water cannot drift out of phase — `billboard.rs:50` now reads
  `TotalTime` (falling back to the closure-local accumulator for synthetic test
  worlds that skip the registration). The `Access` chain gained
  `WindField`, `SpeedTreeWind` and `MeshHandle` in the same session but not
  `TotalTime`.

  Reported at LOW rather than folded into -02 because the reasoning for
  declaring it is *only* the documentation one, and the codebase has already
  written that reasoning down: the comment at `boot.rs:1173-1180` explains that
  these three PostUpdate exclusives declare access despite the analyzer not
  pairing exclusives, precisely because "a blank `sys.accesses` row is exactly
  the wrong place for [the ordering contract] to be invisible" (#2391). A row
  that is present but three-quarters complete is a weaker version of the same
  problem.
- **Evidence**:
  ```rust
  // byroredux/src/systems/billboard.rs:50-53
  let wind_time = world
      .try_resource::<TotalTime>()
      .map(|time| time.0)
      .unwrap_or(elapsed);
  ```
  The chain at `boot.rs:1184-1190` is `ActiveCamera`, `WindField`, `Billboard`,
  `SpeedTreeWind`, `MeshHandle`, `GlobalTransform` (w). No `TotalTime`.
- **Impact**: Documentation / `sys.accesses` completeness only. No live or
  latent race — the system is exclusive and `TotalTime` has exactly one writer
  (the engine's own per-frame tick, outside the scheduler).
- **Related**: #2391 (the reason these exclusives declare access at all),
  CONC-2026-08-20-01, -02.
- **Suggested Fix**: One line —
  `.reads_resource::<byroredux_core::ecs::resources::TotalTime>()` on the
  `make_billboard_system` chain. Its `submersion_system` sibling twelve lines
  below already declares exactly this.

---

## Carried forward from 2026-08-16 (verified still present, NOT re-reported as new)

- **CONC-2026-08-16-02** (LOW) — the cancelled-screenshot arm in
  `crates/debug-server/src/system.rs:72-78` still `return`s from `System::run`
  instead of falling through to the command drain, so one frame of unrelated
  queued commands is deferred. **Unchanged since 2026-08-16.** Re-read at HEAD;
  the `return` is still there and the three sibling arms still fall through.
- **CONC-2026-08-16-03** (LOW) — `pre_parse_cell`'s doc comment is still split
  in half by the `parse_one_nif` extraction. At HEAD the head block ends
  mid-sentence at `streaming.rs:1124` ("…which may have an empty") and the
  completing clause still sits orphaned at `:1172-1174`, now separated by an
  even longer `parse_one_nif` doc block. **Unchanged since 2026-08-16.**

## Prior findings verified FIXED

- **CONC-2026-08-16-01** (MEDIUM, → **#3089, CLOSED**) — the streaming
  pre-parse worker no longer shares rayon's global pool with the ECS scheduler.
  `build_stream_parse_pool` (`streaming.rs:1017-1027`) builds a dedicated
  `rayon::ThreadPool` sized `max(cores/2, 1)` with named
  `byro-stream-parse-{i}` threads, constructed **once per worker-thread
  lifetime** (`:1051`, outside the request loop, not per request), and the
  Phase-2 fan-out runs inside `stream_pool.install(…)` at `:1318`. Verified
  fixed exactly as recommended.
- **#2404** (`push_kinematic` / `pull_dynamic` holding storage read guards
  across the `PhysicsWorld` guard) — **FIXED in this delta.** Both now snapshot
  under the read guards, `drop()` them explicitly, and only then take the
  resource guard: `sync.rs:936-953` (`push_kinematic`) and `:991-1008`
  (`pull_dynamic`), each carrying an in-code `(#2404)` comment. This was listed
  as an OPEN skip in the 2026-08-16 report; it is no longer skippable and no
  longer a defect.

## Guards verified intact

Each was actively re-checked at `bb0b92f2`, not assumed from the prior report.
Only the ones whose files changed in the delta are listed — the unchanged ECS
core machinery is covered by the note in *Scope* above.

### Dimension 1 — Vulkan queue & AS sync (`draw.rs` +180, `resize.rs`, `acceleration/`)

1. **Queue-Mutex discipline (#284 / CONC-D2-NEW-01).** `draw.rs:3620-3634` still
   binds the `MutexGuard` to a `let` and derefs inside `queue_submit`, with the
   VUID-vkQueueSubmit-queue-00893 rationale intact; the `drop(queue)` on the
   failure arm (`:3642`) still precedes the semaphore recovery.
2. **Frame-in-flight discipline.** `draw.rs:1476` still waits both `in_flight`
   slots before any per-frame resource is reused; the per-image `image_fence`
   wait at `:1595` is unchanged.
3. **`reset_fences` immediately before `queue_submit`** — `:3600`, with both
   failure arms recovering `image_available[frame]`.
4. **AS build → ray-query barrier on BOTH `build_tlas` arms (#2931 / CON-D2-01).**
   `draw.rs:2522-2530` — `ACCELERATION_STRUCTURE_BUILD_KHR` /
   `ACCELERATION_STRUCTURE_WRITE_KHR` → `FRAGMENT_SHADER | COMPUTE_SHADER` /
   `ACCELERATION_STRUCTURE_READ_KHR`, still **outside** the `if !tlas_build_failed`
   branch that begins at `:2532`, with the comment explaining that it also
   publishes `record_skinned_blas_refit`. This is the guard most at risk from a
   180-line diff through this function; it survived intact.
5. **Cluster-cull HOST→COMPUTE and COMPUTE→FRAGMENT pair** — `draw.rs:2591` and
   `:2609`, unchanged.
6. **Swapchain recreate.** `context/resize.rs` — the water pipeline rebuild at
   `:342-364` is inside the render-pass-changed block, downstream of
   `recreate_swapchain_core`'s leading `device_wait_idle`, and the in-code
   SAFETY comment at `:343-346` states that dependency explicitly.

### Dimension 2 — compute → AS → fragment chains

1. **Volumetrics latch symmetry (#1105) — now three latches, all symmetric.**
   The delta added `boundary_geometry_written` alongside `tlas_written` and
   `lights_written`. All three are declared (`volumetrics.rs:817`, `:821`,
   `:825`), initialised in `new()` (`:896-898`), `debug_assert!`ed and reset in
   `dispatch` (`:1994-2014`), and set by their respective writers (`:2565`,
   `:2621`, `:2666`). No asymmetry.
2. **Volumetrics gate is single-`if let`.** `post_passes.rs:538-549` — all three
   `write_*` calls and the `dispatch` sit inside the *same* `if let (Some(tlas),
   Some(..), Some(..))`, and the `else` arm calls `record_neutral_frame`. A
   frame cannot reach `dispatch` with any of the three descriptor writes
   skipped.
3. **New COMPUTE→COMPUTE cluster-buffer barrier.** `post_passes.rs:585-592`,
   with a correct in-code rationale: `cluster_cull`'s trailing barrier in
   `draw_frame` targets `FRAGMENT_SHADER` only, and this later compute read is
   not covered by it. Correct per spec (source read; not validation-confirmed).
4. **New water-params latch.** `water.rs:518-552` — `upload_params` clears
   `params_ready[frame]` on entry, sets it only after
   `param_buffers[frame].write_mapped` returns `Ok`, and the geometry pass gates
   the entire water block on `water.params_ready(frame)`
   (`geometry_pass.rs:522`). The call site (`draw.rs:3371-3375`) sits **before**
   the shared HOST→SHADER barrier at `:3413` and before `record_geometry_pass`
   at `:3443`, so the host write is published. The per-draw index is bounded by
   `water_commands.iter().take(MAX_WATER_DRAWS)` on the consumer side
   (`geometry_pass.rs:527-528`), matching the producer's own truncation — no
   shader index can escape the UBO.
5. **New selected-ray probe readback (`render_debug.rs`).** Full round trip is
   correctly ordered: `arm_selected_ray_probe` (`draw.rs:3388`, host write) →
   HOST_WRITE→SHADER barrier widened with `SHADER_WRITE` (`:3413-3423`) →
   `record_geometry_pass` (`:3443`) → FRAGMENT_SHADER/SHADER_WRITE →
   HOST/HOST_READ barrier (`:3461-3469`) → next use of the slot waits the fence
   (`:1476`) → `collect_selected_ray_probe` (`:1506`), which calls
   `buffer.invalidate_if_needed(device)` before `mapped_slice_mut` and uses
   `read_unaligned` rather than a misaligned reference
   (`scene_buffer/descriptors.rs:267-277`). Same shape as the #2740
   host-readback discipline.

### Dimension 3 — ECS lock ordering, the new water/weather/billboard systems

The dispatch specifically asked whether any newly-added multi-component query
grabs two component locks out of order. **It does not.** Every new site either
uses the TypeId-sorting accessor or drops before escalating:

1. `submersion_system` (`systems/water.rs:245`) uses
   `world.query_2_mut::<WaterVolume, ParticleEmitter>()` — the sorting
   accessor, not two hand-rolled guards.
2. `water_damage_system` (`systems/water.rs:31-51`) takes three read guards
   (`WaterContact`, `ActorVitals`, `Dead`), collects to a `Vec`, then
   `drop()`s all three explicitly at `:49-51` before touching
   `query_mut::<ActorValues>()` at `:55` and `query_mut::<Dead>()` at `:65`.
   The `Dead` read/write pair is the interesting one — a live `dead_q` read
   guard held into the `query_mut::<Dead>()` would be a same-thread
   self-deadlock that `lock_tracker::track_write` would (correctly) panic on;
   the explicit `drop` at `:51` is what prevents it, and it is the right kind
   of explicit.
3. `submersion_system` drops `wq`/`vq` at `:236-237` before the
   `query_2_mut` block, and takes `query_mut::<SubmersionState>()` only after
   that block closes.
4. `make_water_interaction_system` (`systems/water.rs:359-424`) drops
   `global_q`/`contact_q` at `:406-407` before the `RippleEvent` /
   `SplashEvent` writes.
5. `make_billboard_system` acquires `GlobalTransform` (write) → `Billboard`
   (read) → `SpeedTreeWind` (read) → `MeshHandle` (read) in source order with
   no TypeId sort — but it is `add_exclusive_with_access`, so it never overlaps
   another system, and #829's single-write-query design is what removed the
   earlier read-then-write cycle on `GlobalTransform`. Not a finding; recorded
   so the next sweep does not re-derive it.
6. `weather_system` touches **no component storage at all** — it is
   resource-only. Its deepest nesting is `WeatherDataRes` (read, held
   `:456-676`) across `WeatherTransitionRes` (read, `:535`/`:655`/`:666`) and
   `CellLightingRes` (write, `:466`). Both inner resources are also taken
   standalone elsewhere in the same body, always *after* `drop(wd)` at `:676` —
   never in the reverse nesting. No ABBA edge, and no other Early-batch system
   touches any of those four resources.

### Dimension 5 — physics / Resource↔Storage, the new WATAL sink

1. **`apply_buoyancy` (`crates/physics/src/water.rs:465-812`) is disciplined
   throughout.** Three separate escalations, each clean: the quiesced-scene
   early-out scopes its `world.resource::<PhysicsWorld>()` read guard to a bare
   block (`:509-517`); the target-gathering phase drops `handles_q` / `body_q` /
   `contact_q` at `:571-573` before `resource_mut::<PhysicsWorld>()` at `:583`;
   and the `PhysicsWorld` write guard's block closes at `:798` before
   `world.query_mut::<WaterContact>()` at `:802`. The `writes` `Vec` exists
   precisely to carry data across that boundary.
2. **`clear_stale_water_contacts` (`:389-436`)** mirrors it — three read guards,
   `restore` `Vec`, explicit `drop`s at `:415-417`, then `PhysicsWorld` write,
   then `WaterContact` write.
3. **`collect_water_surfaces` (`:355-376`)** holds `WaterPlane` + `WaterVolume`
   + `WaterFlow` read guards simultaneously in source order — all reads, on a
   stage with one system. Same shape as `player_water_state`
   (`character.rs:893-896`) and `submersion_system`. No write of any of the
   three exists in any parallel system, so there is no reader-blocked-behind-
   queued-writer path to close a cycle with.
4. **The renderer-side `WaterFlow` reader is not a second lock domain.**
   `byroredux/src/render/water.rs:141-145` takes `WaterPlane` / `WaterFlow` /
   `RippleEvent` read guards, and every one of its five resource reads
   (`TotalTime`, `WindField`, `WeatherDataRes` ×2, `GameTimeRes`, `:73-91`) is
   statement-scoped through a `.map(…).unwrap_or(…)` chain, so no resource
   guard is alive when the storage guards are taken. It also runs inside
   `build_render_data` on the main thread, outside `Scheduler::run` entirely —
   it cannot overlap `apply_buoyancy`. The dispatch's concern here is
   unfounded at HEAD.
5. **`ContactConfig`** still snapshotted once per batch in `register_newcomers`,
   not re-locked per newcomer.
6. **Single-threaded placement.** `Stage::Physics` contains exactly one
   registration (`boot.rs:1235`); nothing can be co-scheduled with
   `physics_sync_system`.
7. **`pull_dynamic`'s `Parent`/`GlobalTransform`/`Transform` read trio
   (`sync.rs:1041-1043`) is the reverse edge of `transform_propagation_system`'s
   `Transform`(write)-held-across-`Parent`/`GlobalTransform`.** Checked
   deliberately because it looks like a textbook ABBA. It is structurally
   unreachable: propagation is `Stage::PostUpdate` exclusive, `physics_sync` is
   `Stage::Physics` parallel, and stages run sequentially. The in-code comment
   at `:1038-1040` says exactly this. Not a finding — recorded so it is not
   re-derived.

### Dimension 6 — GPU teardown, the two new allocator-holding subsystems

1. **`WaterPipeline` now holds its own `allocator: Option<SharedAllocator>`**
   (`water.rs:261`) because `param_buffers` is new this session. Its `destroy`
   (`:697-728`) frees every `param_buffer` through that clone and then sets
   `self.allocator = None`, releasing the `Arc` clone. `VulkanContext::Drop`
   reaches `w.destroy(&self.device)` ~120 lines before
   `Arc::try_unwrap(alloc_arc)`, so the #665 outstanding-reference leak guard
   cannot fire on account of water. The resize path
   (`resize.rs:342-348`) destroys the old pipeline before constructing the new
   one, so the clone count stays at one.
2. **`VolumetricsPipeline::destroy` (`volumetrics.rs:2676-2765`) covers every
   new field.** All six image `Vec`s (`lighting_volumes`,
   `emission_history_volumes`, `combustion_state_volumes`,
   `combustion_dynamics_volumes`, `combustion_optical_volumes`,
   `integrated_volumes`) plus both noise volumes are drained through one chain;
   all six `GpuBuffer` `Vec`s are destroyed **and then `.clear()`ed** with the
   #732 LIFE-N1 comment explaining that the `clear()` is what releases each
   buffer's own `Arc` clone before `try_unwrap`. Both pipelines, both layouts,
   both descriptor pools/layouts and all three samplers are null-guarded.
   Cross-checked field-by-field against the struct declaration at `:761-801`
   and the `new_inner` initialiser at `:854-891`. No field is missed.

### Dimension 7 — worker threads

1. **Thread inventory is unchanged.** `grep` for `thread::spawn` /
   `thread::Builder` across the whole workspace returns five hits, of which two
   are inside `#[cfg(test)]` modules (`crates/core/src/ecs/resources/mod.rs:1574`,
   after the `#[cfg(test)]` at `:1078`; `crates/papyrus/src/parser/script.rs:1244`,
   after the one at `:812`). The three production spawns are the same three as
   2026-08-16: `streaming.rs:752` (cell worker), `debug-server/src/listener.rs:169`
   (listener) and `:228` (per-client). New this delta: the named
   `byro-stream-parse-{i}` rayon pool, which is owned by and confined to the
   cell worker thread.
2. **#1167 Drop ordering** unchanged — `shutdown` still takes `worker` then
   `request_tx`, `Drop` delegates.
3. **`recenter_lod_water` (`streaming.rs:816-833`) takes `&mut World`**, so it
   is a structural-mutation-capable main-thread call and cannot run inside a
   system body. Its single `query_mut::<Transform>()` guard closes with the
   `if let` block. The new `ChurnTracker` fields are plain owned state on
   `WorldStreamingState`, no interior mutability.
4. **`merge_external_material` still unreachable from the worker.** The
   transitive callee set out of `parse_one_nif` is unchanged; `MaterialProvider`
   and its caches are never touched off the main thread.

---

## Candidates considered and NOT reported

Recorded so a later sweep does not re-derive them.

1. **`weather_system` holds `WeatherDataRes` (read) across a `CellLightingRes`
   write and two `WeatherTransitionRes` reads, with no ordering discipline
   between resource locks.** Real nesting, but not a defect: no other system in
   any stage touches those four resources, so there is no counterparty to form
   a cycle with, and the reverse order never occurs within `weather_system`
   itself.
2. **`water_audio_system` holds an `ActiveCamera` resource guard across
   `world.query::<SubmersionState>()` (`systems/audio.rs:214-221`)** — an
   unordered Resource↔Storage pair, the class Dimension 5 exists to police.
   Disproved as a live defect: the system is `add_exclusive_with_access`
   (`boot.rs:1169`), so it never overlaps anything, and both accesses are reads.
3. **`submersion_system` holds `WaterPlane` + `WaterVolume` storage reads *and*
   a `TotalTime` resource guard across a re-entrant call into the World
   (`weather_wave_adjustment`, `systems/water.rs:176-182`).** The call does
   re-enter `World`, but only to take a `WindField` read — a fourth, distinct
   lock, never one already held, so no same-thread reentrancy panic. And the
   system is exclusive. Worth remembering if it is ever promoted to a parallel
   batch.
4. **`WaterPipeline::destroy` leaves `params_ready` as an empty `Vec`, so a
   later `params_ready(frame)` would panic on index-out-of-bounds.**
   Disproved as reachable: both call sites (`Drop`, `resize`) either drop or
   immediately replace the pipeline, and `params_ready` is only read from
   `geometry_pass.rs:522` which is gated on `self.water.is_some()` naming the
   live pipeline.
5. **The volumetrics inject compute now reads the global vertex/index SSBOs
   (bindings 19/20/21), which `rebuild_geometry_ssbo` can reallocate.**
   Disproved: `write_boundary_geometry` re-points all three descriptors from
   the *current* `mesh_registry` handles every gated frame
   (`post_passes.rs:552-561`), and the rebuild runs in `render_one_frame`
   before `draw_frame`, so a stale handle cannot survive into a dispatch.
6. **`build_stream_parse_pool` `.expect()`s on pool-construction failure, on
   the worker thread, outside `pre_parse_cell_panic_safe`.** A panic there
   would kill the worker silently at startup. Not reported: the surviving
   failure path is benign (the `Receiver` drops, `request_load` starts returning
   `Err`, which the caller already handles), and `ThreadPoolBuilder::build` with
   an explicit `num_threads` has no realistic failure mode.
7. **`render/water.rs:60-140` re-implements `weather_wave_adjustment`'s gust /
   direction / scale math inline instead of calling it**, so the renderer and
   the physics/gameplay sampler can silently diverge. Real, and worth filing —
   but it is a duplication/divergence defect, not a concurrency one. Out of
   dimension; belongs to `/audit-tech-debt` or `/audit-renderer`.
8. **`render/water.rs` acquires `WeatherDataRes` twice in eight lines
   (`:84`, `:91`) rather than once.** Two sequential, non-overlapping read
   guards. Micro-inefficiency, not a lock-order or lifetime defect.

## Skill-text drift (not code defects, not re-filed)

The Dimension-1 bullet in `.claude/commands/audit-concurrency/SKILL.md` still
says to "confirm the guard is **not** held across `queue_submit`". The code
deliberately does the opposite and is correct to —
VUID-vkQueueSubmit-queue-00893 requires the queue to be externally synchronised
*for the duration of the call*, which is what holding the guard achieves, and
`draw.rs:3621-3625` carries an in-code comment saying so. Already filed as
skill-text drift by the 2026-08-12 report (§5.1) and again by 2026-08-16; noted
a third time only so the next reader of that bullet does not "fix" working code.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-20.md
```

TALLY: CRITICAL=0 HIGH=1 MEDIUM=1 LOW=1
