# Concurrency & Synchronization Audit — 2026-07-16

**Scope**: All 7 dimensions (comprehensive sweep, audit #4 of a 21-audit preset run).
- **Dimension 1** — Vulkan Queue & Acceleration-Structure Sync (CRITICAL surface)
- **Dimension 2** — Compute → AS → Fragment Chains
- **Dimension 3** — ECS Lock Ordering & Deadlock
- **Dimension 4** — Scheduler Access Declarations (regression guard)
- **Dimension 5** — RwLock Patterns (Resource↔Storage, Physics Step)
- **Dimension 6** — Resource Lifecycle (GPU teardown ordering)
- **Dimension 7** — Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds

**Depth**: deep (traced concurrent paths + timing windows).

**Method**: The prior sweep (`AUDIT_CONCURRENCY_2026-07-05.md`, Dimensions 1–2
only) left the renderer-concurrency surface clean and unchanged since — no
`crates/renderer/` logic has moved. What *has* landed since then is a full new
feature arc: the M42 NPC AI-package procedure runtimes
(`byroredux/src/systems/{sandbox,wander,travel,follow,escort,guard,patrol,locomotion}.rs`,
~2,000 new lines across 7 systems + a shared locomotion primitive), none of
which had been swept for concurrency before. This sweep therefore ran all
seven dimensions fresh, with Dimensions 3, 4, and 5 (ECS lock ordering,
scheduler registration, physics RwLock patterns) specifically targeting the
new M42 code as primary new-code surface, and Dimensions 1/2/6/7 re-verifying
the renderer/worker-thread machinery was not trusted to be unchanged without
re-reading it.

Per the standing speculative-fix guardrail, no Vulkan barrier/stage/layout
change is proposed on reasoning alone in this report; every Dimension 1/2
claim below is a traced concrete code state, not a hypothesis.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |

**Total findings: 0.** All seven dimensions are clean. The new M42 AI-package
system family (~2,000 LOC across `wander.rs`, `travel.rs`, `follow.rs`,
`escort.rs`, `guard.rs`, `patrol.rs`, `sandbox.rs`, `locomotion.rs`) was read
in full for the first time from a concurrency angle and introduces no lock-
ordering, scheduler-registration, or physics-RwLock hazard. The previously
confirmed-clean renderer/worker-thread machinery (Dimensions 1, 2, 6, 7)
remains unchanged and correct. Two already-filed, non-regressed items are
noted below for completeness; they are not re-reported as findings.

## Why the new M42 systems are safe (cross-dimension synthesis)

Three independent dimension sweeps (3, 4, 5) converged on the same underlying
fact from different angles, which is worth stating once at the top level:

- **All seven M42 systems are registered `add_exclusive(Stage::PostUpdate, …)`**
  in `byroredux/src/boot.rs` (lines 726/739/753/767/781/794/804), each gated
  behind its own env var (`BYRO_SANDBOX_SIT`/`BYRO_WANDER`/`BYRO_TRAVEL`/
  `BYRO_FOLLOW`/`BYRO_ESCORT`/`BYRO_GUARD`/`BYRO_PATROL`). Exclusive systems
  run serially, alone, after each stage's parallel batch — they can never be
  co-scheduled with each other, with `physics_sync_system` (`Stage::Physics`),
  or with any parallel-stage system. A repo-wide grep confirmed no second
  (parallel) registration exists for any of the seven.
- **Every system uses the same disciplined two-pass shape**: Pass 1 acquires
  only read guards (behavior/state/Transform components + `PhysicsWorld` via
  `try_resource`), collects plain-value decisions into a `Vec`, and drops all
  read guards before Pass 2 acquires each write storage as a separate,
  sequentially-scoped single-type lock. No write guard is ever held across
  acquisition of another storage or resource.
- **The shared `step_toward` / `step_oscillating_wander` locomotion
  primitive** (`locomotion.rs`) takes `physics: Option<&PhysicsWorld>` as a
  parameter (a read guard resolved once by the caller) and never re-enters
  the `World`, so it cannot double-lock any storage. Its only physics call is
  the read-only `PhysicsWorld::cast_ray_down` — none of the seven systems call
  any physics *mutator* (`set_linear_velocity`, `set_kinematic_translation`)
  at all.
- **Zero raw `.read()`/`.write()` lock calls** appear in any of the eight new
  files — every acquisition routes through the standard `World` query/resource
  API, which resolves `PoisonError` via `storage_lock_poisoned::<T>()` /
  `resource_lock_poisoned::<R>()` (fail-fast re-panic), and zero
  `World::insert`/`spawn`/`despawn` structural-mutation calls appear outside
  `#[cfg(test)]` modules (systems hold `&World`, which wouldn't compile
  otherwise).

One non-blocking **correctness nuance** surfaced independently by Dimension 5
(not a concurrency defect, not filed): `Stage::PostUpdate` (2) runs *before*
`Stage::Physics` (3) in stage order, so the M42 systems' ground-snap raycasts
read the *previous* frame's physics query-pipeline BVH state — at most
one-frame staleness of ground height, with no locking implication. Worth a
mental note for whoever next reasons about M42 stage placement, but out of
scope for a concurrency fix.

## Dimension summaries

### Dimension 1 — Vulkan Queue & Acceleration-Structure Sync
**Clean, 0 findings.** Re-verified fresh (not trusted from the prior audit):
single-Mutex queue submission holds the guard across `queue_submit`/
`queue_present` (`draw.rs:3924-3996`, #1713); frame-in-flight fence/semaphore
discipline intact (`draw.rs:2263-2345`, `sync.rs:79-99`); acquire→render→
present chain correct (per-image `render_finished`, `sync.rs:43-64`); every
AS build→read barrier present with correct access flags (`draw.rs:1924-1931,
2091-2098, 3008-3016`; `blas_static.rs:863-869`; `blas_skinned.rs:654-669`,
#1790); deferred BLAS-scratch destruction via `pending_destroy_scratch`
(#1782) and deferred AS destruction via `pending_destroy_blas` (#a476b256)
both confirmed, with shutdown draining via `device_wait_idle`; swapchain
recreate gated by `device_wait_idle` at phase-1 entry (`resize.rs:31-36`);
one-time command buffers confirmed off the per-frame hot path. The renderer
crate has not changed since 2026-07-05 (last commit touching it is a
docs-only audit `.md`), so this is a confirmation of a static, unchanged
state rather than new terrain.

### Dimension 2 — Compute → AS → Fragment Chains
**Clean, 0 findings.** The palette→skin→BLAS-refit→ray-query chain re-traced
end to end and intact, including resolving an apparent file-order red flag
(`record_skinned_blas_refit` is *defined* at `draw.rs:1562` but *called* at
`draw.rs:2977`, after the palette dispatch/barrier — correct execution
order despite lower line numbers appearing first in the file). Cross-frame
ping-pong (SVGF/TAA `prev = (f+1) % MAX_FRAMES_IN_FLIGHT`, volumetrics
per-FIF `lighting_volumes`/`integrated_volumes`) reads only the previous
frame's slot. Volumetrics `tlas_written`/`lights_written` latches are
set/assert/reset symmetric (#1105). Bloom's per-mip RAW chain and the final
up-mip→composite publish are present (#931). Caustic CLEAR→COMPUTE→FRAGMENT
and the MaterialBuffer SSBO HOST→shader upload (R1) are both correct. The
#1811/#1812 skin-chain skip optimizations were re-traced and remain correct.
The open #1874 ghosting investigation (RenderDoc-gated per the project's
speculative-fix guardrail) is unchanged — no new static evidence found, not
re-investigated here.

### Dimension 3 — ECS Lock Ordering & Deadlock
**Clean, 0 findings.** This was the primary new-code target. All eight new
M42 files (`locomotion.rs`, `wander.rs`, `travel.rs`, `follow.rs`,
`escort.rs`, `guard.rs`, `patrol.rs`, `sandbox.rs`) were read in full, plus
the shared nested resolver `crates/scripting/src/condition.rs::resolve_entity_by_global_form_id`
that travel/follow/escort/guard call mid-Pass-1. The nested resolver acquires
`FormIdPool`/`FormIdComponent` and the caller then reads `GlobalTransform` —
none of these types overlap the Pass-1 held set
(`*Behavior`/`Transform`/`*State`/`PhysicsWorld`), so there is no same-type
double-acquire and no `lock_tracker` conflict. `sandbox_seat_system` holds a
`resource_mut::<SeatReservations>()` write guard across its actor loop, but
`SeatReservations` is type-disjoint from every read guard held concurrently —
safe. The core `world.rs` TypeId-sorted acquisition invariant (`query_2_mut`
et al.) and `lock_tracker` same-thread reentrancy detection are unchanged and
correctly ordered; the M42 code adds no new multi-lock accessor to violate
them. Poisoning resolution (`storage_lock_poisoned`) is used uniformly; zero
raw lock calls in the new files.

### Dimension 4 — Scheduler Access Declarations (regression guard)
**Clean, 0 findings.** Independently re-verified (not accepted from Dimension
3) that all seven M42 systems are registered exactly once, each via
`add_exclusive(Stage::PostUpdate, …)` in `boot.rs` — a repo-wide grep found no
duplicate/parallel registration for any of them. Exclusive systems land in
`StageData.exclusive`, which `Scheduler::access_report`'s conflict loop never
pairs, so none of the seven can produce a `Conflict` or `Unknown` row; they
only add to the documentation-only `undeclared_exclusive_count()` KPI, not the
two KPIs (`known_conflict_count()`, `unknown_pair_count()`) that must stay
zero. The `AccessConflict` three-variant model (`None`/`Unknown`/`Conflict`,
no `Parallel` variant) is unchanged and unit-tested (12 tests, all passing).
Three in-binary `debug_assert_eq!(…, 0)` boot guards (`boot.rs:975-997`,
#1394/#1602) independently pin the two KPIs at every debug-build boot — a
stronger regression guard than a periodic audit. One documentation
correction (not a defect): `player_controller_system` is registered as a
*declared parallel* system (`add_to_with_access`), not exclusive as an older
version of this checklist assumed — this is the correct, intentional design
(it declares the union of its two inner systems' access sets), and the code
is sound. Pre-M42 exclusive systems (`audio_system`, `spin_system`) remain
exclusive; `cargo test -p byroredux-core --lib ecs::` passes 327/0.

### Dimension 5 — RwLock Patterns (Resource↔Storage, Physics Step)
**Clean, 0 findings.** Confirmed independently (from the physics-RwLock
angle) that none of the seven M42 systems call any `PhysicsWorld`-mutating
helper (`set_linear_velocity`, `set_kinematic_translation`); their only
physics contact is `step_toward`'s read-only `cast_ray_down` via a parameter-
passed `Option<&PhysicsWorld>`. `physics_sync_system`'s 4-phase structure
(`collect_newcomers` → `register_newcomers` → step → `pull_dynamic`) still
drops all storage read guards before taking the `PhysicsWorld` write guard
(`sync.rs:497` explicit `drop(pw)` before the subsequent `query_mut`), and the
two velocity/translation helpers still drop their `RapierHandles` read guard
(via a `Copy`-returning closure) before taking `resource_mut::<PhysicsWorld>()`.
All five call sites of those helpers (character.rs ×2, save_io.rs, camera.rs,
plus confirmed zero M42 call sites) hold no `PhysicsWorld` guard at the call.
`ContactConfig` is snapshotted once per batch, not re-locked per newcomer.
Cell-unload teardown (`release_victim_rapier_bodies`, #1520) still collects
under read guards, drops them, then takes the write guard, before the despawn
loop. Since `physics_sync_system` (`Stage::Physics`) and all seven M42 systems
(`Stage::PostUpdate`) are exclusive and run in distinct, ordered stages, no
cross-thread RwLock inversion is reachable — no deadlock cycle exists.

### Dimension 6 — Resource Lifecycle (GPU teardown ordering)
**Clean, 0 new findings.** Re-verified fresh from source (renderer unchanged
since 07-05): reverse-order Drop with the #1483 allocator-independent hoist
still correctly leaves nothing allocator-dependent destroyed after
`self.allocator.take()` (`context/mod.rs:2978-3273`); swapchain recreate
rebuilds every screen-sized resource (G-buffer, SVGF/TAA per-FIF history,
caustic, water-caustic, bloom, composite, egui framebuffers) with old
image-views destroyed after the new swapchain create but before the old one's
destroy (#654); volumetrics is correctly *not* recreated (its 160×90×128
froxel grid is screen-independent, only its composite-facing views rebind);
AS shutdown drains `pending_destroy_blas`/`pending_destroy_scratch` before
freeing BLAS entries, TLAS slots, and per-skinned-entity scratch; scene-buffer
SSBOs (including the R1 material buffer), mesh registry, texture registry,
and egui-pass all drain their deferred-destroy queues on `destroy()`; no
per-frame descriptor/buffer allocation was found outside the pre-allocated
per-FIF ring pattern. The one known #1861 (LOW, load-time one-shot command-
buffer/fence leak on error paths) is unchanged and not re-reported.

### Dimension 7 — Worker Threads (Streaming, Debug Server) & Thread-Safety Bounds
**Clean, 0 new findings.** Streaming Drop ordering (#1167): `shutdown()`
explicitly `take()`s the worker handle, then `take()`s and drops the request
sender before `join_with_timeout`, and `Drop` delegates to `shutdown` with a
short-circuit guard so double-invocation is safe regardless of struct field
declaration order. Worker↔main data flow: `PartialNifImport` is compile-time
pinned `Send` (`assert_send::<PartialNifImport>()`); BSA/BA2 `File` access is
`Mutex`-serialized for safe concurrent extraction from `Arc<TextureProvider>`;
`MaterialProvider::merge_bgsm_into_mesh` is called only from four main-thread
sites, never from the worker; the NIF import-cache read path uses a
point-in-time `Arc<HashSet<String>>` snapshot with write-back deferred to
`consume_streaming_payload` on the main thread. Debug server: per-client TCP
threads never touch `&World` (all mutation routes through the Late-stage
exclusive `DebugDrainSystem`); the command queue is bounded
(`MAX_QUEUED_COMMANDS = 64`, atomic check-then-push); the screenshot bridge's
owner-tag + generation-counter scheme (#1603/#1007/#1011) prevents a
cancelled capture's straggler result from landing in a later claimant's slot,
and drain/present only ever run on the main thread in frame order. Allocator
sharing: all ~39 `SharedAllocator.lock()` call sites are single-statement,
never span a `queue_submit`. `Send`/`Sync` bounds are compiler-enforced on
`Component`/`Resource` with zero manual `unsafe impl Send/Sync` anywhere in
the repo; `cxx-bridge` remains a 26-line stub with no live FFI surface to
audit; Ruffle's `SwfPlayer` wraps its `Player` in `Arc<Mutex<_>>` and is only
ever touched from the main loop.

## Already-filed items (not re-reported)

- **#1861** [OPEN, LOW] — `with_one_time_commands_inner` leaks the command
  buffer (and, on the owned-fence path, the fence) on three post-recording
  `?`-propagated error paths. Load-time one-shot path only, not per-frame —
  confirmed unchanged, not re-reported.
- **#1874** [OPEN, HIGH] — Ghosted diagonal double-image in TES interiors,
  sticks after the camera parks. Mechanism narrowed (shared bad motion vector
  + TAA parked-camera clamp bypass, SVGF/TAA reprojection) but origin
  unconfirmed; explicitly a RenderDoc-gated investigation per the standing
  speculative-fix guardrail, not actionable from static reading. Confirmed
  no new evidence surfaced this sweep.

## Report Finalization

- No new GitHub issues warranted from this sweep — zero findings across all
  seven dimensions.
- The new M42 AI-package system family (~2,000 LOC) has now had its first
  concurrency pass and is confirmed clean; future audits of this dimension
  can treat it as previously-swept ground rather than re-reading in full,
  unless it changes again.
- The two open items above (#1861, #1874) remain tracked elsewhere and are
  restated here only for cross-reference completeness.
- `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-07-16.md` — will be a
  no-op (zero findings); run only to confirm dedup against the existing
  issue set.
