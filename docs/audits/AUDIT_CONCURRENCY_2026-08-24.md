# Concurrency and Synchronization Audit — 2026-08-24

**Scope**: Full comprehensive run, all 7 dimensions, `--depth deep` (default). No `--focus` filter.
**Audited at**: `HEAD = 048a8bd8` (lock_tracker rewrite landed one commit earlier, `5428e872`; GPU morph-target deformation, #3231 Phase D, landed the day before this audit in `d0322785`).

## Build-environment note

A same-day workspace build break was reported by another audit (`E0004` non-exhaustive
match in `crates/scripting/examples/fragment_coverage.rs:59`). This audit's scope
(`crates/core`, `crates/renderer`, `crates/physics`, `crates/debug-server`,
`crates/debug-protocol`, `byroredux`) does not depend on `crates/scripting`'s example
target. Verified before starting:

```
cargo check -p byroredux-core                                            # clean
cargo check -p byroredux-renderer                                        # clean
cargo check -p byroredux-physics -p byroredux-debug-server -p byroredux-debug-protocol   # clean
```

Per-dimension work additionally ran `cargo test -p byroredux --bin byroredux
scheduler_access_tests` (Dimension 4, 14/14 pass). No workaround was needed — the
break did not block any check this audit performs.

## Executive Summary

**Total: 10 findings — 0 CRITICAL, 2 HIGH, 4 MEDIUM, 4 LOW** (9 NEW + 1 Existing/regression-confirmed).

| # | ID | Severity | Dimension | Status | One-line |
|---|----|----|----|----|----|
| 1 | CONC-D2-2026-08-24-01 | HIGH | Compute → AS → Fragment Chains | NEW | `MorphSlot::weight_buffer` is single-buffered and host-written before the frame's fence wait — WAR race against the previous frame's still-in-flight `skin_vertices.comp` read |
| 2 | CONC-D3-2026-08-24-01 | HIGH | ECS Lock Ordering | NEW | #2675's detector fix exposed a real, still-live 3-edge lock cycle (`Transform→GlobalTransform→CharacterController→Transform`) that aborts any `BYRO_LOCK_ORDER_CHECK=1` character-mode session, and which no CI job's code path ever exercises |
| 3 | CONC-D3-2026-08-24-02 | MEDIUM | ECS Lock Ordering | NEW | Canonical lock-acquisition-order doc omits `CharacterController`/`RapierHandles`, the exact types involved in finding #2's cycle |
| 4 | CONC-D3-2026-08-24-03 | MEDIUM | ECS Lock Ordering | NEW | #2134's Resource↔Storage restructure covered 4 of 6 sibling AI-package systems; `wander_system`/`patrol_system` still hold `PhysicsWorld` inside a 5-storage-guard span |
| 5 | CONC-D3-2026-08-24-05 | MEDIUM | ECS Lock Ordering | NEW | `weather_system`'s `WeatherDataRes`→`WeatherTransitionRes` hold order is undocumented; acyclicity rests on one uncommented `drop()` in the reverse-order sibling |
| 6 | CONC-D3-2026-08-24-04 | LOW | ECS Lock Ordering | NEW | The six `Resource` accessors defuse the lock-tracker scope *before* constructing the guard — the inverse of #2149's discipline in the four `Query` accessors (latent, not live) |
| 7 | CONC-D5-2026-08-24-01 | LOW | RwLock Patterns (Physics) | Existing: #3130 | `pull_dynamic`'s lock-drop comment still describes a guard drop as happening "below" it when it actually happens ~75 lines above — confirmed still present, doc-only |
| 8 | CONC-D5-2026-08-24-02 | MEDIUM | RwLock Patterns (Physics) | NEW | `player_water_state` re-locks `TotalTime`+`WindField` inside the per-water-plane loop while holding 3 storage read guards — the only site in the physics path that inverts the project's storage-before-resource discipline; one registration change (`weather_system` off its deliberate exclusive placement) away from a live ABBA |
| 9 | CONC-D5-2026-08-24-03 | LOW | RwLock Patterns (Physics) | NEW | `dump_awake_fallers` and `spawn_collider_census_report` hold `RenderLayer`/`FormIdComponent`/`PhysicsSourceForm` storage guards across a `FormIdPool` resource acquisition — read-only, no live deadlock (no runtime `FormIdPool` writer), but inconsistent with the same functions' own documented #2136 discipline on their `PhysicsWorld` half |
| 10 | CONC-D5-2026-08-24-04 | LOW | RwLock Patterns (Physics) | NEW | `physics_sync_system` (the full 4-phase tick, `dt=0.0`) is invoked re-entrantly from 3 non-scheduler call sites (`character.rs:772`, `view.rs:160`, `scene.rs:1163`) with no documented or asserted exclusivity requirement — safe today only because all three happen to run outside the scheduler or inside an exclusive lane |

Dimensions 1, 4, 6, and 7 came back **clean** (0 new findings) — see their sections
below for the full checklist-verification trail, which is substantial evidence of a
deliberate re-derivation rather than a rubber stamp (Dimension 6 in particular
independently re-confirmed a conclusion `AUDIT_CONCURRENCY_2026-08-20.md` already
reached, from the current post-#1749/#3231 code rather than by trusting the prior
report).

Two **audit-process notes** surfaced (not codebase findings, listed separately below
under "Process Notes"): a stale SKILL.md checklist wording already tracked as #2690,
and GitHub issue #3111 whose fix has landed but which is still open on the tracker.

---

## Dimension 1: Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface)

**0 findings.** Scope: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`),
`sync.rs`, `acceleration/{blas_static,blas_skinned,tlas,memory,predicates,mod}.rs`,
`context/resize.rs`, `texture.rs`.

Every checklist item traced against line-numbered current code:

- **Queue submission single-Mutex.** The live code deliberately holds `graphics_queue`'s
  Mutex across `queue_submit`/`queue_present` (`draw.rs:3788-3826`, `:3904-3919`), per
  `VUID-vkQueueSubmit-queue-00893` — this is *correct* per spec (submit/present don't
  block for GPU completion; holding the lock across a non-blocking enqueue is cheap and
  required when `present_queue` may alias `graphics_queue`, #284). The SKILL.md
  checklist's literal wording ("guard must not be held across queue_submit/present") is
  stale and already tracked as **Existing: #2690** — see Process Notes. The one-time-command
  path (`texture.rs:814-826`) correctly scopes the lock to just the submit and releases
  it before the fence wait, with a regression test pinning the ordering.
- **Frame-in-flight discipline** — dual-slot fence wait (`in_flight[frame]` +
  `in_flight[prev]`, `draw.rs:1613-1626`, #282), `image_available[frame]` recovery on
  every fallible acquire path, `images_in_flight[img]` per-swapchain-image guard —
  all present and correct.
- **Acquire→render→present semaphore chain** — `render_finished` is correctly
  per-swapchain-image (`sync.rs:56-70`), indexed by `img` at both submit-signal and
  present-wait sites, with a regression test (`render_finished_is_sized_and_indexed_per_swapchain_image`)
  scanning the source to guard against re-indexing by `frame`.
- **AS build→read barriers** — static BLAS compaction barrier, skinned BLAS refit's
  #1790 WRITE|READ dst mask, and the single frame-wide TLAS AS_WRITE→AS_READ barrier
  (`draw.rs:2659-2667`, #2931) that also publishes the same-command-buffer skinned
  refit — all present.
- **Deferred BLAS-scratch destruction (#1782)** and **AS build-input barrier flag
  (#507945d8-class, `SHADER_READ`@`ACCELERATION_STRUCTURE_BUILD_KHR`)** — both correct,
  with the documented deliberate immediate-destroy exception in
  `build_skinned_blas_batched_on_cmd` intact.
- **Deferred AS destruction vs in-flight reads (#a476b256-class)** — `pending_destroy_blas`
  used at both eviction sites; every immediate `destroy_acceleration_structure` call
  found is on a pre-registration error path with a SAFETY comment; shutdown drains the
  queue before final AS teardown.
- **Swapchain recreate sync** — `device_wait_idle` is the first fallible step in
  `recreate_swapchain_core` (`resize.rs:37`), before any destroy/rebuild.
- **One-time command buffers** — static-BLAS build/compaction, buffer staging, and
  init-time uploads all route through the fenced `with_one_time_commands_inner`; no
  blocking one-time submit found unconditionally inside `draw_frame`'s per-frame body.

**Note for Dimension 6/7 (not filed here)**: `egui_pass.rs:245-255` holds
`queue.lock()` across both submit and an internal fence wait inside a third-party
call — safe only because Vulkan dispatch is currently single-threaded.

---

## Dimension 2: Compute → AS → Fragment Chains

**1 finding: HIGH.**

### CONC-D2-2026-08-24-01: `MorphSlot::weight_buffer` host write races the previous frame's still-in-flight `skin_vertices.comp` read
- **Severity**: HIGH
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/morph_compute.rs:29-58` (`MorphSlot` struct), `:166-185` (`update_weights`); `byroredux/src/render/skinned.rs:280-292` (`update_morph_weights`); `byroredux/src/app_frame.rs:169` (call site, before `ctx.draw_frame` at `:474`); contrast `crates/renderer/src/vulkan/context/draw.rs:2464` (`upload_bone_worlds`, correct pattern) and `draw.rs:1604-1627` (dual-fence wait, #282)
- **Status**: NEW — landed in commit `d0322785` ("Fix #3231: wire GPU morph-target deformation end-to-end (Phase D)"), one day before this audit. No matching open issue in the dedup baseline (not a duplicate of #3233, which is an unrelated NIFAL index-space bug on the same feature).
- **Description**: `MorphSlot` (new in #3231) holds exactly one `weight_buffer` per
  entity — not a `[MAX_FRAMES_IN_FLIGHT]`-indexed double buffer the way every other
  per-frame GPU input in this renderer is shaped (`bone_world_buffers()[frame]`,
  SVGF/TAA/caustic/volumetrics param UBOs). `weight_address()` is handed to
  `skin_vertices.comp` as a raw `buffer_reference` device address, read during the
  compute dispatch that feeds the skinned-BLAS refit → ray-query chain. `update_morph_weights`
  is called from `App::render_one_frame` at `app_frame.rs:169`, **before**
  `ctx.draw_frame` is invoked (`:474`) — i.e. before `draw_frame`'s own dual-fence wait
  (`draw.rs:1604-1627`, added by #282 specifically because the GPU commonly lags the
  CPU by up to a frame). So the host memcpy into `weight_buffer` happens with no fence
  protecting it, while the previous frame's `skin_vertices.comp` dispatch may still be
  reading the same physical memory. `pose_dirty`'s hash folds in morph weights (per
  #3231's commit message), so the highest-risk frames — weights genuinely changing,
  e.g. a talking/blinking NPC — are exactly the frames most likely to race, not a rare
  edge case. #3231's own verification note exercised "120 actively-dispatched skinned
  slots," a dense, GPU-heavy scenario where 1-frame CPU/GPU overlap is the common case
  per #282's own rationale.
- **Evidence**:
  ```rust
  // morph_compute.rs — ONE buffer, not Vec<GpuBuffer; MAX_FRAMES_IN_FLIGHT>
  pub struct MorphSlot {
      delta_buffer: GpuBuffer,
      delta_address: vk::DeviceAddress,
      weight_buffer: GpuBuffer,      // <-- single, no per-FIF index
      weight_address: vk::DeviceAddress,
      ...
  }
  // morph_compute.rs:171-185 — plain host memcpy, no barrier/fence
  pub fn update_weights(&mut self, device: &ash::Device, weights: &[f32]) -> Result<()> {
      self.weight_buffer.mapped_slice_mut()?[..bytes.len()].copy_from_slice(bytes);
      self.weight_buffer.flush_if_needed(device)
  }
  ```
  ```rust
  // app_frame.rs:148-169 — runs BEFORE draw_frame, i.e. before any fence wait this frame
  let frame = build_render_data(...);
  crate::render::update_morph_weights(&self.world, ctx);   // <- host write happens here
  ...
  // app_frame.rs:474 (later in the same function)
  let draw_result = ctx.draw_frame(FrameInputs { ... });   // <- fence wait happens INSIDE this call
  ```
- **Impact**: WAR data race on host-visible/device-visible memory: no Vulkan barrier or
  fence orders the host write against the prior frame's device read. Worst-case symptom
  is visibly torn/incorrect morph deformation (stale + new weights partially blended)
  on skinned NPCs under GPU load; this also feeds the skinned-BLAS refit and thus RT
  secondary rays through `skinnedVertexAddress`, so corruption is not necessarily
  confined to raster.
- **Related**: Sibling correct pattern: #282, #1195/#1196 (`SkinSlot`'s pose-dirty-gated
  dispatch skip, which this new code mirrors for the dispatch but not the write-timing
  discipline). Not a duplicate of #3233.
- **Suggested Fix**: Either (a) make `weight_buffer`/`weight_address` a
  `[MAX_FRAMES_IN_FLIGHT]`-indexed resource with push-constant frame selection, mirroring
  every other per-frame GPU input, or (b) move `update_morph_weights` to run inside
  `draw_frame` *after* the dual-fence wait, mirroring where `upload_bone_worlds` runs.
- **Verification Path**: Not a barrier/stage-mask defect, so `BYRO_VALIDATION` sync
  validation will not flag it (it tracks GPU-side command ordering, not host-mapped-memory
  writes racing an in-flight device read). Provable from code structure alone (no
  per-FIF buffering + write ordered before the frame's fence wait) — reported as a firm
  finding, not a HYPOTHESIS. Empirical confirmation path: force an artificial stall in
  `skin_vertices.comp` for one frame while an NPC's morph weights are actively animating,
  then diff-capture deformed geometry across frames N-1/N in RenderDoc for
  torn/inconsistent weights; or profile a GPU-bound scene with several actively-animating
  skinned NPCs (#3231's own "120 slots" scenario) to confirm real cross-frame overlap,
  then visually inspect for morph flicker/tearing.

### Checklist items traced clean (no findings)

- **Skin chain (M29)** — palette build → `COMPUTE_SHADER_WRITE→(COMPUTE_SHADER_READ|VERTEX_SHADER_READ)`
  barrier (`draw.rs:2550-2567`) → BLAS refit's own barrier (widened by #2403 to cover
  the fragment-stage `skinnedVertexAddress` dereference) → refit's AS barrier
  (`skinned_blas_refit.rs:672-679`) → the single frame-wide AS_WRITE→AS_READ barrier
  after `build_tlas` (`draw.rs:2659-2667`, #2931). Chain intact end to end; raster path
  still inline-skins in `triangle.vert`, no `VERTEX_INPUT` barrier required.
- **Cross-frame ping-pong** — SVGF/TAA both `(f+1)%MAX_FRAMES_IN_FLIGHT`, both
  compile-time-asserted `MAX_FRAMES_IN_FLIGHT >= 2` (#918-class). Volumetrics
  `lighting_volumes`/`integrated_volumes` correctly `[frame]`-indexed. Caustic's
  accumulator is an intra-slot decay/clear EMA, not a cross-frame read (per-frame-fence
  guarantees slot idleness). Water-caustic is cleared-and-rewritten within the same
  frame — neither is a history-buffer aliasing risk.
- **Volumetrics gate (#1105)** — `tlas_written[frame]` latch set/reset symmetric with
  sibling latches; the sole call site sets and dispatches atomically inside one
  `if let` branch, no path calls one without the other.
- **Bloom RAW chain (#931/#2796)** — per-mip `SHADER_WRITE→SHADER_READ` post-barrier on
  every `down_mips[i]`/`up_mips[i]` write; `up_mips[0]`'s barrier correctly targets
  `COMPUTE_SHADER` dst stage post-#2796 (compute consumer, not composite fragment).
- **Caustic CLEAR→COMPUTE→FRAGMENT** — both the parked-camera decay path and the
  moving-camera clear path correctly sequence every read-modify-write.
- **MaterialBuffer SSBO (R1)** — `upload_materials` called before `record_geometry_pass`,
  covered by the bulk `HOST_WRITE→(VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT)` barrier; not
  on a compute path.

---

## Dimension 3: ECS Lock Ordering & Deadlock

**4 NEW findings — 1 HIGH, 2 MEDIUM, 1 LOW** (a 3rd MEDIUM listed separately above as
item 5 in the executive table; 5 findings total in the source dimension report, 1
MEDIUM folded into the exec table already). **Plus 7 regression guards verified
INTACT.**

### CONC-D3-2026-08-24-01: #2675 fixed the detector but left the live 3-cycle it documented — the reachability probe now aborts any character-mode debug run, and neither CI job that sets the flag can reach it
- **Severity**: HIGH
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/character.rs:533-541` and `:193-212`; `crates/core/src/ecs/systems.rs:78-84`; detector at `crates/core/src/ecs/lock_tracker.rs:383-390`
- **Status**: NEW (the *cycle* half of #2675; #2675 itself is the detector-coverage half and its fix has landed)
- **Trigger Conditions**: A debug build with `BYRO_LOCK_ORDER_CHECK=1` running a character-mode cell (`PlayerMode::Character` with a live `PlayerEntity`). Deterministic — no timing window needed. An actual hang additionally requires two of the three producers to be co-scheduled in one stage.
- **Description**: #2675 enumerated a complete three-edge lock cycle already present in
  the live schedule and fixed only the detector (depth-1 containment check → `find_path`
  reachability). All three edges are still present, byte-for-byte:

  | Edge | Producer | Stage / mode | Held-across evidence |
  |---|---|---|---|
  | `Transform → GlobalTransform` | `make_transform_propagation_system` (`systems.rs:78-84`) | PostUpdate parallel | `tq` (Transform) bound before `gq` (GlobalTransform); `tq.storage_mut().drain_dirty_into(...)` at `:93` proves `tq` outlives `gq` |
  | `GlobalTransform → CharacterController` | `camera_follow_system` (`character.rs:533,539`) | Late parallel | `gq` bound at `:533`, `cq` at `:539`, then `gq.get(cam_entity)` at `:548` — `gq` provably still live |
  | `CharacterController → Transform` | `character_controller_system` via `player_controller_system` (`character.rs:193,205`) | Early parallel | `cq` bound at `:193`, nested `Transform` query at `:204-212` inside `cq`'s block |

  Composing edges 2+3 gives `GlobalTransform ⇝ Transform`, the exact reverse of the
  canonical chain's own head (`docs/engine/ecs.md:597`). The detector is now *correct*
  (strengthened per #2675) and the graph is now *cyclic* — so it fires on real content
  instead of staying silent. **Why CI still passes**: `.github/workflows/ci.yml` sets
  `BYRO_LOCK_ORDER_CHECK=1` in exactly two jobs — `lock-order-check` (`cargo test
  --workspace`) never drives `camera_follow_system`/`character_controller_system` (no
  test call site exists); `vulkan-validation` (`--bench-frames 5`) passes no `--cell`,
  so `PlayerMode` never becomes `Character` and both systems early-return before
  touching storage. The strengthened detector has never been run against the cycle it
  was strengthened to catch.
- **Impact**: (a) With the flag set on any character-mode debug session, the process
  aborts on frame ~1 — the detector is unusable for the mode most gameplay work happens
  in. (b) Without it, a genuine ordering violation sits on the renderer-feeding pose
  path waiting on a stage merge or re-stage (both demonstrated live practice in this
  cluster, e.g. #3180's `submersion_system` re-stage) to become a silent hang with no
  panic and no log.
- **Related**: #2675 (detector fix, landed), #2388, #2135, #2547, #2387. `docs/engine/ecs.md:594-640`.
- **Suggested Fix**: Break edge 2: in `camera_follow_system`, copy the two `gq.get(...)`
  results into locals and drop `gq` before acquiring `CharacterController`. Add a test
  driving `make_transform_propagation_system` → `character_controller_system` →
  `camera_follow_system` sequentially on one `World` with `PlayerMode::Character` under
  `global_order::set_enabled_for_tests(true)`, asserting no panic — the test #2675
  needed and did not add, and what makes `lock-order-check` actually cover this cluster.

### CONC-D3-2026-08-24-02: the canonical acquisition order omits the two physics-facing types that transitively invert its own head
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering
- **Location**: `docs/engine/ecs.md:594-612`
- **Status**: NEW
- **Trigger Conditions**: Any new system touching `CharacterController` or `RapierHandles` alongside the hierarchy cluster — no compile-time or test-time signal exists.
- **Description**: The canonical chain (`Transform → Parent → Children → GlobalTransform → SkinnedMesh → MeshHandle → LocalBound → WorldBound → Name → StringPool`) omits `byroredux_physics::CharacterController`/`RapierHandles` — exactly the types that, in `character.rs`, are acquired both before `Transform` and after `GlobalTransform` (the mechanism of finding #1). `character.rs:199-203` documents an ordering constraint (#2135) purely as a local comment; the knowledge exists but never reached the canonical doc.
- **Impact**: The one artifact meant to make hand-ordered N-lock acquisition auditable is silent about the cluster with the most live inversions.
- **Related**: CONC-D3-2026-08-24-01, #2388, #2135, #2404.
- **Suggested Fix**: Extend the chain with the physics pair (most naturally after `GlobalTransform`), and hoist `character.rs:199-203`'s local note into the doc as a worked example.

### CONC-D3-2026-08-24-03: #2134's "snapshot before PhysicsWorld" restructure skipped `wander_system_inner` and `patrol_system_inner`
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/patrol.rs:80-93`; `byroredux/src/systems/wander.rs:243-259`
- **Status**: NEW (same finding class as #2134, which covered 4 of 6 siblings)
- **Trigger Conditions**: Any cell with a `WanderBehavior`/`PatrolBehavior` actor and a live `PhysicsWorld`. Both systems are `Stage::PostUpdate` exclusives today, so no live hang — promoting either to parallel (`add_to_with_access`, a one-line change) removes that protection.
- **Description**: #2134 restructured `follow`/`escort`/`travel`/`guard` into a snapshot-then-physics two-pass shape (comment at `follow.rs:242-246` names #2134 explicitly). `patrol.rs`/`wander.rs` were never restructured — both still acquire `PhysicsWorld` *inside* a block holding five storage read guards (`Transform`, `PatrolState`/`WanderState`, `NavmeshTile`, `NavPath`, behavior), for the whole decision loop. The #2134 sweep was scoped by behavior type and never named these two, since they share `step_oscillating_wander`/`pick_wander_target` rather than `step_along_waypoints`.
- **Impact**: The edge direction found (storage → `PhysicsWorld`) matches every other site in `crates/physics/src/sync.rs` — not a live cycle today, since no reverse `PhysicsWorld → storage` edge exists. It is an incomplete fix leaving two systems outside the convention the other four now encode, with a five-guard hold window that any future reversed site would immediately close a cycle against.
- **Related**: #2134, #2404 (open), #3130.
- **Suggested Fix**: Apply the sibling shape — hoist the per-entity snapshot under the storage guards, close the block, run the physics-touching step in a second block holding only `PhysicsWorld`, mechanically identical to `follow.rs:246-267`.

### CONC-D3-2026-08-24-05: `weather_system`'s `WeatherDataRes`→`WeatherTransitionRes` hold is undocumented, and its acyclicity rests on an uncommented `drop` in the reverse-order sibling
- **Severity**: MEDIUM
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/weather.rs:466-685` (hold span), `:544-546`, `:664-670`, `:675-680` (nested acquisitions), `:819-830` (reverse-order sibling)
- **Status**: NEW (same finding class as #2269/#2153/#2154 — undocumented resource-pair ordering — different pair)
- **Trigger Conditions**: Any exterior cell with `WeatherDataRes` loaded. `weather_system` is exclusive in `Stage::Early` today (#3111), so no live race — the hazard is that the safety is circumstantial and unwritten.
- **Description**: `weather_system` holds a `WeatherDataRes` read guard for 220 lines and acquires `WeatherTransitionRes` reads three times inside that span, establishing `WeatherDataRes → WeatherTransitionRes`. Its sibling `promote_weather_transition_target` walks the pair in reverse: `WeatherTransitionRes` read, seven field copies, `drop(tr)` at `:829` (**no comment**), then `WeatherDataRes` write. That uncommented `drop` is the single line preventing a two-node cycle. `collapse_weather_transition` is a second entry point into the same pair.
- **Impact**: Deleting/moving `weather.rs:829` — e.g. an "avoid the copies" cleanup that borrows `tr.target` directly — creates `WeatherTransitionRes → WeatherDataRes` against the existing reverse edge, a length-2 cycle. Under `BYRO_LOCK_ORDER_CHECK=1` that aborts; without it, a real hang the moment `weather_system` leaves the exclusive lane.
- **Related**: #2269, #2153, #2154, #3111, #1103.
- **Suggested Fix**: Add a two-line lock-order note at `weather.rs:466` and at the `drop(tr)` naming the invariant. More robust: a `try_resource_2_mut`-style paired accessor so ordering is TypeId-sort-enforced rather than conventional.

### CONC-D3-2026-08-24-04: the six resource accessors defuse the tracker scope *before* constructing the guard — the inverse of #2149's discipline in the query accessors
- **Severity**: LOW
- **Dimension**: ECS Lock Ordering
- **Location**: `crates/core/src/ecs/world.rs:687-688`, `:716-717`, `:772-777`, `:786-792`, `:814-815`, `:833-835`
- **Status**: NEW
- **Trigger Conditions**: Only reachable if a fallible operation moves into `ResourceRead::new`/`ResourceWrite::new` in the future. Then a `catch_unwind`ing caller is left with an orphaned `LOCKS` row and a spurious deadlock panic on the next acquisition.
- **Description**: All four `Query` accessors construct the wrapper first, defuse after (the #2149 fix, with a comment at each site). All six `Resource` accessors do the opposite — benign today only because `ResourceRead::new`/`ResourceWrite::new` are pure struct literals. Undocumented and structurally opposite to its siblings, so a future hot-path optimization (the #1367 cached-downcast treatment, already applied to the Query side) would silently reintroduce #2149 for resources.
- **Impact**: No live defect — latent regression risk.
- **Related**: #2149, #137, #1367.
- **Suggested Fix**: Reorder all six to construct-then-defuse (zero cost, both infallible today), or add a one-line note at each site.

### Regression guards verified INTACT (no findings)

All seven re-read at HEAD (the lock_tracker rewrite in `5428e872` names no issues in
its commit message, so per the known multi-issue-close gotcha these are still **OPEN
upstream despite the fix being present in code** — worth closing rather than
re-auditing):

1. **TypeId-sorted acquisition (#313)** — all four paired accessors (`query_2_mut`,
   `query_2_mut_mut`, `resource_2_mut`, `try_resource_2_mut`) branch on `TypeId`
   ascending order for both the real locks and the `lock_tracker` scopes; same-type
   access hits `assert_ne!` up front.
2. **Check-before-insert (#2384)** — `global_order::record_and_check` runs and gets the
   chance to panic before the incoming acquisition's row is inserted into `LOCKS`;
   `is_clean()` verified post-caught-ABBA-panic in three test scenarios.
3. **GRAPH poison recovery (#2385)** — all four `GRAPH` acquisitions use
   `unwrap_or_else(|poison| poison.into_inner())`, no `.expect`/`.unwrap`; the cycle
   panic explicitly drops the write guard before unwinding.
4. **Recursive read stays a warning (#2386)** — warns once on the 1→2 transition, no
   panic. **Partially open** (not re-filed, still tracked at #2386): the recursive path
   returns before `record_and_check`, dropping every `held_others → T` edge, not just
   the `T → T` self-edge.
5. **Cross-thread guarantee has a real-`World` test (#2387)** — a two-thread `Barrier`
   race on opposite query orders asserts exactly one is rejected and both tracker maps
   end clean.
6. **Poison fail-fast (#466)** — every acquisition resolves `PoisonError` through the
   named side-table panic helpers; no path silently unwraps a poisoned guard.
7. **No structural mutation during system execution** — every system takes `&World`;
   the two `&mut World` entry points in this tree are called from the winit handler,
   not the scheduler.

**Still-open pre-existing rows re-confirmed present, not re-filed**: #2400
(`animation_system_inner` holds `AnimationClipRegistry` + `NameIndex` across ~30
downstream acquisitions, ordering documented only in local comments), #2547 (partial —
`world.rs` sites correctly say "debug builds with `BYRO_LOCK_ORDER_CHECK=1`" but
`lock_tracker.rs`'s module header still leads with looser wording), #2135, #2388,
#2404, #3130.

---

## Dimension 4: Scheduler Access Declarations (regression guard — M27 closed)

**0 findings.** M27/R7 remain closed; `known_conflict_count()`/`unknown_pair_count()`/
`undeclared_parallel_count()` are 0 on the real schedule, verified both by static read
and by running `cargo test -p byroredux --bin byroredux scheduler_access_tests` —
**14/14 pass**.

- **Conflict model** (`crates/core/src/ecs/access.rs`) — exactly `None`/`Unknown`/`Conflict`, no `Parallel` variant; pessimistic "undeclared ⇒ Unknown" fallback intact.
- **Migration KPIs** — enforced twice: `debug_assert_eq!` at boot (`boot.rs:1512-1540`) and a hard `assert_eq!` in `scheduler_access_tests.rs` that runs under plain `cargo test` regardless of build profile — a stronger guarantee than the checklist implies.
- **Exclusive phase** — `audio_system` (Late), `spin_system` (Update) unchanged; `player_controller_system` remains the M27 Phase-3 *merge* (parallel, union access), not a re-stage.
- **Re-entry/panic policy** — `Scheduler` confirmed never a `Resource` anywhere in the codebase (#868 holds structurally); fail-fast panic policy (#1412) unchanged, not flagged.
- **WindField producer/consumer (#3111)** — traced all 5 sites in `boot.rs`. `weather_system` (sole writer, `Stage::Early` exclusive) completes before `PostUpdate`/`Physics`/`Late` readers begin; `player_controller_system` (`Early` parallel) intentionally reads the previous frame's value by design (documented at `boot.rs:744-747`). No cross-stage sequencing bug — this is the fix option #3111 recommended, now in place.
- **Other producer/consumer pairs checked by hand** (the class the automated KPIs cannot catch): `PhysicsWorld`, `StringPool`, `CellLightingRes` — all correctly sequenced across stages/phases. `GameTimeRes`/`WaterContactScratch`/`AnimationClipRegistry`/`SubtreeCache`/`NameIndex` have no cross-system read/write pair to check.

**Out of scope**: live `sys.accesses` invocation via a running engine + byro-dbg attach
was not performed (no live engine instance available, per the project's "no parallel
engine launch" rule). The static equivalent — reading the exact `access_report()` code
path the command calls, exercised directly by the dedicated test suite — was used
instead and is a strictly stronger check.

**Administrative note** (see Process Notes): recommend closing #3111.

---

## Dimension 5: RwLock Patterns — Resource↔Storage & Physics Step

**4 findings: 1 MEDIUM (NEW), 2 LOW (NEW), 1 LOW (Existing: #3130, regression-confirmed).**
A deeper re-pass past the checklist-verification bullets below (see "Additional
findings" after them) surfaced three real gaps the first pass's summary line
("every checklist item otherwise traces to correct discipline") missed — all three
are storage/resource-lock-order or declaration gaps, not live deadlocks today, but
each is one nearby registration or refactor change away from becoming one, per this
dimension's own severity rubric.

### CONC-D5-2026-08-24-01: `pull_dynamic`'s lock-drop comment is stale (still present)
- **Existing: #3130** (OPEN, documentation/low/tech-debt/physics/concurrency/doc-rot)
- **Severity**: LOW
- **Location**: `crates/physics/src/sync.rs:1135-1139` (comment), actual drop at `:1060-1061`
- **Description**: The comment above `pull_dynamic`'s `Transform` write-guard
  acquisition claims the `RapierHandles`/`RigidBodyData` read guards drop "below" it —
  they actually drop ~75 lines earlier. Unchanged from the original #3130 report
  (originally citing `sync.rs:1075-1080`; the file has shifted ~60 lines from added
  diagnostics since, but the same misplaced comment persists verbatim). The underlying
  invariant it (mis)describes — no `Transform` write while a `RapierHandles`/
  `RigidBodyData` read is live, avoiding an ABBA edge against `character_controller_system`
  — is correctly implemented; this is pure doc-rot, not a live deadlock risk.
- **Suggested Fix**: Unchanged from #3130 — move the comment to sit directly above the
  actual `drop()` calls and reword to state the invariant rather than describe adjacent
  code.

### Checklist verification (no other findings)

- **Phase 1 Resource↔Storage separation** — `collect_newcomers` (storage reads only,
  returns owned `Vec`) is a separate call from `register_newcomers` (`PhysicsWorld`
  write, then `RapierHandles` write only after `drop(pw)`); the two guards are never
  live simultaneously.
- **Helper lock order** (`set_linear_velocity`/`set_kinematic_translation`) — the
  `RapierHandles` read guard is scoped to the `match` head expression and dropped
  (Copy-out) before `resource_mut::<PhysicsWorld>()`. All three callers
  (`character_controller_system`, `snap_character_body_to_camera`,
  `ground_character_body_at`) checked — none holds a `PhysicsWorld` guard across the call.
- **`ContactConfig`** — snapshotted once before the newcomer loop, never re-locked inside it.
- **Cell-unload teardown (#1520)** — `release_victim_rapier_bodies` collects into a
  scratch `Vec` under a scoped storage-read block, drops it, then takes the
  `PhysicsWorld` write; confirmed this runs before the despawn loop that drops the ECS
  `RapierHandles` rows.
- **Single-threaded placement** — `physics_sync_system` is the sole occupant of
  `Stage::Physics`'s parallel batch (structurally nothing to co-schedule against); cross-
  stage systems touching physics state execute in strictly sequential stages by
  scheduler design (see Dimension 4).
- **Supplementary spot-check**: Phase 2.5 buoyancy (`crates/physics/src/water.rs`) shows
  the same release-reads-before-write discipline in both its passes.

### Additional findings (deeper re-pass)

### CONC-D5-2026-08-24-02: `player_water_state` re-locks `TotalTime`+`WindField` inside the per-plane loop while holding three storage read guards
- **Severity**: MEDIUM
- **Location**: `byroredux/src/systems/character.rs:893-940` (guards at 898-901, resource acquisitions at 916-920)
- **Description**: This is the only site in the physics path that holds *storage* read guards (`WaterPlane`, `WaterVolume`, `WaterFlow`, acquired at `:898-901`) across a *resource* acquisition — the exact reverse of the "snapshot storages, drop guards, then take the resource" discipline `push_kinematic`, `pull_dynamic`, `dump_awake_fallers`, and `apply_buoyancy_with_scratch` all follow. Inside the loop body, `:916-917` takes `world.try_resource::<TotalTime>()` and `:920` calls `weather_wave_adjustment`, which itself takes `world.try_resource::<WindField>()` (`crates/physics/src/water.rs:367-370`) — a second resource lock nested under the first, both under the three storage guards. It also re-acquires both resources and recomputes an identical `weather_wave_adjustment` once per water plane per frame, where `apply_buoyancy_with_scratch` (`water.rs:590-600`) hoists both out of its loop.
- **Impact**: Not a live deadlock. `WindField`'s only writer, `weather_system`, is a deliberate `Stage::Early` *exclusive* (`boot.rs:744-748`, pinned by `player_wind_read_is_declared_and_weather_writer_is_exclusive`) — the same design choice CONC-D3-2026-08-24-05 above flags as undocumented in the canonical lock-order doc. That test's own stated purpose is a *read/write race*, not a lock-order edge, so a future registration change moving `weather_system` into the Early parallel batch is all that separates this from a genuine cross-thread ABBA against `player_controller_system` (which holds water storages and would want `WindField`, while `weather_system` holds `WindField` and would want storages).
- **Related**: #3111 (established `weather_system`-exclusive as the mitigation for this exact pair — see this dimension's own note above that #3111 is stale-open despite the fix landing); `apply_buoyancy_with_scratch` is the reference implementation for the correct hoist-before-loop shape.
- **Suggested Fix**: Hoist `time_secs` + `weather_wave_adjustment` above the `query::<WaterPlane>()` acquisitions in `player_water_state`, matching `apply_buoyancy_with_scratch:590-600`, and pass the resulting values into the loop. Removes both the per-iteration re-lock and the storage→resource inversion in one edit.

### CONC-D5-2026-08-24-03: two physics diagnostics hold storage read guards across a `FormIdPool` resource acquisition, inconsistent with their own documented discipline
- **Severity**: LOW
- **Location**: `crates/physics/src/sync.rs:311-349` (`dump_awake_fallers`) and `crates/physics/src/sync.rs:594-624` (`spawn_collider_census_report`)
- **Description**: Both functions carefully snapshot `PhysicsWorld` and drop that guard before opening ECS storages — each carries an explicit comment citing #2136 for exactly that discipline. Both then invert it for the resource↔storage pair: `RenderLayer`+`FormIdComponent`+`PhysicsSourceForm` read guards stay open across `world.try_resource::<FormIdPool>()` and the whole resolution loop that follows.
- **Impact**: Read-only on both sides and `FormIdPool` has no runtime writer, so there is no deadlock today. The concern is precedent — these are the two functions in the crate that document lock discipline explicitly, and they model the correct order for the `PhysicsWorld` pair while inverting it for the `FormIdPool` pair. `spawn_collider_census_report` is also runtime-reachable via the `phys.census` debug console command, a surface `_audit-common.md` lists as having no owner audit.
- **Related**: #2136 (the `PhysicsWorld`-before-storage discipline these functions implement for one pair but not the other); CONC-D5-2026-08-24-02 above (same storage→resource inversion shape, hot path).
- **Suggested Fix**: Resolve the `FormIdPool` lookup into an owned snapshot (or resolve form ids in a second pass after the storage guards drop) so the resource lock is never taken with a storage guard live; extend the existing #2136 comments to state the full rule (`PhysicsWorld` → storages → no resource acquisitions while storages are open).

### CONC-D5-2026-08-24-04: `physics_sync_system` is invoked re-entrantly from three non-scheduler sites with no documented or asserted exclusivity requirement
- **Severity**: LOW
- **Location**: `byroredux/src/systems/character.rs:772`, `byroredux/src/commands/view.rs:160`, `byroredux/src/scene.rs:1163`
- **Description**: The full 4-phase physics tick — `resource_mut::<PhysicsWorld>()`, `pw.step(dt)`, the buoyancy pass's writes, and `pull_dynamic`'s `Transform` *write* guard — is invoked with `dt = 0.0` from three sites outside `Scheduler::run`, purely to register newcomers so the query pipeline can be flushed for a floor probe. Nothing in the function's doc comment, its call sites, or a test states these entries require an exclusive/`&mut World` context; `physics_sync_system` takes `&World`, so the type system doesn't enforce it. The three contexts happen to be safe today: `transition.rs`/`scene.rs` run on `&mut World` outside the scheduler, and the `combat.approach` console command executes inside `DebugDrainSystem`, a `Stage::Late` exclusive that the conflict analyzer doesn't see.
- **Impact**: No defect today, but the exercised write surface (`Transform`, `RapierHandles`, `WaterContact`, `PhysicsWorld`, `WaterContactScratch`) is invisible to the access analyzer from these call paths, and one sits on the debug console command surface — a subsystem `_audit-common.md` lists as un-owned. A future console command or scheduler registration that reaches one of these helpers inherits an undeclared full physics tick.
- **Related**: CONC-D5-2026-08-24-01 (same system's own declaration being the analyzer's only view of it — see also the cross-referenced ECS-2026-08-24-05 finding on that declaration's `Parent`/`ActorBoneCollider` gap); the standing note that the debug server's command surface has no owner audit.
- **Suggested Fix**: Document the requirement on `physics_sync_system` (exclusive/`&mut World` context only, naming the three call sites), or extract the narrower "register newcomers, then flush the query pipeline" operation the three sites actually want into its own helper so the full tick isn't the public entry point for a floor probe.

**Cross-reference (not a new finding here, filed under `/audit-ecs`)**: `physics_sync_system`'s declared `Access` also omits `Parent` (read in `pull_dynamic`) and `ActorBoneCollider` (read in `collect_newcomers`) — the same declaration-completeness gap class as CONC-D3-2026-08-24-02/03/05 above, but in this dimension's own scope. Independently found and filed as **ECS-2026-08-24-05** (`AUDIT_ECS_2026-08-24.md`) and separately cross-checked by `/audit-physics`; not re-filed here to avoid a duplicate issue.

---

## Dimension 6: Resource Lifecycle (GPU teardown ordering)

**0 findings.** Scope: `context/mod.rs`'s Drop impl (now `context/teardown.rs` under
#1749, a verbatim move per its own doc comment), all `destroy()` methods, `buffer.rs`,
`acceleration/`, `context/resize.rs`, `egui_pass.rs`, `scene_buffer/`, `material.rs`.

`git log --since 2026-08-20` on this dimension's scope shows only three substantive
commits since the prior sweep (`AUDIT_CONCURRENCY_2026-08-20.md`, which already
reported this dimension clean): #1749's verbatim extraction, #3231's `morph_slots`
teardown arm, and a pure `cargo fmt` reformat. This audit **independently re-derived**
the clean conclusion from the current code rather than trusting the prior report:

- **Reverse-order destruction / allocator freed last** — `device_wait_idle` first, then
  an allocator-*independent* block (#1483, hoisted out of the `Some(allocator)` guard),
  then the allocator-owned block (guarded by `if let Some(alloc) = self.allocator.clone()`),
  then depth resources and pipelines (guarded by `if let Some(ref allocator) = ...`,
  borrow not move), then `allocator.take()` + `Arc::try_unwrap` (with the #665 leak
  guard logging+leaking rather than freeing against a destroyed device on a stray
  clone), then device/surface/debug-messenger/instance. No resource needing the
  allocator is destroyed after `.take()`.
- **Allocator-clone-holding subsystems** (`water.rs`, `exposure.rs`, `texture.rs`,
  `buffer.rs`'s `StagingGuard`) — all release their clone strictly before
  `allocator.take()`; `water.rs`'s destroy runs ~120 lines before the `Arc::try_unwrap`
  check.
- **Swapchain recreate coverage** — G-buffer, SVGF, TAA, caustic, water-caustic,
  volumetrics (`lighting_volumes`+`integrated_volumes`+4 combustion volumes+2 noise
  volumes), bloom (`down_mips`+`up_mips`), composite, egui framebuffers — every
  subsystem's `recreate_on_resize`/`recreate_framebuffers` body destroys its *old*
  per-FIF/per-image resources before allocating new ones (verified in each body, not
  just the call site).
- **AS cleanup on shutdown** — `AccelerationManager::destroy` drains `pending_destroy_blas`,
  destroys `blas_entries`/`tlas`/`skinned_blas`/scratch; per-skinned-entity skin output
  buffers destroyed separately, ahead of the skin-compute pipeline teardown.
- **Other GPU SSBO/descriptor cleanup** — `scene_buffer/descriptors.rs` destroys every
  buffer Vec including `material_buffers` (the actual home of "MaterialBuffer SSBO"
  cleanup — `material.rs` itself holds only CPU-side translation logic, out of scope
  here); `texture_registry.rs` drains pending-destroy then destroys all live entries;
  `EguiPass::destroy()` flushes the final pending texture-delta batch before destroying
  framebuffers.
- **Per-frame leaks** — the only `allocate_command_buffers` call site is the one-time
  init-only persistent allocation, freed once in Drop; no descriptor-pool/descriptor-set
  allocation in the per-frame draw path; staging buffers route through the RAII
  `StagingGuard`.
- **`terrain_tiles`/`fsr_temporal`** (no explicit `destroy()`) — checked and confirmed
  pure host-side POD with no Vulkan handle or allocator clone; natural Drop is correct.

---

## Dimension 7: Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds

**0 findings.** All 5 checklist items verified correct against live code; prior fixes
(#1167, #856, #1010, #1009, #1007, #1011, #1006, #855, #1174, #1603, #503, #3089,
#2831) remain in place with no regression.

- **Streaming Drop ordering (#1167)** — `WorldStreamingState::shutdown` explicitly
  `take()`s `worker` then `request_tx` before the bounded (10ms-cadence, timeout-detach)
  join; `Drop` delegates to `shutdown(1s)` as a safety net. Correct independent of field
  declaration order.
- **Worker ↔ main data flow** — worker functions take no `World`/`&World` parameter at
  all (structurally impossible to touch ECS), with a compile-time `assert_send::<PartialNifImport>()`
  guard (#1171); `Arc<TextureProvider>` is immutable post-construction with archive
  reads Mutex-serialised inside `BsaArchive`/`Ba2Archive`; `merge_external_material` has
  zero call sites in `streaming.rs` (all four real call sites are main-thread drain
  code); the NIF import cache is touched by the worker only through an immutable
  `Arc<HashSet<String>>` snapshot, with the actual registry write deferred to the
  main-thread drain phase.
- **Debug server** — per-client threads never reference `World`; the command queue is
  bounded (`MAX_QUEUED_COMMANDS = 64`) with atomic check-and-push (#1010, no TOCTOU
  window); the drain system (Late-stage exclusive) runs strictly before
  `render_one_frame` on the same thread each iteration, so no cross-thread race with
  the fence-gated screenshot readback is structurally possible.
- **Allocator sharing** — every one of ~50 `allocator.lock()` call sites across the
  renderer crate is a single-statement lock-then-allocate/free pattern; none hold the
  guard across a queue submit.
- **`Send + Sync` bounds** — `Component`/`Resource` require `'static + Send + Sync` at
  the trait level with zero `unsafe impl Send/Sync` anywhere in the repo (compiler-
  enforced); Ruffle/UI's `UiManager` is deliberately excluded from `Resource` and lives
  as a plain `App` field, confirming single-thread confinement; `cxx-bridge` remains a
  26-line placeholder with no FFI surface to audit.

**Non-concurrency observation** (flagged for `/audit-nifal`, not filed here): all four
`merge_external_material` call sites discard its `#[must_use]` `MergeOutcome` via
`let _ =`, contrary to the function's own doc comment warning that this erases the only
signal distinguishing a `PresenceOnly` outcome from a fully-populated merge (#2709).

---

## Process Notes (not codebase findings)

1. **SKILL.md Dimension-1 checklist wording is stale** — already tracked as **#2690**
   (open, `documentation`/`low`). The checklist's literal instruction to verify the
   queue Mutex is "not held across `queue_submit`/`queue_present`" is backwards: the
   code correctly holds it across those calls per `VUID-vkQueueSubmit-queue-00893`, and
   a future audit that "fixed" this per the checklist's literal wording would
   reintroduce a CRITICAL-class queue data race. #2690's own recommended fix (narrow
   the wording to "must not be held across the post-submit fence wait") has not yet
   landed in `.claude/commands/audit-concurrency/SKILL.md` as re-read for this run.
2. **GitHub issue #3111 should be closed** — its fix (WindField access declaration
   correction) landed in commit `5428e872` the same day as this audit and is pinned by
   a passing regression test (`player_wind_read_is_declared_and_weather_writer_is_exclusive`),
   but the issue is still marked OPEN on the tracker.

## Dedup Methodology

`gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels`
fetched to `/tmp/audit/concurrency/issues.json` at the start of this run (and refreshed
once mid-run by a sub-agent). Every finding above was checked against this baseline and
against `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md` (the most recent prior full run)
before being filed as NEW, Existing, or a regression confirmation.
