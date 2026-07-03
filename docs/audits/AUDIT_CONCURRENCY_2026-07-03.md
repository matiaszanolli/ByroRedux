# Concurrency & Synchronization Audit — 2026-07-03

**Scope**: Vulkan queue/AS sync, compute→AS→fragment chains, ECS lock ordering,
scheduler access declarations, physics RwLock patterns, GPU resource lifecycle,
worker threads. All 7 dimensions run at `--depth deep` against HEAD `8498e559`.

**Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md` (prior day, commit
`1b4e8e84`). Since that report, 17 commits landed on `main`, and unusually many
of them touch exactly this audit's surface — the shared BLAS scratch buffer,
the skinned-BLAS refit barrier, first-sight `bind_inverses` bookkeeping, the
pose-hash dirty gate, and mid-batch BLAS eviction budget accounting:

```
6245106c Fix #1782: defer BLAS scratch-buffer destruction until in-flight frames retire
d688fe06 Fix #1790: add the missing AS_READ bit to the skinned-BLAS scratch barrier
af6e4c9b Fix #1791: requeue drained first-sight bind_inverses on a draw_frame early return
e682f78c Fix #1792: thread pending_bytes through evict_unused_blas so mid-batch eviction actually reclaims
8cd6636f Address #1793: document the two budget-eviction correctness gaps at their exact sites
e040231a Fix #1796: roll back the pose-hash commit when draw_frame bails before dispatch
dfd87730 Address #1797: document the quantify-before-fixing gate on the shared BLAS scratch barrier
c7c1fe1d Fix #1794: stop re-filling bone_world's identity padding every frame
e3e9df0d Fix #1795: quantize particle color fade to restore MaterialTable dedup
d68c86c9 Fix #1803: remove dead GlobalTransform probe in emit_particles
9f48a16e Fix #1804: gate the two-sided blend split on z_write
```

Every one of these commits was individually re-verified against the current
tree rather than trusted from its commit message. **The headline result: the
prior CRITICAL finding (CONC-D1-01 / #1782, shared BLAS-scratch-buffer
use-after-free) is confirmed fixed and complete** — all three grow/shrink call
sites now route through the `pending_destroy_scratch` deferred queue, which is
drained on both the per-frame tick and every shutdown path. None of the six
follow-up commits introduced a new concurrency bug; #1790's barrier widening,
#1791/#1796's requeue/rollback bookkeeping, and #1792's eviction-budget
threading were all traced end-to-end and found correct.

**Verdict**: Zero CRITICAL, zero HIGH. One MEDIUM re-confirmed
(`Existing: #1783`, skin_palette/skin_compute independent-init-failure
coupling gap — unchanged since 07-02, not touched by any of the listed
commits). Six LOW findings re-confirmed as still-accurate, already-open
issues (`#1784`–`#1789`). One new minor LOW observation on a fatal-error
code path. Two dimensions (5 — physics RwLock, 7 — worker threads) are fully
clean with zero findings, unchanged from 07-02.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 7 |
| **Total**| **8** |

Dedup baseline: `/tmp/audit/concurrency/issues.json` (`gh issue list --repo
matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels`, 71
issues). Every finding below was checked against this list plus a live
`gh issue view` on each candidate number. `#1782`, `#1790`, `#1791`, `#1792`,
`#1793`, `#1796`, `#1797` are all confirmed **CLOSED** (fixed) as of this
report and are not re-listed as findings — see the "Verified fixed since
07-02" section. `#1783`–`#1789` are all confirmed **OPEN** and still
accurate against current source.

---

## Findings

### CONC-D2-01: `skin_palette` init failure is not coupled to `skin_compute` — skin chain can run against a never-populated palette SSBO

- **Severity**: MEDIUM
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:~1800-1841` (two independent `match ... Ok/Err -> Some/None` init gates), `crates/renderer/src/vulkan/context/draw.rs:~2702` (palette dispatch gated on `skin_palette` only), `crates/renderer/src/vulkan/context/draw.rs:~2762` (`record_skinned_blas_refit` gated on `skin_compute` + `accel_manager` only)
- **Status**: Existing: #1783 (OPEN). Re-verified against HEAD `8498e559` — the two init gates are still fully independent; nothing in the 07-02→07-03 commit window touched either gate or either consumer's guard condition.
- **Description**: `SkinComputePipeline` and `SkinPaletteComputePipeline` are created by two independent `Ok`/`Err` matches, each degrading to `None` on failure with only a `log::warn!`. If `skin_palette` init fails while `skin_compute` succeeds (a partial mid-init failure), the palette dispatch is skipped (gated on `skin_palette.is_some()`) but `record_skinned_blas_refit` still runs (gated only on `skin_compute` + `accel_manager`), dispatching `skin_vertices.comp` against a bone-palette SSBO (`create_device_local_uninit`) that no GPU pass ever wrote.
- **Evidence**: Unchanged from the 07-02 report; re-read directly this cycle, line numbers hold at the `~` precision above.
- **Impact**: Garbage bone matrices → garbage skinned vertices → BLAS built/refit over garbage geometry → garbage skinned silhouettes in RT shadows/reflections/GI and the inline-skinned raster path. Not a memory-safety bug — all accesses stay in-bounds of allocated buffers.
- **Trigger Conditions**: `SkinPaletteComputePipeline::new` fails while `SkinComputePipeline::new` succeeds on an RT-capable device (rare partial-init failure), then any skinned draw.
- **Verification Path**: Fault-inject an `Err` into `SkinPaletteComputePipeline::new`, run `docs/smoke-tests/m41-equip.sh`; observe garbage/collapsed NPC geometry. Host-side init-coupling fix, not a barrier/stage change — exempt from the Vulkan speculative-fix guardrail.
- **Related**: None new.
- **Suggested Fix**: Force `skin_compute = None` if `skin_palette.is_none()` after the second match, or gate both consumers on the same `skin_compute.is_some() && skin_palette.is_some()` predicate.

---

### CONC-D3-01: `World` accessor docs claim the same-thread lock tracker is "release no-op" — it is compiled and active in release builds

- **Severity**: LOW
- **Dimension**: ECS Lock Ordering
- **Location**: `crates/core/src/ecs/world.rs` — `query`/`query_mut` (~271-398), `has`/`count` (~311-322), `try_resource`/`try_resource_mut` (~687-707), and (scope addition confirmed this cycle) `resource`/`resource_mut`/`resource_2_mut` (~569-622) all carry the same "debug only" / "zero-cost no-op" wording, vs. `crates/core/src/ecs/lock_tracker.rs` (module doc: "Thread-local check — always on, debug and release").
- **Status**: Existing: #1784 (OPEN). Re-verified — still accurate. Note for the issue: the doc-rot footprint is slightly larger than the original filing enumerated — `resource`, `resource_mut`, and `resource_2_mut`'s "In debug builds the `lock_tracker` additionally panics" wording is the same class of error and should be corrected in the same pass.
- **Description**: `track_read`/`track_write` carry no `cfg(debug_assertions)` gate and panic on a same-thread conflicting acquisition in release too; only the cross-thread `global_order` ABBA graph is debug-only, and that graph is further gated behind `BYRO_LOCK_ORDER_CHECK=1`.
- **Evidence**: `lock_tracker.rs` `#[cfg(debug_assertions)]` wraps only the `held_others` collection + `global_order::record_and_check` call, not `track_read`/`track_write` themselves.
- **Impact**: Documentation-only; misleads maintainers into thinking release builds have no re-entrancy protection and no per-acquisition cost.
- **Trigger Conditions**: None at runtime.
- **Verification Path**: `cargo test --release -p byroredux-core lock_tracker::tests::write_then_write_same_type_panics` passes in release.
- **Related**: CONC-D3-02/04 (same declaration-trust surface).
- **Suggested Fix**: Rewrite all ten "Panics (debug only)" headers (the seven originally enumerated plus `resource`/`resource_mut`/`resource_2_mut`) to state the thread-local check is always-on; only the cross-thread graph is debug+env-gated.

---

### CONC-D3-02: `animation_system` access declaration omits three color-sink component writes

- **Severity**: LOW (latent — animation is the only parallel system in `Stage::Update` today)
- **Dimension**: ECS Lock Ordering / Scheduler Access Declarations (declaration drift)
- **Location**: `byroredux/src/main.rs:~802-812` (declaration) vs. `byroredux/src/systems/animation.rs:~108-175` (`apply_color_channels` writes)
- **Status**: Existing: #1785 (OPEN). Re-verified — still accurate; unchanged since 07-02.
- **Description**: `apply_color_channels` writes `AnimatedDiffuseColor`, `AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor`, and `LightSource`. The `animation_system` declaration lists Diffuse, Emissive, and LightSource, but omits Ambient, Specular, and Shader.
- **Impact**: A future `Stage::Update` parallel system touching any of the three undeclared sinks would be co-scheduled as "no conflict" by the analyzer — a real cross-thread write-write window the startup asserts can't see (declaration-level gap, not acquisition-level).
- **Trigger Conditions**: Requires a future parallel Update-stage system touching these types — latent today.
- **Verification Path**: `BYRO_LOCK_ORDER_CHECK=1` won't catch it (declaration-level); code review only.
- **Related**: CONC-D4-01 (sibling declaration-completeness gap, different system).
- **Suggested Fix**: Add `.writes::<AnimatedAmbientColor>().writes::<AnimatedSpecularColor>().writes::<AnimatedShaderColor>()` to the `animation_system` declaration.

---

### CONC-D3-04: `CommandRegistry` read guard held across arbitrary command execution; `help` re-enters the same lock

- **Severity**: LOW (latent — benign today: read-read only, no runtime writer of `CommandRegistry` exists)
- **Dimension**: ECS Lock Ordering
- **Location**: `crates/debug-server/src/evaluator.rs:~413-416`, `byroredux/src/main.rs:~268-269` and `~2726-2727`; re-entry at `byroredux/src/commands/world_info.rs:17`
- **Status**: Existing: #1786 (OPEN). Re-verified — still accurate.
- **Description**: All three command-dispatch sites hold a `ResourceRead<CommandRegistry>` while calling `reg.execute(world, expr)`. `HelpCommand::execute` re-acquires the registry read-only under that outer guard. Confirmed via repo-wide grep: zero `resource_mut::<CommandRegistry>()` / `try_resource_mut::<CommandRegistry>()` call sites exist, so this is currently safe (read-read is re-entrant in both `RwLock` and the always-on thread-local tracker).
- **Impact**: Latent footgun — the day a command acquires `CommandRegistry` mutably (e.g. runtime alias registration), the same-thread tracker panics (by design, converting what would otherwise be a `std::sync::RwLock` platform-dependent deadlock hazard into a loud crash).
- **Trigger Conditions**: A future runtime `CommandRegistry` writer, or a command that acquires it mutably.
- **Verification Path**: `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace` records the `CommandRegistry → X` edge; a write-under-read case panics via the thread-local tracker at the offending line.
- **Related**: CONC-D3-01 (same tracker mechanism).
- **Suggested Fix**: Document the contract on `ConsoleCommand::execute` ("runs under a read guard on `CommandRegistry` — must never acquire it mutably"); optionally have `HelpCommand` receive the listing from the dispatcher instead of re-locking.

---

### CONC-D4-01: `physics_sync_system` under-declares its read surface (`ContactConfig` + the #1698 faller-dump reads)

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `crates/physics/src/sync.rs:371` (`ContactConfig` read in `register_newcomers`) and `crates/physics/src/sync.rs:244` (`FormIdPool` read in the `#1698` `dump_awake_fallers` diagnostic path, gated behind `BYRO_PROFILE_FALLERS`) vs. `byroredux/src/main.rs:887-908` (declaration)
- **Status**: Existing: #1787 (OPEN). Re-verified — still accurate. `physics_sync_system` remains the sole `Stage::Physics` parallel registration, so no live pairing exists; both undeclared accesses are read-only.
- **Description**: The declared access surface omits `ContactConfig` and `FormIdPool` resource reads that the body actually performs.
- **Impact**: No live hazard today. Latent "declared-but-incomplete surface" class: a future parallel writer of either resource would have `analyze_pair` return `None` (both sides "declared", no visible overlap) instead of `Conflict` — invisible to the startup asserts, which only catch *undeclared systems* and *declared conflicts*, not incomplete declarations.
- **Trigger Conditions**: Requires a future parallel writer of `ContactConfig`/`FormIdPool` in a co-scheduled stage. Not triggerable in the current schedule.
- **Verification Path**: `sys.accesses` shows the declared row without these entries; startup asserts stay green (can't see this class).
- **Related**: CONC-D3-02 (same declaration-completeness class, different system).
- **Suggested Fix**: Append `.reads_resource::<byroredux_physics::ContactConfig>()` and `.reads_resource::<FormIdPool>()` to the `physics_sync_system` registration in `main.rs`.

---

### CONC-D4-02: `DebugDrainSystem` is registered after the access-report/`SystemList` snapshot — omitted from `sys.accesses` and `systems` output

- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/main.rs:1012` (access-report snapshot) / `~1047` (`SchedulerAccessReport` resource stored) vs. `byroredux/src/main.rs:1083` (`byroredux_debug_server::start(...)` registers `DebugDrainSystem`); `crates/debug-server/src/lib.rs:33` (`add_exclusive(Stage::Late, drain_system)`)
- **Status**: Existing: #1788 (OPEN). Re-verified — still accurate, unchanged since 07-02.
- **Description**: `App::new` snapshots `scheduler.access_report()`/`system_names()` before `debug_server::start()` registers `DebugDrainSystem`, so the drain system never appears in `sys.accesses` or `systems` output.
- **Impact**: Introspection completeness only. `DebugDrainSystem` is exclusive with no declared `access()`, so it's never paired by the analyzer and the startup asserts are unaffected — it didn't exist yet when they ran, and exclusive+undeclared is permitted by design (#1237).
- **Trigger Conditions**: Always, on every debug-mode launch — an operator running `sys.accesses`/`systems` sees one Late-stage exclusive entry missing.
- **Verification Path**: `cargo run -- --bench-hold` + `byro-dbg` → `systems`/`sys.accesses`; count Late-stage exclusive rows vs. `Scheduler::system_names()` after `start()`.
- **Related**: None.
- **Suggested Fix**: Move the `SchedulerAccessReport`/`SystemList` snapshot after `debug_server::start()`, or have `sys.accesses` append a note for post-snapshot exclusive registrations. Cosmetic.

---

### CONC-D6-01: Stale `context/mod.rs` line-number citations in `acceleration/mod.rs::destroy()` comments — drifted further, not fixed

- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs:~269` and `~310-311`
- **Status**: Existing: #1789 (OPEN). Re-verified — still stale, and the drift has **increased** since the 07-02 report (which itself already found the citations stale relative to an even earlier baseline).
- **Description**: The doc comments on `AccelerationManager::destroy()` cite `context/mod.rs:1300`, `:1859`, and `:2093` as the `device_wait_idle()` call sites that make its immediate (non-deferred) destroys safe. The actual current call sites are `context/mod.rs:2534` (inside `flush_pending_destroys`, function starts at `:2525`) and `context/mod.rs:2849` (inside `impl Drop for VulkanContext`, function starts at `:2844`) — both have shifted again since the 07-02 report's cited `:2521`/`:2836`, confirming these are refactor-fragile line-number references, not the underlying invariant.
- **Impact**: None functionally — the invariant itself (both call sites `device_wait_idle` immediately before the immediate-destroy calls this comment describes) is confirmed still correct and held. Documentation/traceability defect only; a future reader chasing the comment lands on unrelated code.
- **Trigger Conditions**: N/A — static documentation drift, worsens with every refactor near either cited region.
- **Verification Path**: `grep -n "device_wait_idle" crates/renderer/src/vulkan/context/mod.rs`.
- **Related**: CONC-D1 (BLAS-scratch fix, same file family — see "Verified fixed since 07-02" below for the scratch-queue shutdown-drain confirmation, which is otherwise clean).
- **Suggested Fix**: Replace the three literal line numbers with symbolic references (e.g. "see `flush_pending_destroys` / `VulkanContext::Drop`") so future refactors can't re-stale them. Bundle with the next touch of this file.

---

### CONC-D1-NEW-01: First-sight `bind_inverses` requeue/rollback is skipped on a fatal `queue_submit`/`queue_present` `Err` path (informational, negligible impact)

- **Severity**: LOW
- **Dimension**: Vulkan Queue & AS Sync / Compute → AS → Fragment Chains (skin-chain bookkeeping)
- **Location**: `byroredux/src/main.rs:~1863-1867` (rollback/requeue only runs on the `Ok` arm of `draw_frame`'s result) vs. `crates/renderer/src/vulkan/context/draw.rs:~3735` (`queue_submit` failure) and the `queue_present` failure path
- **Status**: NEW (minor, found while re-verifying #1791/#1796's completeness; not previously reported)
- **Description**: `skin_dispatch_ran` is set `true` inside `record_skinned_blas_refit` (`draw.rs:1462`), called well before the frame's `queue_submit`/`queue_present` (`draw.rs:~3735`/`~3767`). The #1791/#1796 requeue-and-rollback logic in `main.rs` only runs on `draw_frame`'s `Ok` arm; on the `Err` arm (`main.rs:1916`, which logs and calls `event_loop.exit()`), the requeue of drained first-sight `bind_inverses` and the pose-hash rollback never fire, even though the recorded skin dispatch never reached the GPU (a fatal error at submit/present, not a completed frame).
- **Impact**: One-shot CPU-side bookkeeping loss (a first-sight entity's palette re-upload could be skipped on the next frame — if there is one) on a path that is itself fatal: `queue_submit`/`queue_present` failure triggers `event_loop.exit()` in the same tick. No use-after-free, no data race, no persistent corruption — the engine is tearing down. This is squarely below the "recoverable path" bar the severity scale reserves MEDIUM for.
- **Trigger Conditions**: `vkQueueSubmit` or `vkQueuePresentKHR` returning an error other than `ERROR_OUT_OF_DATE_KHR` (which has its own recovery path) — essentially a fatal device/driver failure.
- **Verification Path**: Code-level only; not practically reproducible without fault-injecting the Vulkan loader, and not worth doing given the shutdown-imminent context.
- **Related**: #1791, #1796 (the fix this is a residual gap in).
- **Suggested Fix**: None recommended — flagging for completeness only. If ever addressed, move the rollback/requeue call to run unconditionally (both arms) before returning from the frame driver, but this is optional hardening on a path that is about to exit the process regardless.

---

## Verified fixed since 07-02 (not findings — confirmation only)

### #1782 (was CONC-D1-01, CRITICAL) — Shared BLAS-scratch-buffer use-after-free: CONFIRMED FIXED, complete

All three retirement sites (`build_blas` single-shot grow at `blas_static.rs:318-320`, `build_blas_batched` streaming-hot-path grow at `blas_static.rs:770-772`, and `shrink_blas_scratch_to_fit`'s both arms at `memory.rs:72-78`/`89-91`) now push the retired buffer into `pending_destroy_scratch` (`DeferredDestroyQueue<GpuBuffer>`) instead of calling `old.destroy(...)` immediately. The queue is ticked every frame in `tick_deferred_destroy` (`blas_static.rs:90-92`), which runs after the double `wait_for_fences` (`draw.rs:2098`→`2217`), with a `DEFAULT_COUNTDOWN` of `MAX_FRAMES_IN_FLIGHT` (2) — strictly conservative versus the 2 frames-in-flight that could reference the old scratch's device address. It is also drained unconditionally on both shutdown paths (`AccelerationManager::destroy` and `VulkanContext::flush_pending_destroys`), each immediately after a `device_wait_idle`, so no retired scratch buffer can leak past process exit regardless of the deferred countdown (this was independently re-verified from the Dimension-6 teardown-correctness angle, not just the Dimension-1 sync angle). `build_skinned_blas_batched_on_cmd`'s own grow-destroy remains deliberately immediate and is correctly *not* flagged — it runs after that frame's own fence wait, with an in-code comment warning against "fixing" it the same way.

### #1790 (was a live sync-validation hazard) — Skinned-BLAS scratch barrier AS_READ bit: CONFIRMED FIXED

`record_scratch_serialize_barrier` (`blas_skinned.rs:654-671`) now carries `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR` in its dst mask, closing the same-command-buffer first-sight BUILD → UPDATE-refit RAW hazard on `srcAccelerationStructure` that sync-validation had flagged. The change only widens visibility, so it cannot introduce a new hazard.

### #1791 / #1796 — bind_inverses requeue + pose-hash rollback on `draw_frame` early return: CONFIRMED FIXED, no new state-tracking bug

Traced the full state machine across `main.rs` and `draw.rs`: `skin_dispatch_ran` resets to `false` before both `Ok`-arm early-return guards (empty-framebuffers, `ERROR_OUT_OF_DATE_KHR`) and is set `true` only after the bind-inverse upload completes with no early-return window in between — so "upload happened" always implies "flag true," ruling out double-upload or double-requeue. `pending_for_requeue` is consumed via `mem::take` (single-shot) and is pre-filtered to entities whose `SkinnedMesh` still exists. Pose-hash rollback correctly restores the pre-frame baseline via `.entry(entity).or_insert(old)` semantics, verified against the in-tree regression tests. The one residual gap (fatal `Err`-arm loss) is reported above as CONC-D1-NEW-01 — informational only.

### #1792 — `evict_unused_blas` `pending_bytes` threading: CONFIRMED FIXED, no double-free/double-count

`pending_bytes` is read-only input to the eviction gate and loop-break condition; it is never added into the committed byte totals, and evicted entries still route through the existing `pending_destroy_blas` deferred queue (no immediate `destroy_acceleration_structure` reintroduced).

### #1793 / #1797 — documentation-only commits: no behavioral change, correctly not re-derived as bugs

Both commits add explanatory comments (the two LRU/throughput gaps they document are CPU-side visual-completeness/performance concerns, not data races or use-after-free — out of this audit's severity floor). `#1793` is confirmed CLOSED in the tracker (the eviction-rebuild-path gap it documents is tracked, not silently dropped).

### #1794, #1795, #1803, #1804 — no concurrency surface

Read for completeness: `bone_world` identity-padding skip (#1794), particle color-fade quantization for `MaterialTable` dedup (#1795 — confirmed it does **not** relocate the `MaterialBuffer` SSBO upload timing), dead `GlobalTransform` probe removal (#1803), and two-sided blend-split gating (#1804) are all single-threaded CPU-side logic changes with no lock, barrier, or cross-thread implication.

---

## Cross-referenced / dedup'd (not separately counted)

- **`#1837`** (ECS-2026-07-02-04, `insert_resource` silently swallows a poisoned prior-value lock) — filed by the companion ECS audit, re-confirmed present exactly as described during Dimension-3 review here. Not double-counted; requires `&mut World` (post-`catch_unwind` recovery path only), no live-system exposure.
- **`#1793`** — cross-referenced above under "Verified fixed since 07-02" (documentation commit); it is a BLAS-eviction LRU/rebuild-path completeness concern, not a Dimension-1/6 concurrency defect, and is out of this audit's severity floor.

---

## Dimension summaries

### Dimension 1 — Vulkan Queue & Acceleration-Structure Sync
Zero CRITICAL/HIGH. #1782's fix verified complete across all three call sites with drain coverage confirmed on both the per-frame tick and every shutdown path. All 8 checklist items PASS: single-Mutex queue submission correctly *holds* the guard across `queue_submit`/`queue_present` (per VUID-vkQueueSubmit-queue-00893 external-synchronization requirement — the checklist's phrasing describing a copy-out-then-drop pattern does not match current code, which is the *correct* Vulkan-spec-compliant shape); both-slot fence discipline and per-image `render_finished` semaphores intact; AS build→read and AS build-INPUT barriers correct-flag at every site; deferred AS-object destruction (#a476b256) holds, unperturbed by #1792's eviction-budget change; swapchain-recreate wait-idle-first ordering intact; no blocking one-time submit in the per-frame hot path. One new minor LOW (CONC-D1-NEW-01) on a fatal-error bookkeeping edge case.

### Dimension 2 — Compute → AS → Fragment Chains
One MEDIUM re-confirmed (CONC-D2-01 / #1783), unchanged and untouched by the 07-02→07-03 commit window. #1790's barrier fix and #1791/#1796's requeue/rollback logic verified complete and correct, with no new double-write, double-requeue, or hash-skew bug. Cross-frame ping-pong indices (SVGF/TAA/caustic/water-caustic/volumetrics), the volumetrics `tlas_written` latch, the bloom within-frame RAW chain, and the caustic CLEAR→COMPUTE→FRAGMENT sequence are all untouched by recent commits and remain correct. #1795 confirmed not to have moved `MaterialBuffer` SSBO upload timing.

### Dimension 3 — ECS Lock Ordering & Deadlock
Core invariant holds — no HIGH/MEDIUM. TypeId-sorted acquisition intact across all four multi-lock accessors (`query_2_mut`, `query_2_mut_mut`, `resource_2_mut`, `try_resource_2_mut`); same-type access still panics via `assert_ne!`; the #313 ABBA-edge guard holds; the `lock-order-check` CI job (`.github/workflows/ci.yml:47-58`) is current. New scripting (#1768) and CHARAL systems (`character_controller_system`) swept for guard-lifetime/declaration violations — all scripting additions are `add_exclusive` (serial, no composition risk); CHARAL's controller declares its access fully. Three LOW re-confirmed as accurate, already-open issues (#1784/#1785/#1786); `#1784`'s doc-rot scope is slightly larger than originally filed (extends to `resource`/`resource_mut`/`resource_2_mut`).

### Dimension 4 — Scheduler Access Declarations (regression guard — M27 closed)
Two LOW re-confirmed (#1787/#1788). Conflict model sound (`None`/`Unknown`/`Conflict`, no `Parallel` variant; undeclared ⇒ `Unknown` ⇒ serialise) — 13 unit tests pin every branch. The #1394/#1602 startup guard runs inside `App::new` before the event loop; every parallel-batch registration uses `add_to_with_access`; all Late-stage/Early-stage parallel batches inspected and conflict-free. CHARAL has not added a registered system (data-model only); the #1768 scripting systems are correctly `add_exclusive`. `player_controller_system` (the M27 Phase-3 fly-camera/character-controller merge) is correctly registered parallel — it has no sibling to conflict with, so exclusivity isn't required; not a regression.

### Dimension 5 — RwLock Patterns (Resource↔Storage, Physics)
**Zero findings**, re-verified directly against current source (not carried forward on trust). `physics_sync_system`'s 4-phase collect-then-register discipline, helper lock order (`set_linear_velocity`/`set_kinematic_translation` drop the `RapierHandles` read guard before taking `PhysicsWorld` write), single-snapshot `ContactConfig`, cell-unload teardown (`release_victim_rapier_bodies`), and ragdoll teardown (`activate_ragdoll`, #1772) all hold. The only physics/ragdoll-adjacent change since 07-02 (`ffe9a816`, ragdoll bone-name-miss logging) touches only a lock-free pure function (`template_from_imported`) — zero surface area for this dimension. Single-threaded `Stage::Physics` placement confirmed (sole registrant).

### Dimension 6 — Resource Lifecycle (GPU teardown ordering)
One LOW re-confirmed (#1789), and the drift has *increased* since 07-02 (cited line numbers moved again with the intervening refactors — see the finding for current actual sites). The key open resource-lifecycle question from #1782's fix — does `pending_destroy_scratch` actually get drained on shutdown, or could it leak — resolves cleanly: both shutdown routes (`AccelerationManager::destroy`, `VulkanContext::flush_pending_destroys`) drain it unconditionally, and both are gated behind a preceding `device_wait_idle`. The three post-07-02 BLAS/eviction commits (#1791/#1792/#1793) introduced no teardown-ordering regression. Reverse-order destruction, allocator-freed-last, and the #1483 hoist all spot-checked clean.

### Dimension 7 — Worker Threads & Thread-Safety Bounds
**Zero findings.** `git log 1b4e8e84..HEAD` touches none of `streaming.rs`, `debug-server/`, `debug-ui/`, `allocator.rs`, `crates/ui/`, or `crates/cxx-bridge/`. The sole nearby diff (`crates/core/src/ecs/resources.rs`, the `SkinSlotPool` pose-hash-rollback bookkeeping for #1791/#1796) is a plain owned `HashMap` field reached only via the existing main-thread `resource_mut` path — no new thread, channel, Mutex, or Send/Sync-relevant type. Streaming Drop ordering (#1167), worker↔main channel-only data flow, `Arc<TextureProvider>` Mutex-serialized extract, main-thread-only BGSM resolution, debug-server bounded 64-command queue and fence-gated screenshot readback, and `SharedAllocator` never held across a queue submit are all re-verified directly against current source and unchanged.

---

## Report Finalization

Recommended next step:

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-07-03.md
```

Note for the publish step: seven of the eight findings in this report are
already tracked as OPEN GitHub issues (`#1783`–`#1789`) from the 07-02 audit's
publish pass — only `CONC-D1-NEW-01` is a candidate for a genuinely new issue,
and given its negligible impact (fatal-shutdown-path bookkeeping only), filing
it is optional/low-priority.
