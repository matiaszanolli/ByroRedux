**HEAD**: `f97775ca8` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md` (@ `b3db49fa`, 385 commits ago) · **Audited**: Dims 1–7 (every dimension's `Paths:` had commits since the baseline) · **Unchanged since baseline (skimmed)**: none at the dimension level. Within dimensions, these had zero commits and got a guard spot-check only: `crates/core/src/ecs/{world,lock_tracker,scheduler,access}.rs`, `byroredux/src/extensions/`, `crates/debug-server/src/{listener,system}.rs`, `crates/renderer/src/vulkan/allocator.rs`.

# Concurrency & Synchronization Audit — 2026-09-21

**Command**: `/audit-concurrency` (default scope, depth deep), one leg of `/audit-suite --preset comprehensive`.
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Order.** One auditor ran the dimensions one at a time, with no sub-agents. Per-dimension notes are in
  `/tmp/audit/concurrency/dim_{1..7}.md`.
- **Dim 1 reuse.** Dim 1 was completed by an earlier run of this audit that was killed partway through
  (`dim_1.md`, with a guard-test log: 51 renderer tests pass). I re-verified every claim in it against HEAD
  and reused it. The two Dim 1 findings below came out of that re-check and the Dim 2 work, and are
  appended to `dim_1.md`.
- **Evidence other than code reading:**
  - CI logs for main-branch runs from 09-14 to 09-21, read with `gh run view` (read-only). This includes
    HEAD's run `35658431384`, whose `lock-order-check` job runs every lock-relevant test binary under
    `BYRO_LOCK_ORDER_CHECK=1`.
  - The sibling ECS leg's local `BYRO_LOCK_ORDER_CHECK=1` runs of the core and bin suites.
  - No engine launch. No workspace build. I did not run cargo locally, because of the shared RAM budget
    and because two independent green runs of every relevant suite already exist.
- **Dedup sources:**
  - `/tmp/audit/issues.json` (4,458 issues) and `gh issue view 4546`.
  - `docs/audits/`, including today's ECS, safety and renderer reports.
  - All 11 findings from the 09-11 baseline are closed (#4177–#4188). I verified each fix at the sites
    this run touched (see the per-dimension notes).

## Summary

| Severity | NEW | Regression | Matched to existing open issues |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 3 | 0 | 2 (#3429, #3659) |
| LOW | 2 | 0 | 0 |

| ID | Sev | Dim | Title |
|---|---|---|---|
| CONC-D1-2026-09-21-01 | MEDIUM | 1 | The all-slots fence wait is unpinned as to its argument; two more of its riders are missing from the #870/#3643 list |
| CONC-D2-2026-09-21-01 | MEDIUM | 2 | Ground-cover counter readback (and three older readbacks) have no device→host memory dependency; the project's newer readbacks all emit one |
| CONC-D3-2026-09-21-01 | MEDIUM | 3 | The `lock-order-check` CI lane has been red on every sampled main run since ≥09-14, so two real lock-order cycles it caught went unnoticed |
| CONC-D1-2026-09-21-02 | LOW | 1 | `draw_frame_guards_on_empty_framebuffers_before_acquire` is vacuous: its needles match only the test's own literals |
| CONC-D3-2026-09-21-02 | LOW | 3 | The new P2 combat systems re-open two closed hold-stack patterns (#3444 shadowed guard; #3473 hold across a helper) |

**Headline.** The code-level concurrency picture is stable:
- no new cycle among the exercised paths;
- the scheduler proof is live;
- teardown is complete for every new GPU owner;
- no new production worker threads.

The most consequential problem is in CI, not code:
- The ABBA detector's hand-built-world CI lane (`lock-order-check`) has been continuously red, alternating
  between real cycles and the adapter-dependent `crates/ui` tests.
- A FormIdPool↔FormIdComponent cycle (never filed) and #4546 both sat on main behind a lane that was
  already red.
- Together with SAFE-D5-2026-09-21-01 (the live-world `vulkan-validation` lane never reaches Vulkan), no
  CI lane can currently signal a new lock-order cycle.

---

## Findings

### CONC-D1-2026-09-21-01: The all-slots fence wait is unpinned as to its argument, and two more of its riders are missing from the #870/#3643 list
- **Severity**: MEDIUM (same class and grade as #3643 and #4516)
- **Dimension**: Vulkan Queue & AS Sync (frame-in-flight discipline)
- **Location**:
  - The wait: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:65`.
  - The rider list: `crates/renderer/src/vulkan/sync.rs:23-99`.
  - Rider 1: `crates/renderer/src/vulkan/context/draw.rs:2383-2427` → `crates/renderer/src/vulkan/acceleration/memory.rs:407-409`.
  - Rider 2: `crates/renderer/src/vulkan/groundcover.rs:1198-1300`, called via `byroredux/src/app_frame.rs:369`, before `draw_frame` at `:516`.
- **Status**: NEW
- **Description**:
  - `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` is the only safety argument for every
    resource listed in `sync.rs` (7 entries). No test pins that the wait covers the *whole* array.
    - The #3442 pin (`temporal_history_indexing_uses_the_general_previous_slot_form`,
      `shader_constants.rs:1127`) rejects only a `(f + 1) % MAX_FRAMES_IN_FLIGHT` spelling.
    - The tests that scan `sync_and_acquire_frame.rs` check only *position*: `morph_compute.rs:331`,
      `depth_capture.rs:417`, and the caustic, skin, instance and image-health scans.
    - `helpers.rs:29` nonetheless says "That wait is itself pinned since `ac48ab63` (#3442)".
    - The textbook per-slot form, `&[self.frame_sync.in_flight[frame]]`, therefore passes every test.
      It is a plausible perf change: today the all-slots wait means CPU recording of frame N never
      overlaps GPU execution of frame N-1.
  - Two riders on that wait are not in the list:
    1. **Post-present shrink of the *next* slot** (`draw.rs:2403-2426`). `shrink_tlas_scratch_to_fit`
       destroys that slot's TLAS scratch immediately (`memory.rs:408`). Its SAFETY text (`draw.rs:2386,
       2397`) credits "the standard MAX_FRAMES_IN_FLIGHT alternation" and says the slot is "the one whose
       previous frame work signalled at the start of this frame". That is true only because the
       start-of-frame wait covered both slots. Under a per-slot wait, that slot's last user (frame N-1)
       can still be executing its TLAS build.
    2. **Ground-cover `prepare`.** It runs from the app before `draw_frame`, on `current_frame`
       (`resources.rs:713-727`). It harvests `counter_readback[frame]`, writes six host-visible per-slot
       buffers, and rewrites that slot's descriptor sets. Its documented contract "after slot `frame`'s
       fence has been waited" (`groundcover.rs:1198`) holds only through the *previous* `draw_frame`'s
       all-slots wait.
  - Neither rider is a MAX_FRAMES_IN_FLIGHT-bump hazard: both hold at any N while the wait stays
    all-slots. They are wait-policy hazards, which the const-assert and the tripwire test cannot see.
- **Evidence**: `grep 'in_flight, true'` returns only the production line. None of the seven
  `include_str!("…sync_and_acquire_frame.rs")` scans asserts on the wait's argument.
- **Impact**: None today. A one-line revert to the per-slot wait stays green and silently breaks nine
  sites (7 listed + these 2): immediate BLAS/TLAS scratch destroys under an in-flight build (UAF), host
  writes into in-use mapped buffers, and `VUID-vkUpdateDescriptorSets-None-03047` on in-use sets.
- **Trigger Conditions**: Any change of the top-of-frame wait to cover fewer than all `in_flight`
  fences, followed by a frame in which the next slot's TLAS scratch shrinks or ground cover is active.
- **Verification Path**: `cargo test` for the pin. The consequences would show as
  `BYRO_VALIDATION=1` UAF / descriptor-in-use VUIDs only after such a revert.
- **Related**: #870, #3643 (MEDIUM), #3442, #4516 (MEDIUM), #2929; CONC-D2-2026-09-21-01 (the
  readback `prepare` harvests).
- **Suggested Fix**:
  - Add a source-scan pin that the top-of-frame `wait_for_fences` passes the whole `in_flight` slice.
  - Add both riders to the `sync.rs` block and to `frames_in_flight_contract_names_every_dependent_resource`'s table.
  - Reword `draw.rs:2383-2399` to cite the all-slots wait rather than "standard alternation".

### CONC-D2-2026-09-21-01: The ground-cover counter readback — and three older readbacks — have no device→host memory dependency; the project's newer readbacks all emit one
- **Severity**: MEDIUM. The spec floor is HIGH. I rated it MEDIUM per the #4181/#4293 precedent on this
  same buffer: the consequence is diagnostic/telemetry only, and nothing is observable on the dev config.
- **Dimension**: Compute → AS → Fragment Chains (ground-cover scatter → publish incl. counter readback)
- **Location**:
  - `crates/renderer/src/vulkan/groundcover.rs:1640-1661`: the publish barrier, then `cmd_copy_buffer`
    into `counter_readback[frame]`, with no `→ HOST` edge.
  - `groundcover.rs:1352-1372`: the harvest, which invalidates and reads after the slot's fence.
  - Siblings:
    - `context/screenshot.rs:229-260`: copy, then restore to `PRESENT_SRC`.
    - `context/depth_capture.rs:245-262`: copy, then restore.
    - `shaders/presentation.frag:195-198`: the image-health `atomicAdd`s. The presentation pass is
      recorded *after* the only FRAGMENT→HOST barrier, `context/draw.rs:1984-1994`.
- **Status**: NEW for the ground-cover site. The image-health and screenshot siblings are the code half
  that closed #2740 left undone: 51c725351 fixed documentation only, deferring the barrier to a
  `BYRO_VALIDATION=1` run.
- **Description**:
  - The Vulkan spec's fence-waiting note says a fence's memory dependency covers only device access.
    Making device writes available to the host needs a memory dependency with `HOST_READ` in its
    destination access scope; that is the device→host domain operation. The host-side
    `invalidate_if_needed` (#2752) is the other half.
  - The newer readbacks follow the rule:
    - cluster telemetry, `compute.rs:362-371`;
    - combustion surface-light readback, `volumetrics.rs:1373-1382`. Its comment reads "the barrier
      supplies the device->host memory dependency that the fence's device-only access scope does not
      create by itself";
    - ray-probe record, `draw.rs:1984-1994`;
    - the sky-filter test, `sky_cube/filter/tests.rs`: `TRANSFER_WRITE → HOST_READ` before its fence wait.
  - The four readbacks above do not.
- **Evidence**:
  - `grep HOST_READ crates/renderer/src` finds exactly three production sites.
  - No `PipelineStageFlags::HOST` destination follows any `TRANSFER` copy.
- **Impact**: Stale reads are possible on a GpuToCpu memory type that is `HOST_CACHED` and not coherent,
  or on a driver that does not flush at submit end. Affected data:
  - `GroundCoverStats`: blade counts, the §11.3 density histogram and extrema. EXAL tuning reads these
    directly.
  - The image-health smoke gate.
  - Golden-frame screenshots.
  - Depth captures.
- **Trigger Conditions**: Every frame with ground cover active, and every image-health / screenshot /
  depth-capture readback, on such a memory type.
- **Verification Path**: No validation-layer signal exists for this class: syncval does not model host
  reads of mapped memory, which is why #2740's "run BYRO_VALIDATION first" condition could never be met.
  The evidence is the spec text plus the in-tree precedent. Only a non-coherent readback device can show
  it empirically.
- **Related**: #2740 (closed, docs-only), #2752, #4181, #4293; CONC-D1-2026-09-21-01.
- **Suggested Fix**:
  - Add one global `memory_barrier(TRANSFER | FRAGMENT_SHADER, TRANSFER_WRITE | SHADER_WRITE → HOST, HOST_READ)`
    as the last command before `end_command_buffer`. That covers all four. Alternatively, add per-site
    edges that mirror `volumetrics.rs:1373-1382`. The change is purely additive.
  - Add a pin that every `create_host_readback` consumer's writer is followed by a `HOST_READ` edge.

### CONC-D3-2026-09-21-01: The `lock-order-check` CI lane has been red on every sampled main run since at least 09-14, so two real lock-order cycles it caught went unnoticed
- **Severity**: MEDIUM. This is a defence-in-depth gap on the HIGH-floor ECS-deadlock class, graded
  like SAFE-D4/D5-2026-09-21-01.
- **Dimension**: ECS Lock Ordering & Deadlock
- **Location**: `.github/workflows/ci.yml:186-198`: job `lock-order-check`, `cargo test --workspace`
  under `BYRO_LOCK_ORDER_CHECK: 1`, without `--no-fail-fast`.
- **Status**: NEW. SAFE-D5-2026-09-21-01 covers the other lane (`vulkan-validation`); this is the lane
  the skill's Dim 3 first step names.
- **Description**: Reconstructed from `gh run view` on main:

  | Run(s) | Commit, date | Why the lane was red |
  |---|---|---|
  | 34858811547, 34997424662, 35102663503 | 8d08da793 09-14, fd0cd577c 09-15, 5e7781197 09-16 | A **real cycle**. 29 bin tests panic at `lock_tracker.rs:476` with "`FormIdPool` while holding `FormIdComponent` … closes a cycle `FormIdPool → FormIdComponent → FormIdPool`". It was never filed (no FormIdPool/FormIdComponent cycle issue after #3266) and was fixed silently by a 09-16 feature commit. |
  | 35223046709, 35357015643 | 8c834e0be 09-17, e6c6405a8 09-18 14:33 | The 8 adapter-dependent `crates/ui` Ruffle tests ("Ruffle requires hardware acceleration"). Not a lock failure. |
  | 35389142963 | 913fd39d8 09-18 20:01, the commit that added `walk_anim` | The **#4546 cycle** (`ActorCinematicState → Transform → AnimationPlayer → ActorCinematicState`, 5 walk_anim tests). The lane caught it the day it landed, but the color stayed red→red. #4546 was filed from a manual run on 09-20 and fixed on 09-21 (c1f38e3da). |
  | 35658431384 | HEAD | The same 8 `crates/ui` tests, after every lock-relevant binary passed (bin 2309, core 768, physics 175, renderer 1139, …). |

  In short, the lane was red on real cycles on 09-14→09-16 and 09-18→09-21, and red on the Ruffle
  tests on 09-17→09-18 and at HEAD.
- **Evidence**:
  - Job logs saved to `/tmp/audit/concurrency/lo_*.log` and `lockorder_job_head.log`.
  - Without `--no-fail-fast`, cargo stops at the first failing test binary. On 09-18 the bin failure
    prevented every later binary (core, physics, scripting, save…) and all doctests from running under
    the detector.
- **Impact**: The lane no longer changes color when a new cycle lands, so it cannot signal one. Both
  known escapes were latent ABBA risks between PostUpdate systems, which #4546's own issue notes would
  deadlock under a schedule change. With the `vulkan-validation` lane also inert (SAFE-D5), CI currently
  gives no lock-order signal at all.
- **Trigger Conditions**: Any commit introducing a new lock-order cycle in a path the test suite exercises.
- **Related**: SAFE-D4-2026-09-21-01 (same `cargo test --workspace` failure, clippy half), SAFE-D5-2026-09-21-01, #4546, #3266, #1410, #2137.
- **Suggested Fix**:
  - Make the lane able to go green on its own: gate the Ruffle adapter tests, or `--exclude byroredux-ui`
    in this job.
  - Add `--no-fail-fast`.
  - Add a second gate that fails the job iff the output contains `lock-order cycle`.
  - File the FormIdPool↔FormIdComponent episode for the record.

### CONC-D1-2026-09-21-02: `draw_frame_guards_on_empty_framebuffers_before_acquire` is vacuous
- **Severity**: LOW (test gap; the property holds today)
- **Dimension**: Vulkan Queue & AS Sync (acquire discipline, `image_available` signal-pending)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:3020-3056`
- **Status**: NEW
- **Description**:
  - The test asserts that the empty-framebuffers guard precedes `.wait_for_fences(` and
    `.acquire_next_image(` in `draw.rs`. Both calls moved to `sync_and_acquire_frame.rs` (`:65`, `:150`)
    in 7463204eb (#3282, 09-02). Since then both `find`s match the test's own literals (`draw.rs:3036`,
    `:3039`).
  - As a result `guard_pos < wait_pos` always holds. Even deleting the production guard passes: the
    guard needle then matches the test literal at `:3025`, which is still before `:3036`.
  - #3991 repaired the identical self-match in the sibling
    `skin_dispatch_ran_is_reset_before_both_early_return_guards` (its comment says so) and missed this one.
- **Evidence**: `grep -n '\.wait_for_fences(\|\.acquire_next_image(' context/draw.rs` returns only lines
  3036 and 3039.
- **Impact**: The #1211 contract has no live pin: skip the frame before acquiring, or leave
  `image_available[frame]` signal-pending → `VUID-vkAcquireNextImageKHR-semaphore-01779`. The property
  itself is true today (guard at `draw.rs:1797`, before the `sync_and_acquire_frame` call at `:1812`).
- **Related**: #1211, #3282, #3991, #3442 (the compose-needles-at-runtime technique).
- **Suggested Fix**: Anchor on `self.sync_and_acquire_frame(&mut t)` in `draw.rs`, assert that the wait
  and the acquire live in `sync_and_acquire_frame.rs`, and compose the needles at runtime.

### CONC-D3-2026-09-21-02: The new P2 combat systems re-open two closed hold-stack patterns
- **Severity**: LOW (latent; no reverse edge exists; both systems are exclusive)
- **Dimension**: ECS Lock Ordering & Deadlock (guard lifetime in system bodies)
- **Location**: `byroredux/src/systems/combat_anim.rs:104-107` (with `:329-417`); `byroredux/src/systems/combat_ai.rs:61-156` → `byroredux/src/combat.rs:369-465`
- **Status**: NEW (recurrences of closed #3444 and #3473)
- **Description**:
  1. **`combat_feedback_system_inner`** (ec3a18d2f, 09-21):
     - `let Some(clips) = world.try_resource::<DraugrCombatClips>() … ; let clips = *clips;` shadows the
       guard without dropping it. This is the exact #3444 defect.
     - The read guard therefore stays live for the whole function:
       - the read pass: HitEvent, CombatState, PlayerEntity, GlobalTransform, PlayerMode,
         DraugrCombatAnim, Dead, AnimationPlayer, AnimationTarget;
       - the write pass: AnimationPlayer and DraugrCombatAnim writes;
       - the sound pass, commented "after every component write, locks dropped": SoundArchiveProvider
         (BSA extract + decode), then an `AudioWorld` write.
  2. **`npc_combat_ai_system`** (f61ea0447, 09-13):
     - The read pass holds `AiCombatState` + `Transform` read guards while calling `attack_damage`, which
       takes the chain `MeleeDamageConfig → CharacterRuleset → ActorValues → CharacterLevel` beneath them
       (plus EquippedWeapon/CreatureAttack/Dead/WalkSpeed).
     - That is the five-deep hold stack across a helper that `attack_damage`'s own #3473 comment says the
       #2270 house rule prohibits.
     - The new `Transform → CharacterRuleset/ActorValues` edges are not in `docs/engine/ecs.md`. The
       combat_ai tests have no weapon or ruleset, so the detector never records them.
- **Evidence**: No cycle exists today.
  - `DraugrCombatClips`' only other readers take `&mut World`.
  - `CharacterRuleset → ActorValues → CharacterLevel` is consistent at `core/src/character/regen.rs:213-226`
    and `scripting/src/condition.rs:676-682`.
  - Every AI-walker `WalkSpeed`/`Dead` read happens under a live `Transform` read.
- **Impact**:
  - Both sites add spurious or undocumented edges to the lock graph.
  - The combat_feedback comment is false.
  - A future `Transform`-after-CHARAL site, or promoting either system to a parallel lane, would close a
    cycle with no test signal.
- **Related**: #3444, #3473, #2270, #4325 (closed; the PhysicsWorld half of combat_ai), ECS-2026-09-21-D5-02 (combat_ai's undeclared `WalkSpeed`).
- **Suggested Fix**:
  - Bind the clips copy in a scoped block (`let clips = { *world.try_resource::<…>()? };`).
  - Compute per-attacker reach, damage and cooldown after the combat/transform guards drop (a second
    pass over the collected entities).
  - Otherwise, document `Transform → CharacterRuleset` in `ecs.md`.

---

## Existing issues re-confirmed (not re-filed)

- **#3429 (OPEN, MEDIUM)**, Dim 1. `update_rgba` reallocates a full-viewport VkImage and fence-waits per
  Fresh frame, ahead of `draw_frame`. The trigger has widened: the FO4 (62fc0bf22) and Skyrim (M48.6)
  Scaleform `--hud` routes now drive an always-on HUD through `tick_ui_overlay` → `update_rgba`
  (`byroredux/src/app_frame.rs:1033-1050`). The MenuXml HUD was moved to the in-place rotation for
  exactly this reason (`hud.rs:279-288`).
- **#3659 (OPEN, MEDIUM)**, Dim 7. `Ba2Archive::extract` still holds `Mutex<File>` (`crates/bsa/src/ba2.rs:408-418`)
  across `extract_general` / `extract_dx10` decompression. Unchanged.

## Cited from sibling legs (concurrency-relevant; not re-reported)

- **SAFE-D5-2026-09-21-01**: the `vulkan-validation` lane never reaches Vulkan (missing
  `libxkbcommon-x11`; lavapipe rejected since 09-17). That leaves the lock-order detector's only
  live-world CI run inert. CONC-D3-2026-09-21-01 is the distinct other-lane consequence.
- **SAFE-D4-2026-09-21-01**: `cargo test --workspace` fails on the 8 `crates/ui` adapter tests, so
  clippy is skipped. It is the same root cause as the lock-order lane's standing red.
- **SAFE-D3-2026-09-21-01**: a poisoned allocator lock can double-panic during teardown
  (`teardown.rs:432` and the Drop-side `expect`s).
- **SAFE-D2-2026-09-21-01**: three staging-pool release sites record `allocation.size()` (#4512 class).
- **SAFE-D1-2026-09-21-01**: the FSR dispatch-failure recovery blit declares `scene_color`
  `oldLayout = GENERAL` (#3572 argument-insertion slip).
- **ECS-2026-09-21-D5-01**: the declaration-completeness guard ignores read vs write and substring-matches
  type names. Confirmed at `boot/schedule/mod.rs:616-640, 702`.
- **ECS-2026-09-21-D5-02**: five exclusives under-declare; `ragdoll_writeback_system` is registered with
  plain `add_exclusive` (`late.rs:121`).
- **REN-D11-2026-09-21-02**: the auto-exposure adaptation reads the per-FIF slot written two frames
  earlier. This is a cross-frame indexing issue in the new meter; its barriers trace clean (renderer Dim 4).

---

## Per-dimension results

### Dim 1 — Vulkan Queue & AS Sync (36 commits): 1 MEDIUM, 1 LOW (above)
- **Queue Mutex.** There are still exactly three production queue sites: the frame submit
  (`draw.rs:2186-2224`, guard bound and dropped on both arms), present (`:2318-2334`), and the one-time
  submit (`texture.rs:966-972`, guard scoped to the submit; the fence wait happens outside it, pinned by #1713).
- **FIF tripwires.** The const-assert (`sync.rs:88`) and the tripwire test (`sync.rs:599`, now naming the
  HUD `texture_handles`) are both live and not ignored.
- **HUD rotation.** It is safe: it rotates per upload, at most one upload per frame, before `draw_frame`.
- **AS barriers.**
  - #4177's hoisted barrier is at `blas_static.rs:743-745`.
  - #4179's frame-scope AS_WRITE→AS_READ edge precedes `build_tlas` (`dispatch_skin_and_cluster.rs:294-307`).
  - The post-TLAS publish, dst FRAGMENT|COMPUTE, runs on both arms (`:393-400`).
  - Every ray-query shader runs in the FRAGMENT or COMPUTE stage. This includes the new ground-cover
    scatter (c775745b5), which runs after that publish.
- **Deferred destruction.** Every eviction goes through `pending_destroy_blas` / `pending_destroy_scratch`.
  The remaining immediate destroys are never-submitted or post-one-time-fence objects, or sit behind
  `device_wait_idle`.
- **Swapchain recreate** idles the device first. #4180's static-BLAS recovery now runs between frames
  (`app_step.rs:343`).

### Dim 2 — Compute → AS → Fragment Chains (66 commits): 1 MEDIUM (above)
- **Sky-cube bake / prefilter / SH irradiance.** Correct layout pairs on both mip ranges. Every consumer
  of mips ≥1 and of binding 21 is a fragment shader. The bake is unconditional per frame and runs before
  the geometry pass.
- **Ground-cover barriers.** #4293 (TRANSFER→TRANSFER seed ordering), #4181 (TRANSFER in the publish dst)
  and the interaction-field edges are all intact.
- **Barrier-helper refactor 13e00a24b.** Mask- and range-identical to the originals (diffed).
- **Skin chain.** #4204's narrowed palette dispatch is per-FIF and re-arms bind-inverse slots for both
  frame slots. The publish mask still carries COMPUTE, and its consumer test discovers every `.comp`.
- **Volumetrics.** The `tlas_written` latch is symmetric. History pairs use `(frame + MAX - 1) % MAX`.
  Neutral clears go through `clear_general_accumulator`.
- **TAA re-tap (#3572).** The blit's source barrier covers the COMPUTE write and restores `GENERAL`.
- **Unchanged since baseline.** Bloom, caustic, SVGF, TAA, water-caustic and material have zero
  barrier-line changes.

### Dim 3 — ECS Lock Ordering (38 commits): 1 MEDIUM, 1 LOW (above)
- **First step.** Satisfied by HEAD CI (every lock-relevant binary green under the detector) plus the
  ECS leg's local runs.
- **#4546.** Walk_anim and every cinematic system take `Transform` before the cinematic state (`ecs.md`
  records the rule).
- **#4325.** PhysicsWorld is taken only between combat_ai's storage passes.
- **#4183.** Submersion's wave snapshot is hoisted above the water storage guards.
- **`restoration_system`.** It drops its read guard before the TypeId-sorted pair.
- **`StartCombat`.** It adds `QuestStage/ObjectiveState → AiCombatState`. Every `condition::evaluate`
  caller runs guard-free, so no path leads back into the quest resources.
- **Extensions.** Unchanged since baseline.

### Dim 4 — Scheduler Proof Soundness (18 commits): 0 NEW (cite ECS D5-01/D5-02)
- **The proof is live.**
  - The three release `assert_eq!`s are in `registries.rs:27-50`.
  - The floors (≥9 systems, ≥7 pairs) are at `scheduler_access_tests.rs:292/302`.
  - `add_to_with_access(` = 9 = `PARALLEL_SYSTEMS`. No `#[ignore]`.
- **physics_sync's `water.rs` hop.** Guarded by `water_and_animation_parallel_accesses_are_complete`,
  which #3964 extended.
- **Fly-camera `GlobalTransform` publish.** It is declared, and its Early writer precedes its Update readers.
- **Cross-stage pins.** All present: #4185 footstep (`:714`), #4186 WindField lag (`early.rs:80-83`),
  billboard, submersion.
- **The 18 resources added since the baseline.** None shows a writer-after-reader-stage inversion. Their
  runtime writers are console commands, `&mut World` load paths, or same-stage exclusives.

### Dim 5 — RwLock Patterns / Physics (12 commits): 0 NEW
- **Collider census.** It drops its four storages before `FormIdPool` (`sync.rs:741-746`).
- **#3974 player current arm.** It nests `WaterCurrentVolume` under the water storages. The dynamic path
  collects surfaces and currents in disjoint scopes, so no reverse edge exists.
- **`activate_ragdoll`.** It collects, then drops, then takes a scoped PhysicsWorld write.
- **KCC-backed walkers and combat_ai.** PhysicsWorld remains a sink in every one.
  `character_controller_system` drops its `pw` before `set_kinematic_translation`.
- **#1520 release.** It still runs before `despawn_batch`.
- **Tests.** The physics suite passes under the detector at HEAD (175/175).

### Dim 6 — Resource Lifecycle (48 commits): 0 NEW (cite SAFE-D2/D3; #3429)
- **Load-bearing orderings.** Exactly three, pinned (#4527). Placeholders are demoted (#4188).
- **#4187 fix** in place: `StagingPool` now holds `Option<SharedAllocator>` and `destroy()` calls `take()`.
- **New GPU owners.** Ground cover, the bench, the sky cube with its filter and irradiance, the exposure
  meter plus resource, cloud noise, and #4199's retired instance buffers are all released before
  `Arc::try_unwrap`.
- **#4307 pipeline rebuild.** Ground-cover pipelines are rebuilt after the idle. A failed rebuild leaves
  them null, and `record_draw` skips on a null pipeline.
- **Per-frame paths.** No per-frame allocation that persists was found.

### Dim 7 — Worker Threads (4 commits): 0 NEW (#3659 re-confirmed)
- **Thread census.** No new production worker thread. Every other hit is in the known set or inside
  `#[cfg(test)]`.
- **#4207 memo.** It is worker-local.
- **Main-thread-only changes.** `SwfPlayer::set_stage_transparent` and `AudioWorld::headless()` run on
  the main thread and hold no ECS guard across a kira call.
- **Allocator locks.** New owners lock only inside allocate/free. The in-place HUD upload holds a
  `StagingGuard` (an `Arc` clone), not a lock, across its submit and fence wait.

## Process notes
- **Scratch notes.** `/tmp/audit/concurrency/dim_{1..7}.md`. CI logs are in
  `/tmp/audit/concurrency/lo_*.log`. The helper script is `/tmp/audit/concurrency/acq_order.py`.
- **Repository.** Nothing was written except this report.
- **Guardrail.** Per the speculative-fix guardrail, none of the Vulkan-side findings asks for an
  unverified barrier change as a confirmed bug:
  - CONC-D2-2026-09-21-01 rests on spec text plus in-tree precedent, and states that no layer signal exists.
  - CONC-D1-2026-09-21-01 concerns a pin and a list, not a barrier.

Next step: `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-09-21.md`. Domain labels:
- `sync`: CONC-D1-01, CONC-D1-02, CONC-D2-01. Add `test-gap` to D1-01 and D1-02.
- `concurrency`: CONC-D3-01, CONC-D3-02. Add `tech-debt` to D3-01 for its CI half.
