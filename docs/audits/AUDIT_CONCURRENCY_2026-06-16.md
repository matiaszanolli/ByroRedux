# Concurrency & Synchronization Audit — 2026-06-16

**Focus**: dimensions 1, 2, 3 only (`--focus 1,2,3`).
**Depth**: deep.
**HEAD audited**: `fa569908` (branch `main`).
**Dedup baseline**: `gh issue list` (29 open issues) + `docs/audits/AUDIT_CONCURRENCY_2026-06-11.md`,
`…06-14.md`, `…05-16.md`.

## Summary

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| 1. Vulkan Queue & AS Sync | 0 | 0 | 0 | 0 |
| 2. Compute → AS → Fragment Chains | 0 | 0 | 0 | 0 |
| 3. ECS Lock Ordering & Deadlock | 0 | 0 | 0 | 0 |
| **Total** | **0** | **0** | **0** | **0** |

**No NEW findings, no regressions.** All three surfaces re-verified CLEAN by direct
code trace against current `main` — not by carrying forward prior verdicts. Per the
project's speculative-Vulkan-fix guardrail, no barrier/stage/layout change is proposed:
every Vulkan-sync edge below was confirmed present by reading the code, not by reasoning
about hypotheticals.

Two prior findings on the Dim-1 surface were re-confirmed FIXED (table at end).

---

## Dimension 1: Vulkan Queue & Acceleration-Structure Sync

**Result: CLEAN.** All six checklist items verified against current code.

1. **Queue submission single-Mutex — CLEAN.** `present_queue = Arc::clone(&graphics_queue)`
   on the common-family path; a distinct Mutex only on the distinct-family fallback
   (`crates/renderer/src/vulkan/context/mod.rs:1451-1457`). `vk::Queue` is `Copy`. All four
   submit/present sites bind the guard and deref `*queue` **inside** the call (guard NOT
   dropped before submit):
   - Main `queue_submit`: `crates/renderer/src/vulkan/context/draw.rs:3509-3538`
   - `queue_present`: `crates/renderer/src/vulkan/context/draw.rs:3567-3574`
   - egui internal submit: `crates/renderer/src/vulkan/context/draw.rs:3408-3424`
   - One-time-commands: `crates/renderer/src/vulkan/texture.rs:659-666`
   The historical CONC-D2-NEW-01 bug (`let q = *queue.lock()` dropping the guard before
   submit) is gone at every site.
2. **Frame-in-flight discipline — CLEAN.** Waits both `in_flight[frame]` and
   `in_flight[prev]` (`draw.rs:487-499`, #282); per-image fence tracking (`:551-566`);
   `reset_fences` deferred to just before submit (`:3483-3500`, #952); error arms recover
   the dangling acquire signal (#910).
3. **Acquire→render→present chain — CLEAN.** `image_available[frame]` waited at
   `COLOR_ATTACHMENT_OUTPUT`; `render_finished[img]` is **per swapchain image** (`:3468`,
   guards VUID-vkQueueSubmit-pSignalSemaphores-00067).
4. **AS build→read barriers — CLEAN.** Full chain present: skin compute → AS_BUILD
   (`draw.rs:1447`), skinned-refit self-emitted AS_WRITE→AS_WRITE scratch-serialise
   (`crates/renderer/src/vulkan/acceleration/blas_skinned.rs:417`), refit→TLAS
   AS_BUILD→AS_BUILD (`draw.rs:1597`), TLAS instance HOST→TRANSFER→AS_READ
   (`crates/renderer/src/vulkan/acceleration/tlas.rs:687`), TLAS→FRAGMENT|COMPUTE
   (`draw.rs:1714`). `STATIC_BLAS_FLAGS` is the shared `constants` const at every
   size-query/record site — CONC-D2-NEW-02 confirmed FIXED. No stale-device-address path.
5. **Swapchain recreate sync — CLEAN.** `device_wait_idle`
   (`crates/renderer/src/vulkan/context/resize.rs:23`) precedes every destroy.
6. **No blocking one-time submit in hot path — CLEAN.** Static BLAS builds and all
   `with_one_time_commands*` callers are load/init-time only; per-frame skinned BLAS rides
   the frame cmd (#911).

---

## Dimension 2: Compute → AS → Fragment Chains

**Result: CLEAN.** Method: diffed every Dim-2 entry-point file `435e265d..HEAD`, re-read
every changed path; confirmed unchanged files byte-identical so prior line-cited verdicts
carry forward.

The only substantive Dim-2 change since the last clean base is the **soft-particle
depth-history copy** (`crates/renderer/src/vulkan/context/draw.rs:311-404`, commit
1ddeae28), plus the ReSTIR reservoir removal (218b425b — a write-only attachment, not a
ping-pong member; removal adds no sync dependency).

**Depth-history copy analysis** — `copy_depth_to_history` snapshots the opaque depth buffer
into a single-buffered `depth_history_image` (`context/mod.rs:1105`), read by `triangle.frag`.
All four hazard edges guarded:
- Within-frame WAR: `SHADER_READ → TRANSFER` barrier (`draw.rs:330-338, 375-383`).
- Cross-frame RAW: same-queue submission order + `TRANSFER_WRITE → SHADER_READ` publish to
  `EARLY_FRAGMENT_TESTS | FRAGMENT_SHADER | COMPUTE_SHADER` (`draw.rs:392-402`).
- `depth_image` round-trip restore to `DEPTH_STENCIL_READ_ONLY_OPTIMAL` before SSAO/SVGF/
  composite read it (`draw.rs:3297/3082`, copy recorded at `:3053`).
- First-frame init clear to far-plane + resize destroy/recreate/reprime under
  `device_wait_idle` (`resize.rs:79-87, 225-232`).

Checklist re-verification: skin chain (M29), cross-frame ping-pong (SVGF/TAA/caustic/
water-caustic/volumetrics), volumetrics `tlas_written` latch set/reset symmetry
(`volumetrics.rs:982` / `:809-815`, #1105), bloom RAW chain (#931), caustic CLEAR→COMPUTE→
FRAGMENT, and MaterialBuffer SSBO (R1) — all CLEAN.

*Non-finding (out of scope):* `crates/renderer/src/vulkan/context/resize.rs:242-243` still
has a stale "7 color … + reservoir" comment after the ReSTIR cut. Doc-rot for a tech-debt
sweep, not a sync bug.

---

## Dimension 3: ECS Lock Ordering & Deadlock

**Result: CLEAN.** All four checklist items verified against current code.

1. **TypeId-sorted acquisition — CLEAN.** `crates/core/src/ecs/world.rs` `query_2_mut` /
   `query_2_mut_mut` / resource-pair accessors acquire storage locks in TypeId-ascending
   order with `lock_tracker` scope guards set up in the same order (#313); same-type access
   still trips the `assert_ne!` panic.
2. **lock_tracker coverage — CLEAN.** Same-thread re-entrant detection always-on in debug;
   cross-thread global lock-order graph opt-in via `BYRO_LOCK_ORDER_CHECK=1`. The
   `lock-order-check` CI job sets the env var (`docs/contributing.md`); no test path that
   should set it was found missing it.
3. **Guard lifetime in system bodies — CLEAN (highest-yield check).** Walked every
   parallel-batch system body. Only Stage::Early and Stage::Late run more than one parallel
   system:
   - **Early** (`player_controller_system`, `weather_system`, `timer_tick_system`):
     `weather_system` (`byroredux/src/systems/weather.rs:330`) touches only resources, all
     scoped/dropped before re-acquire. The three members have **disjoint storage sets** — no
     constructible ABBA edge.
   - **Late** (`camera_follow_system`, `reverb_zone_system`, `log_stats_system`,
     `metrics_sample_system`): only `camera_follow_system`
     (`byroredux/src/systems/character.rs:358`) touches storage — `GlobalTransform`(read) +
     `CharacterController`(read) held together (lines 384-404), dropped, then `Transform`(write)
     alone, then `GlobalTransform`(write) alone, each in its own block; no re-entry of a held
     storage. The other three touch only resources, so no ABBA partner exists.
   - `animation_system` is the sole Update parallel system; its nested
     `build_subtree_name_map` keeps a consistent `NameIndex → Name → Children` order with no
     held-storage re-entry.
   - No `World::insert` (structural `&mut self`) from any scheduled system body; the
     `world.insert*` calls in `byroredux/src/systems/audio.rs:235-263` are in the setup/spawn
     helper, not a scheduled-system path.
   The declared-access invariants (`undeclared_parallel_count == 0`,
   `known_conflict_count == 0`, `unknown_pair_count == 0`) are `debug_assert`-pinned at
   schedule construction (`byroredux/src/main.rs:945-968`, #1394/#1602), and each batch
   member's actual locks are a subset of its declaration with disjoint storage sets across
   the batch.
4. **Poisoning — CLEAN.** Storage `RwLock` acquisitions resolve `PoisonError` through
   `storage_lock_poisoned::<T>()` (re-panics with diagnostic); no path silently unwraps a
   poisoned guard into torn state.

---

## Prior findings status (Dim 1)

| Finding | Source | Current status |
|---|---|---|
| CONC-D2-NEW-01 (queue guard `*queue.lock()` drop before submit) | 2026-05-16 | **FIXED** — all 4 sites bind guard, deref inside call |
| CONC-D2-NEW-02 (`STATIC_BLAS_FLAGS` inline-literal duplication) | 2026-05-16 | **FIXED** — shared const at every size-query/record site |

## Dedup

No OPEN issue in the 29-issue baseline matches a Dim-1/2/3 premise. #1603 (screenshot-latch
residual) is CPU/debug-side, out of scope for these dimensions. Prior concurrency audits
(`2026-06-11`, `2026-06-14`) recorded the same surfaces CLEAN; this pass re-verified against
current `main` rather than trusting the carry-forward.

---

*Next step suggestion:* no findings to publish. If a full-spectrum sweep is wanted, run the
remaining dimensions: `/audit-concurrency --focus 4,5,6,7`.
