# 4600: SAFE-D4-2026-09-21-02: Three SAFETY / `# Safety` texts restate a pre-refactor invariant

State: OPEN  Labels: ['documentation', 'renderer', 'low', 'safety', 'doc-rot']

**Severity**: LOW · **Dimension**: 4 — Unsafe-block discipline (truth of stated invariants)
**Location**:
- `crates/renderer/src/vulkan/frame_upscaler.rs`: `record_native_blit`'s `# Safety` doc (~:691-700) and its first inner `// SAFETY:` (~:746-748)
- `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`: the skin-dispatch `// SAFETY:` (~:500)
- `crates/renderer/src/vulkan/scene_buffer/upload.rs`: `upload_terrain_tiles`' staging `// SAFETY:` (~:1049-1052)

**Status**: NEW
**Verified against**: HEAD `f97775ca8`

## Description

- **`record_native_blit`.** Its `# Safety` says "`scene_color` must be in `SHADER_READ_ONLY_OPTIMAL` (composition's output layout)", and the inner SAFETY says "scene composition left `scene_color` shader-readable". Since #3572 the function takes `source_layout`, and the TAA path legitimately passes `GENERAL`. This stale contract is part of why SAFE-D1-2026-09-21-01 (#4592)'s wrong argument reads as plausible.
- **Skin dispatch.** The SAFETY says "Each `dispatch` binds the compute pipeline + slot set at the COMPUTE bind point". Since #4205 the pipeline is bound once per batch through `SkinComputePipeline::bind`, and `slot_dispatch_does_not_rebind_the_pipeline_per_entity` (`skin_compute.rs`) pins that `dispatch` no longer binds it.
- **Terrain staging.** The SAFETY says "GpuTerrainTile is #[repr(C)] with u32-only fields matching std430". Since #4057 the struct also has `f32` lanes: `cover_affinity0` / `cover_affinity1: [f32; 4]` and `cell_origin_xz: [f32; 2]`.

## Evidence

The quoted texts at the locations above, compared with the code they describe:
- `record_native_blit(…, source_layout, output_layout)`;
- `SkinComputePipeline::bind` / `dispatch`;
- `GpuTerrainTile` in `scene_buffer/gpu_types.rs`.

## Impact

- At the second and third sites this is documentation only: the property the code actually relies on still holds. One pipeline bind precedes the dispatches, and `GpuTerrainTile` is a `#[repr(C)]` POD whose size `gpu_terrain_tile_is_160_bytes` pins.
- At the first site the stale text hides a live bug (SAFE-D1-2026-09-21-01 (#4592)).

## Related

- SAFE-D1-2026-09-21-01 (#4592)
- #3572, #4205 and #4057 (closed): the changes these texts did not follow.
- #3597 and #3583 (closed): earlier instances of this class.

## Suggested Fix

Restate each text against the current code:
- **Blit:** the `source_layout` / `output_layout` contract.
- **Skin loop:** one lazy pipeline bind per batch, plus a per-slot descriptor bind.
- **Terrain copy:** a `#[repr(C)]` POD with `u32` and `f32` lanes.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D4-2026-09-21-02)

## Completeness Checks
- [ ] **UNSAFE**: each restated SAFETY names the invariant the block relies on today
- [ ] **SIBLING**: the `// SAFETY:` comments at `record_native_blit`'s three call sites re-read against the restated contract


---

# 4601: CONC-D1-2026-09-21-01: The all-slots fence wait is unpinned as to its argument, and two more of its riders are missing from the #870/#3643 list

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'vulkan', 'sync', 'test-gap']

**Severity**: MEDIUM (same class and grade as #3643 and #4516) · **Dimension**: 1 — Vulkan Queue & AS Sync (frame-in-flight discipline)
**Location**:
- The wait: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:65` — `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` in `sync_and_acquire_frame`
- The rider list: `crates/renderer/src/vulkan/sync.rs:23-99` (the #870/#3643 block), and its tripwire test `frames_in_flight_contract_names_every_dependent_resource` (`sync.rs:599`)
- Rider 1: `crates/renderer/src/vulkan/context/draw.rs:2383-2427` (post-present TLAS shrink of the *next* slot) → `crates/renderer/src/vulkan/acceleration/memory.rs:331,408` (`shrink_tlas_scratch_to_fit`'s immediate `old.destroy`)
- Rider 2: `crates/renderer/src/vulkan/groundcover.rs:1198-1300` (`GroundCoverPipeline::prepare`), reached via `VulkanContext::prepare_groundcover` (`context/resources.rs:713-727`) from `byroredux/src/app_frame.rs:369` (→ `:823`), before `draw_frame` at `:516`

**Status**: NEW
**Verified against**: HEAD `f97775ca8` (read at the symbols; line numbers as of HEAD).

## Description

`sync_and_acquire_frame`'s top-of-frame `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` is the only safety argument for every resource in the `sync.rs` #870/#3643 block (7 entries). Nothing pins that the wait covers the *whole* array:

- The #3442 pin, `temporal_history_indexing_uses_the_general_previous_slot_form` (`crates/renderer/src/shader_constants.rs:1127`), rejects only the `(f + 1) % MAX_FRAMES_IN_FLIGHT` spelling.
- The seven tests that `include_str!` `sync_and_acquire_frame.rs` (in `caustic.rs`, `skin_compute.rs`, `context/depth_capture.rs`, `shader_constants.rs`, `morph_compute.rs`, `context/resources.rs`, `context/build_and_upload_instances.rs`) check only presence, or the *position* of `.wait_for_fences(` relative to a later call. None asserts on the wait's argument.
- `context/helpers.rs:28-30` nonetheless says "That wait is itself pinned since `ac48ab63` (#3442)".
- So the textbook per-slot form, `&[self.frame_sync.in_flight[frame]]`, passes every test. It is also a plausible perf change: the all-slots wait means CPU recording of frame N never overlaps GPU execution of frame N-1. The throughput side is PERF-D5-2026-09-21-01 (`docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`).

Two riders on that wait are not in the list:

1. **Post-present shrink of the *next* slot.** After `self.current_frame` advances (`draw.rs:2341`), `draw_frame` calls `shrink_tlas_to_fit` and then `shrink_tlas_scratch_to_fit` on the new `current_frame` slot (`draw.rs:2409`, `:2426`). `shrink_tlas_scratch_to_fit` destroys that slot's TLAS scratch immediately: `memory.rs:331` on the slot-`None` arm, and `:408` on the live arm once the replacement is allocated. The SAFETY text at `draw.rs:2383-2399` credits "the standard MAX_FRAMES_IN_FLIGHT alternation" and says the slot is "the one whose previous frame work signalled at the start of this frame". That is true only because the start-of-frame wait covered *both* slots. At N = 2 the next slot's last user is frame N-1, which a per-slot wait on frame N's own slot never waits for, so frame N-1's TLAS build can still be executing when the scratch is freed.
2. **Ground-cover `prepare`.** It runs from the app before `draw_frame`, on `self.current_frame` (`resources.rs:713-727`). It harvests `counter_readback[frame]`, writes six host-visible per-slot buffers (chunk, cell, species, species-table, field-state, disturber), and rewrites that slot's descriptor sets. Its documented contract, "Must be called after slot `frame`'s fence has been waited" (`groundcover.rs:1198`), holds only through the *previous* `draw_frame`'s all-slots wait. The upcoming frame's own wait has not run yet.

Neither rider is a MAX_FRAMES_IN_FLIGHT-bump hazard: both hold at any N while the wait stays all-slots. They are wait-*policy* hazards, which the `== 2` const-assert and the tripwire test cannot see.

## Evidence

- `grep -rn 'in_flight, true' crates/ byroredux/` returns only the production line (`sync_and_acquire_frame.rs:65`).
- The `sync.rs` block names seven riders: `blas_scratch_buffer`, `depth_capture_staging`, `terrain_tile_buffer`, `screenshot_staging` + `depth_capture_pending_readback`, `weight_buffer`, the HUD `texture_handles`, and `FrameSync::images_in_flight`. Neither the TLAS scratch/instance buffers nor any ground-cover buffer appears in that prose or in `frames_in_flight_contract_names_every_dependent_resource`'s table.

## Impact

None today. A one-line change to a per-slot wait stays green and silently breaks nine sites (the 7 listed plus these 2):
- immediate BLAS/TLAS scratch destroys under an in-flight build (use-after-free);
- host writes into in-use mapped buffers;
- `VUID-vkUpdateDescriptorSets-None-03047` on in-use descriptor sets.

**Trigger conditions**: any change of the top-of-frame wait to cover fewer than all `in_flight` fences, followed by a frame in which the next slot's TLAS scratch shrinks or ground cover is active.

**Verification path**: `cargo test` for the pin. The consequences would show only after such a change, as `BYRO_VALIDATION=1` use-after-free / descriptor-in-use VUIDs.

## Related

- #870 and #3643 (the rider list this extends), #3442 (made the wait all-slots), #4516 (the last rider added), #2929 (the TLAS shrink's deferred-destroy history).
- CONC-D2-2026-09-21-01 (#4602): the ground-cover readback that `prepare` harvests.
- PERF-D5-2026-09-21-01 (`docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`): the throughput cost of this same wait. A move to a per-slot wait has to land everything below first.

## Suggested Fix

- Add a source-scan pin that the top-of-frame `wait_for_fences` passes the whole `in_flight` slice. Compose the needle at runtime so it cannot match the test's own literal (the #3442 technique).
- Add both riders to the `sync.rs` #870/#3643 block, to `frames_in_flight_contract_names_every_dependent_resource`'s table, and to the const-assert message's list.
- Reword `draw.rs:2383-2399` to cite the all-slots wait rather than "standard alternation". Once the pin exists, `helpers.rs:28-30`'s "pinned since `ac48ab63`" becomes true.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every other site that writes, rewrites or destroys a non-per-FIF resource outside `draw_frame`'s own wait (pre-draw app hooks, post-present tails) checked against the list
- [ ] **TESTS**: the new pin fails when the wait's argument is changed to `&[self.frame_sync.in_flight[frame]]` (mutation-check, then revert)


---

# 4602: CONC-D2-2026-09-21-01: The ground-cover counter readback — and three older readbacks — have no device→host memory dependency; the project's newer readbacks all emit one

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'vulkan', 'sync', 'terrain-exterior']

**Severity**: MEDIUM. The spec-violation floor is HIGH. It is graded MEDIUM per the #4181 / #4293 precedent on this same buffer: the consequence is diagnostic/telemetry only, and nothing is observable on the dev config. · **Dimension**: 2 — Compute → AS → Fragment Chains (ground-cover scatter → publish, including the counter readback)
**Location**:
- `crates/renderer/src/vulkan/groundcover.rs:1640-1661` (`record_scatter`): the publish barrier, then `cmd_copy_buffer` into `counter_readback[frame]`, with no `→ HOST` edge after it.
- `groundcover.rs:1352-1372` (`harvest`): invalidates and reads after the slot's fence.
- Siblings with the same gap:
  - `crates/renderer/src/vulkan/context/screenshot.rs:229-260` (`screenshot_record_copy`): `cmd_copy_image_to_buffer`, then a restore to `PRESENT_SRC_KHR` whose dst stage is `NONE`.
  - `crates/renderer/src/vulkan/context/depth_capture.rs:245-262` (`depth_capture_record_copy`): copy, then a restore whose dst is `EARLY_FRAGMENT_TESTS | FRAGMENT_SHADER | COMPUTE_SHADER`.
  - `crates/renderer/shaders/presentation.frag:195-198`: the image-health `atomicAdd`s. The presentation pass is recorded *after* the only FRAGMENT→HOST barrier (`context/draw.rs:1984-1994`), so that barrier does not cover them.

**Status**: NEW. Related to closed #2740, but **not a regression of it**:
- #2740's fix (51c725351) was documentation-only by design. The issue said "Do not blind-fix" and deferred any code change to a `BYRO_VALIDATION=1` run, so no device→host barrier ever existed at these sites to regress.
- `git log -S HOST_READ -- crates/renderer/src` shows every production `HOST_READ` edge being *added* (cluster telemetry 9c805cd79, ray probe 8e7582ed4, combustion 2325c1de4) and none removed.
- `groundcover.rs`, `screenshot.rs`, `depth_capture.rs` and `resources.rs` have never contained a `PipelineStageFlags::HOST` edge, and the ground-cover pipeline (637b65264, 2026-09-06) postdates #2740 entirely.

**Verified against**: HEAD `f97775ca8`.

## Description

- The Vulkan spec's note on waiting for fences says that signalling a fence and waiting on the host does not guarantee that device writes are visible to the host: a fence's memory dependency covers only device access, so a memory barrier or other memory dependency is required.
  - Making device writes available to the host needs a memory dependency with `HOST_READ` in its destination access scope. That performs the device→host domain operation.
  - The host-side `invalidate_if_needed` (#2752), which both `harvest` and `collect_image_health` already call, is the other half.
- The project's newer readbacks follow the rule:
  - cluster telemetry, `compute.rs:362-371`;
  - the combustion surface-light readback, `volumetrics.rs:1373-1382`, whose comment reads "the barrier supplies the device->host memory dependency that the fence's device-only access scope does not create by itself";
  - the selected-ray-probe record, `context/draw.rs:1984-1994`, a global `FRAGMENT_SHADER/SHADER_WRITE → HOST/HOST_READ` barrier;
  - the sky-filter test, `sky_cube/filter/tests.rs`: `TRANSFER_WRITE → HOST_READ` before its fence wait.
- The four readbacks above do not. The last barrier on the ground-cover counters (#4181's widened publish edge) is `COMPUTE_SHADER → DRAW_INDIRECT | VERTEX_SHADER | TRANSFER`. It orders the copy after the scatter but says nothing about the host.

## Evidence

- `grep -rn HOST_READ crates/renderer/src` finds exactly three production sites (`compute.rs:362`, `volumetrics.rs:1381`, `context/draw.rs:1993`). Every other hit is in a test.
- No `PipelineStageFlags::HOST` destination follows any `TRANSFER` copy in production code.
- The remaining `create_host_visible` users (`exposure_meter.rs`, `groundcover_bench.rs`, `water.rs`, `sky_cube.rs` and the per-pass parameter UBOs) are host→device uploads, not readbacks, so these four are the complete set of readbacks without the edge.

## Impact

Stale reads are possible on a `GpuToCpu` memory type that is `HOST_CACHED` and not coherent, or on a driver that does not flush at submit end. Affected data:
- `GroundCoverStats`: blade counts, the §11.3 density histogram and extrema. EXAL ground-cover tuning reads these directly.
- The image-health smoke gate. A stale read turns a NaN gate into one that passes anything.
- Golden-frame screenshots.
- Depth captures.

**Trigger conditions**: every frame with ground cover active, and every image-health / screenshot / depth-capture readback, on such a memory type.

**Verification path**: no validation-layer signal exists for this class. Syncval does not model host reads of mapped memory, which is why #2740's "run `BYRO_VALIDATION=1` first" precondition could never be met. The evidence is the spec text plus the in-tree precedent above. Only a non-coherent readback device can show the failure empirically.

## Related

- #2740 (closed, docs-only; see Status), #2752 (the host-side invalidate half), #4181 and #4293 (the earlier ground-cover readback edges).
- #4182 (closed) concerns the *opposite* direction and does not settle this. Host writes flushed before `vkQueueSubmit` are covered by the submit's implicit host-write ordering, which is why the HOST→device barriers it discusses are unneeded. The device→host direction has no such implicit guarantee.
- CONC-D1-2026-09-21-01 (#4601): the ground-cover `prepare` that runs `harvest` is itself an unlisted rider on the all-slots fence wait.

## Suggested Fix

- Add one global `memory_barrier(TRANSFER | FRAGMENT_SHADER, TRANSFER_WRITE | SHADER_WRITE → HOST, HOST_READ)` as the last command before `end_command_buffer`, which covers all four sites. Alternatively, add per-site edges that mirror `volumetrics.rs:1373-1382`. Either change is purely additive.
- Add a pin that every `create_host_readback` consumer's writer is followed by a `HOST_READ` edge.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every `create_host_readback` / `MemoryLocation::GpuToCpu` buffer (image health, screenshot, depth capture, ground-cover counters, cluster telemetry, ray probe, RT-LOD telemetry, combustion) is either covered by the new edge or already has one
- [ ] **TESTS**: a source-scan pin fails if the `HOST_READ` edge is removed, or recorded before the last readback writer (the presentation pass, the screenshot copy)


---

# 4603: CONC-D3-2026-09-21-01: The `lock-order-check` CI lane has been red on every sampled main run since at least 09-14, so two real lock-order cycles it caught went unnoticed

State: OPEN  Labels: ['bug', 'medium', 'tech-debt', 'concurrency']

**Severity**: MEDIUM. A defence-in-depth gap on the HIGH-floor ECS-deadlock class, graded like SAFE-D4/D5-2026-09-21-01 (#4595, #4596). · **Dimension**: 3 — ECS Lock Ordering & Deadlock
**Location**: `.github/workflows/ci.yml:186-198`, job `lock-order-check` ("ABBA lock-order detector"). It runs `cargo test --workspace` under `BYRO_LOCK_ORDER_CHECK: 1`, without `--no-fail-fast`, and has no step that tells a lock-order panic apart from any other test failure.

**Status**: NEW.
- The same lane was red at HEAD twice before: #3580 (2026-08-30) and #3819 (2026-09-03). Both were closed by fixing the cycles of the day, with no structural change to the lane. Those fixes are still in place, so this is not a regression of either. It is the structural gap that has let the lane go red a third time.
- SAFE-D5-2026-09-21-01 (#4596) covers the other lane (`vulkan-validation`). This is the lane that `/audit-concurrency` Dim 3's first step names.

**Verified against**: HEAD `f97775ca8`. CI was read read-only via `gh run list`, `gh api …/actions/runs/<id>/jobs` and `gh run view --job <id> --log-failed`.

## Description

The lane no longer changes colour when a new cycle lands, so it cannot signal one. From the report, reconstructed from `gh run view` on main:

| Run(s) | Commit, date | Why the lane was red |
|---|---|---|
| 34858811547, 34997424662, 35102663503 | 8d08da793 09-14, fd0cd577c 09-15, 5e7781197 09-16 | A **real cycle**. 29 bin tests panic at `lock_tracker.rs:476`: "`FormIdPool` while holding `FormIdComponent` … closes a cycle `FormIdPool → FormIdComponent → FormIdPool`". Never filed. |
| 35223046709, 35357015643 | 8c834e0be 09-17, e6c6405a8 09-18 14:33 | The 8 adapter-dependent `crates/ui` Ruffle tests ("Ruffle requires hardware acceleration"). Not a lock failure. |
| 35389142963 | 913fd39d8 09-18 20:01, the commit that added `walk_anim` | The **#4546 cycle** (`ActorCinematicState → Transform → AnimationPlayer → ActorCinematicState`, 5 walk_anim tests). The lane caught it the day it landed, but the colour stayed red→red. #4546 was filed from a manual run on 09-20 and fixed on 09-21 (c1f38e3da). |
| 35658431384 | f97775ca8 (HEAD) | The same 8 `crates/ui` tests, after every lock-relevant binary passed (bin 2309, core 768, physics 175, renderer 1139, …). |

**Publish-time re-check** (read-only, this pass):
- The `ABBA lock-order detector` job concluded `failure` on **all 300 main-branch CI runs from 2026-09-03 00:05 UTC (run 33697968024) through HEAD (run 35658431384)**. The lane has not been green on main at any point since #3819 closed.
- **The FormIdPool↔FormIdComponent cycle started five days before the report's first sample, and was fixed only incidentally.**
  - Bisected over main runs: run 34350867263 (60928c1ee, 09-09 12:24) fails only on the Ruffle tests, and its bin binary passed (2010 tests). Run 34353152965 (549aa9f65, 09-09 12:48) is the first with the cycle.
  - 549aa9f65 ("Fix #3299") added `byroredux/src/cell_loader/stream_snapshot.rs`. Its `global_form_id` took `world.try_resource::<FormIdPool>()` and then `world.get::<FormIdComponent>(entity)` under it, the reverse of the established `FormIdComponent`-first order.
  - d8255b2e2 ("Implement persistent reference state management…", 09-16) swapped those two lines and copies the id out before taking the pool. The cycle is absent from that commit's own run (35158845923) and from every later run sampled.
  - The cycle therefore sat on main for about seven days behind an already-red lane.
- Other red causes in the window: `tools/byro-launcher`'s `engine::tests::supervision::a_crash_carries_its_code_and_the_last_thing_the_engine_said` (run 33751180670, 09-03 11:42), and the Ruffle `crates/ui` tests (run 33982391005 on 09-05, and the sampled runs through 09-09).

## Evidence

- The report's job logs are saved at `/tmp/audit/concurrency/lo_*.log` and `lockorder_job_head.log`. This pass re-fetched runs directly from GitHub: 34858811547, 34597216311, 34763397456 and 34848442541 (the FormIdPool cycle), 33751180670 (launcher), and 33982391005 and 35658431384 (Ruffle).
- Without `--no-fail-fast`, cargo stops at the first failing test binary. While a bin-binary cycle was present (09-09 → 09-16, 09-18 → 09-21), every later binary (core, physics, scripting, save, …) and all doctests never ran under the detector. At HEAD the Ruffle failure is in `byroredux_ui`, which runs after the lock-relevant binaries, so those did run and pass.

## Impact

- Both known escapes were latent ABBA risks between systems. #4546's own issue notes that its cycle would deadlock under a schedule change.
- The `vulkan-validation` lane is also inert (SAFE-D5-2026-09-21-01, #4596), so CI currently gives no lock-order signal at all. A new cycle on a path the test suite exercises merges with no colour change.

**Trigger conditions**: any commit that introduces a new lock-order cycle on a path the test suite exercises.

## Related

- SAFE-D4-2026-09-21-01 (#4595): the same red `cargo test --workspace` (the 8 `crates/ui` tests) makes the `cargo-test` job skip its clippy step. Same root cause, separate consequence.
- SAFE-D5-2026-09-21-01 (#4596): the `vulkan-validation` lane never reaches Vulkan, so the detector's only live-world CI run is inert too.
- #3580 and #3819 (closed): the previous two red episodes of this lane. #3819's impact section already warned that a red lane "cannot catch a *new* cycle landing on top of these".
- #4546, #3266, #1410, #2137.

## Suggested Fix

- Make the lane able to go green on its own. Gate the Ruffle adapter tests (a wgpu-adapter probe or `#[ignore = "needs a Vulkan adapter"]`, the same fix #4595 needs), or pass `--exclude byroredux-ui` in this job.
- Add `--no-fail-fast`.
- Add a second gate that fails the job if and only if the output contains `lock-order cycle`, so a lock failure is distinguishable from any other red.
- The report also asks that the FormIdPool↔FormIdComponent episode be filed for the record. It is recorded above (549aa9f65 → d8255b2e2) rather than as a separate issue, since it is fixed at HEAD.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D3-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every other CI job that runs a fail-fast `cargo test` checked for the same masking (the `cargo-test` job's clippy step is #4595; the `vulkan-validation` lane is #4596)
- [ ] **TESTS**: a main-branch run after the change shows the `ABBA lock-order detector` job green, and a deliberately introduced cycle on a throwaway branch turns it red with the cycle gate's message


---

# 4604: CONC-D1-2026-09-21-02: `draw_frame_guards_on_empty_framebuffers_before_acquire` is vacuous: its needles match only the test's own literals

State: OPEN  Labels: ['bug', 'renderer', 'low', 'vulkan', 'sync', 'test-gap']

**Severity**: LOW (test gap; the property holds today) · **Dimension**: 1 — Vulkan Queue & AS Sync (acquire discipline, `image_available` signal-pending)
**Location**: `crates/renderer/src/vulkan/context/draw.rs:3017-3056`, `mod framebuffers_empty_guard_tests` / `fn draw_frame_guards_on_empty_framebuffers_before_acquire`

**Status**: NEW (the test has been vacuous since 7463204eb, #3282, 2026-09-02)
**Verified against**: HEAD `f97775ca8`.

## Description

- The test asserts that `draw_frame`'s empty-framebuffers guard precedes `.wait_for_fences(` and `.acquire_next_image(` in `draw.rs`. Both calls moved to `context/sync_and_acquire_frame.rs` (`:65` and `:150`) in 7463204eb (#3282). Since then both `find`s match the test's own literals (`draw.rs:3036`, `:3039`).
- So `guard_pos < wait_pos` always holds. Deleting the production guard also passes: the guard needle then matches the test's own literal at `:3025`, which still comes before `:3036`.
- #3991 repaired the identical self-match in the sibling test `skin_dispatch_ran_is_reset_before_both_early_return_guards` (its comment says so) but missed this one.

## Evidence

- `grep -n '\.wait_for_fences(\|\.acquire_next_image(' crates/renderer/src/vulkan/context/draw.rs` returns only lines 3036 and 3039, both inside this test.
- Found at publish time: three sibling tests in this file (`:3148`, `:3169`, `:3238`) use the same guard needle, `find("if self.swapchain.framebuffers.is_empty() {")`. Their ordering assertions are live while the production guard (`draw.rs:1797`) exists. Their `.expect("draw_frame must guard on empty framebuffers (#1211)")` can never fire, though: if the guard is deleted, the needle falls through to this test's literal at `:3025`.

## Impact

The #1211 contract has no live pin. That contract is to skip the frame *before* acquiring; otherwise `image_available[frame]` is left signal-pending, which trips `VUID-vkAcquireNextImageKHR-semaphore-01779` on the next acquire. The property itself holds today: the guard at `draw.rs:1797` precedes the `self.sync_and_acquire_frame(&mut t)` call at `:1812`.

## Related

#1211 (the contract), #3282 (the split that stranded the needles), #3991 (the sibling repair), #3442 (the compose-needles-at-runtime technique).

## Suggested Fix

- Anchor on `self.sync_and_acquire_frame(&mut t)` in `draw.rs`: the guard must precede it.
- Assert that the wait and the acquire live in `sync_and_acquire_frame.rs`.
- Compose every needle at runtime so that none can match the test's own source.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D1-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: the three other `framebuffers.is_empty()` needles (`draw.rs:3148`, `:3169`, `:3238`) and every other `include_str!("draw.rs")` test checked for self-matching literals
- [ ] **TESTS**: mutation check: deleting the production guard, or moving it after `sync_and_acquire_frame`, makes the repaired test fail


---

# 4605: CONC-D3-2026-09-21-02: The new P2 combat systems re-open two closed hold-stack patterns (#3444 shadowed guard; #3473 hold across a helper)

State: OPEN  Labels: ['bug', 'low', 'gameplay', 'combat', 'concurrency']

**Severity**: LOW (latent; no reverse edge exists today, and both systems are exclusive) · **Dimension**: 3 — ECS Lock Ordering & Deadlock (guard lifetime in system bodies)
**Location**:
- `byroredux/src/systems/combat_anim.rs:104-107` (`combat_feedback_system_inner`), with the guard live through the sound pass at `:329-417`
- `byroredux/src/systems/combat_ai.rs:61-156` (`npc_combat_ai_system`'s read pass) → `byroredux/src/combat.rs:369-465` (`attack_damage` → `melee_damage_charal_bonus`)

**Status**: NEW (recurrences of closed #3444 and #3473)
**Verified against**: HEAD `f97775ca8`.

## Description

1. **`combat_feedback_system_inner`** (ec3a18d2f, 2026-09-21):
   - `let Some(clips) = world.try_resource::<DraugrCombatClips>() else { return; }; let clips = *clips;` shadows the guard without dropping it. This is the exact #3444 defect (`let config = *config;`).
   - The `DraugrCombatClips` read guard therefore stays live for the whole function:
     - the read pass: `HitEvent`, `CombatState`, `PlayerEntity`, `GlobalTransform`, `PlayerMode`, `DraugrCombatAnim`, `Dead`, `AnimationPlayer`, `AnimationTarget`;
     - the write pass: the `AnimationPlayer` and `DraugrCombatAnim` writes;
     - the sound pass, commented "after every component write, locks dropped" (`:329`): `SoundArchiveProvider` (BSA extract + decode), then an `AudioWorld` write.
2. **`npc_combat_ai_system`** (f61ea0447, 2026-09-13):
   - The read pass holds the `AiCombatState` and `Transform` query guards (`combat_q`, `transform_q`) across its loop, and calls `crate::combat::attack_damage(world, entity)` inside it.
   - `attack_damage` → `melee_damage_charal_bonus` takes `MeleeDamageConfig` (scoped) → `CharacterRuleset` → `ActorValues` → `CharacterLevel` beneath those guards, plus `EquippedWeapon` / `CreatureAttack`. The same loop also takes `Dead`, `WalkSpeed`, and `EquippedWeapon` again (via `attack_reach_bu` / `attack_cooldown_seconds`).
   - That is the five-deep hold stack across a helper call that `attack_damage`'s own #3473 comment says the #2270 "snapshot before you iterate" house rule prohibits. #3473 fixed the callee's own `EquippedWeapon` binding; this caller re-creates the stack one level up.
   - The new `Transform → CharacterRuleset / ActorValues` edges are not in `docs/engine/ecs.md`, which records only `CharacterRuleset → ActorValues` (#3441). The combat_ai tests install no weapon or ruleset, so `attack_damage` returns before the CHARAL chain and the detector never records these edges.

## Evidence

No cycle exists today:
- `DraugrCombatClips`' only other readers (`populate_draugr_combat_clips` and its test) take `&mut World`.
- `CharacterRuleset → ActorValues → CharacterLevel` is consistent at `crates/core/src/character/regen.rs:213-226` and `crates/scripting/src/condition.rs:676-682`.
- Every AI-walker `WalkSpeed`/`Dead` read happens under a live `Transform` read.

## Impact

- Both sites add spurious or undocumented edges to the lock graph, and the combat_feedback comment "locks dropped" is false.
- A future `Transform`-after-CHARAL site, or promoting either system to a parallel lane, would close a cycle with no test signal. With the `lock-order-check` CI lane red, there would be no CI signal either (CONC-D3-2026-09-21-01 (#4603)).

## Related

- #3444 (closed; shadowed guard), #3473 (closed; hold across a helper), #2270 (the house rule), #4325 (closed; the `PhysicsWorld` half of combat_ai's lock discipline).
- ECS-2026-09-21-D5-02 (#4574): combat_ai's undeclared `WalkSpeed` in its `Access` row, plus four other exclusives' under-declarations.
- CONC-D3-2026-09-21-01 (#4603): the CI lane that would have to catch a cycle here.

## Suggested Fix

- Take the clips copy in a scoped expression so the guard dies at the copy, e.g. `let Some(clips) = world.try_resource::<DraugrCombatClips>().map(|c| *c) else { return; };`.
- In `npc_combat_ai_system`, compute per-attacker reach, damage and cooldown after the `AiCombatState`/`Transform` guards drop, in a second pass over the collected entities.
- Otherwise, document `Transform → CharacterRuleset` (and `AiCombatState → …`) in `docs/engine/ecs.md`.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D3-2026-09-21-02)

## Completeness Checks
- [ ] **LOCK_ORDER**: after the change, the guard scopes preserve TypeId-sorted acquisition, and every remaining nested edge is in `docs/engine/ecs.md`
- [ ] **SIBLING**: the other combat / AI systems (`combat_input_system`, the AI walkers) checked for `let x = *x;` guard shadowing and for helper calls under live guards
- [ ] **TESTS**: a combat_ai test that installs `EquippedWeapon` + `CharacterRuleset` + `MeleeDamageConfig` runs under `BYRO_LOCK_ORDER_CHECK=1`, so the detector records the CHARAL edges


---

# 4606: PERF-D5-2026-09-21-01: The top-of-frame wait on every frame-in-flight fence idles the GPU for all of `draw_frame`'s CPU recording, on every GPU-bound frame

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'vulkan', 'sync', 'performance']

**Severity**: MEDIUM (throughput: suboptimal CPU/GPU pipelining, no correctness impact) · **Dimension**: 5 — GPU Pipeline
**Location**:
- The wait: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:65`, `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` in `sync_and_acquire_frame`. The premise comment is at `:59-60`.
- `crates/renderer/src/vulkan/context/draw.rs:1812`: `draw_frame`'s first step (`sync_and_acquire_frame`). Recording follows (`cmd_t0` at `:1952`, `cmd_record_ns` at `:2125`), then the single `queue_submit` at `:2197`.
- `crates/renderer/src/vulkan/sync.rs:20-97`: the rider list that depends on the wait staying all-slots.

**Status**: NEW for the throughput cost. The mechanism predates the baseline: #282 introduced it, and #3442 kept it all-slots at N = 2.
**Verified against**: HEAD `73aaed7b9`, which is docs-only on top of the audited `f97775ca8`. Read at the symbols.

## Description

With `MAX_FRAMES_IN_FLIGHT = 2`, the "other" slot's fence belongs to frame N-1. On a GPU-bound frame, that is the submission the GPU is still executing. `sync_and_acquire_frame` waits on the whole `in_flight` array before `draw_frame` N records anything, so the GPU queue is empty when the wait returns. From then until N's `queue_submit`, the GPU has nothing to run while the CPU does:

- swapchain acquire;
- camera/light assembly;
- skin and TLAS recording (`tlas_build`);
- `build_and_upload_instances` (`ssbo_build`);
- geometry and post-pass recording (`cmd_record`, disjoint from the other buckets);
- the submit call itself.

The frame period is therefore about `max(T_pre, GPU) + T_post`, not `max(T_pre + T_post, GPU)`. Only the app work before `draw_frame` overlaps the GPU. The engine keeps one frame in flight at the `draw_frame` boundary, not two.

The comment at `:59-60` says: "Cost stays zero in practice — the GPU is rarely more than 1 frame behind the CPU, so the other fences are almost always signaled". That is backwards for the GPU-bound case, where the other fence is exactly the frame the GPU is still running.

## Evidence

- `t.fence_wait_ns` is measured around exactly this call (`sync_and_acquire_frame.rs:61-68`).
- The bench-of-record medians (`docs/audits/BENCH_stepped-camera_4c9a5b36.tsv`, re-read at publish time) show most of each GPU-bound frame spent in this wait:
  - Prospector TAA: `fence_ms` 10.59 of 13.08 ms `wall_ms`;
  - Prospector FSR-Q: 7.39 of 9.58;
  - Whiterun TAA: 7.53 of 10.74;
  - MedTek TAA: 16.12 of 37.64.
- The upper bound on the post-wait share is `wall − fence`: 2.49 ms Prospector TAA, 3.21 ms Whiterun TAA, 6.99 ms Dugout TAA. The bound also includes CPU work before `draw_frame`, so it is loose.

## Impact

- GPU-bound scenes (the RT default) lose `T_post` every frame. At about 1 ms of post-wait CPU, that is about 8-10 % throughput (*est.*).
- Every CPU cost inside `draw_frame`, such as the instance build and command recording, becomes frame time even on GPU-bound frames.
- Confidence: high on the mechanism (fence semantics plus `draw_frame`'s order). Medium on the magnitude: the post-wait share has not been measured.

## Related

- #4601 (CONC-D1-2026-09-21-01, open, MEDIUM): the *safety* side of the same wait. Its all-slots argument is unpinned, and nine riders depend on it: the seven in the #870/#3643 list plus the post-present TLAS scratch shrink and ground-cover `prepare`. That issue mentions the lost overlap only as context.
- #282: the SVGF previous-slot G-buffer read, the original reason for waiting on the other slot.
- #870, #3643: the rider list. #3442 made the wait all-slots. #4516 added the last rider.

## Suggested Fix

Measure first. On a GPU-bound TAA bench, the `cpu_ms:` sum `acquire + tlas_build + ssbo_build + cmd_record` is the per-frame GPU idle. An Nsight Systems or RenderDoc timeline shows the queue gap directly. If `T_post` is about 1 ms or more:

1. Make all nine riders per-FIF, or defer-destroy them (#4601's list).
2. Cover the SVGF previous-slot G-buffer read (#282) with an in-command-buffer barrier. A barrier's first scope includes earlier submissions on the same queue.
3. Only then wait on `in_flight[frame]` alone.

Changing the wait first is exactly the nine-site use-after-free that #4601 warns about. The speculative-Vulkan rule applies: validate with `BYRO_VALIDATION=1` plus RenderDoc on both upscaler modes before landing. Either way, correct the "cost stays zero" comment at `:59-60`.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D5-2026-09-21-01)

## Completeness Checks
- [ ] **MEASURE**: `T_post` captured on a GPU-bound TAA bench (Prospector / Whiterun) before any wait-policy change
- [ ] **UNSAFE**: If the fix changes the fence wait or adds `unsafe`, the SAFETY comment states the upheld invariant
- [ ] **SIBLING**: All nine riders on the all-slots wait (#4601) are per-FIF or deferred before the wait narrows
- [ ] **DROP**: If Vulkan objects change (per-FIF rider copies), the Drop impl is still reverse-order correct
- [ ] **TESTS**: A pin fails if the wait narrows while any rider is still slot-shared (coordinate with #4601's argument pin)


---

# 4607: PERF-D1-2026-09-21-01: Ground-cover host collection rebuilds std SipHash maps/sets and ~10 fresh allocations every exterior frame

State: OPEN  Labels: ['bug', 'medium', 'performance', 'terrain-exterior']

**Severity**: MEDIUM (redundant allocation and hashing on the per-frame render path; the #2923 rule class) · **Dimension**: 1 — CPU Hot Paths
**Location**:
- `byroredux/src/render/groundcover.rs:14`: `use std::collections::{HashMap, HashSet};`
- `byroredux/src/render/groundcover.rs:112-181`: `GroundCoverResidency::reconcile`
- `byroredux/src/render/groundcover.rs:247`, `:268`, `:313`: `collect_groundcover_frame`'s `resident` and `candidates` Vecs and its `emitted` map
- `byroredux/src/render/groundcover.rs:418`: `collect_groundcover_disturbers`' `found` Vec
- Caller: `byroredux/src/app_frame.rs:369` → `collect_and_prepare_groundcover` (`:727-779`). It runs every frame while the ground-cover pipeline exists and `--groundcover-off` is not set.

**Status**: NEW. Introduced by `6f2831d5c` (2026-09-14) and `fd0cd577c` (2026-09-15). At the 09-11 baseline (`b3db49fa`), the file's only hash collection was a test `HashSet`.
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

`GroundCoverResidency::reconcile` allocates all of these every frame:
- `desired: HashMap<ChunkKey, ChunkCandidate>`;
- `wanted: HashSet<ChunkKey>`, which duplicates `desired.contains_key`;
- `resident: HashSet<ChunkKey>`;
- the `pending` Vec and the returned `Vec<(usize, ChunkCandidate)>`.

`collect_groundcover_frame` adds fresh `resident` and `candidates` Vecs and a std `emitted: HashMap<usize, u32>`. `collect_groundcover_disturbers` adds a fresh `found` Vec. The App already keeps persistent scratch for the *outputs* (`groundcover_cells`, `groundcover_chunks`, …) but not for these intermediates.

Every hashed collection here is std SipHash, in `byroredux/src/render/`. `_audit-common.md`'s hot-path rule (#2923) says this path stays `FxHashMap`/`FxHashSet` end to end, and that a reintroduced std map there is the regression.

The comment at `:308-312` has been stale since the residency ring. It reads: "Survivors arrive in walk order, so one cell's chunks are contiguous and only the last emitted cell can match". `reconcile` returns entries in slot order, so `emitted` does need a map, but not a hashed one.

## Evidence

- In production code, `grep -n 'HashMap\|HashSet' byroredux/src/render/groundcover.rs` → `:14`, `:116`, `:120`, `:139`, `:313`. The `:809` hit is inside the `#[cfg(test)]` module.
- `git show b3db49fa:byroredux/src/render/groundcover.rs | grep -n 'HashMap\|HashSet'` finds only a test `HashSet`.
- Draw distance 3000 plus the chunk bound gives about 135 candidates per frame, at about 6 hash operations each. The ring is bounded to a 15×15 window: `radius_chunks = ceil((3000 + 362) / 512) = 7`.

## Impact

*Est.* 20-40 µs of main-thread time on every exterior frame, from about 800-950 SipHash operations plus about 10 heap allocations. The #2923 guard (the `must stay FxHashSet (#2923)` assertion in `crates/renderer/src/vulkan/context/mod.rs`) pins only named `VulkanContext` fields, so it cannot see this site. No quantitative guard exists for it.

## Related

- #2923: the hot-path Fx rule. #3682, #3137 and #3059 are earlier instances of the same class, all closed and fixed at their own sites; this is a new site, not a regression of them.
- #4609 (PERF-D1-2026-09-21-02): the same per-frame ground-cover collector family (atlas and species table rebuilt every frame).
- #4610 (PERF-D8-2026-09-21-02): the App-owned ground-cover scratches have no `ScratchTelemetry` rows.

## Suggested Fix

- Keep persistent `desired`, `pending` and result buffers on `GroundCoverResidency`, reused with clear + extend.
- Use `FxHashMap`, or a grid-indexed Vec over the bounded 15×15 window.
- Drop `wanted` in favour of `desired.contains_key`.
- Make `emitted` a `Vec<Option<u32>>` indexed by the dense `candidate.cell` ordinal, and fix the stale `:308-312` comment.
- Optionally, turn the #2923 guard into a source scan of `byroredux/src/render/` for `std::collections::Hash*`, so the rule is enforced as written.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: The other per-frame ground-cover collectors (`collect_groundcover_species`, `collect_groundcover_species_table`, `collect_groundcover_disturbers`) are checked for the same fresh-allocation / std-hash pattern
- [ ] **TESTS**: A regression test pins the fix, for example a source scan of `byroredux/src/render/` production code for `std::collections::Hash*`, or an allocation-count assertion on `reconcile`


---

# 4608: PERF-D1-2026-09-21-03: The MenuXml HUD re-rasterizes, copies and synchronously uploads the whole swapchain-sized frame on every changed tick (≤ 30 Hz while the camera turns); its budget was set at 720p

State: OPEN  Labels: ['bug', 'renderer', 'medium', 'performance', 'game:fnv', 'game:fo3', 'game:oblivion', 'ui']

**Severity**: MEDIUM (a periodic main-thread spike on the playable HUD path; opt-in `--hud`) · **Dimension**: 1 — CPU Hot Paths
**Location**:
- `byroredux/src/app_frame.rs:873-893`: `tick_hud_overlay`, with `.map(<[u8]>::to_vec)` at `:886`
- `byroredux/src/hud.rs:423`: the overlay and its three textures are sized to `ctx.swapchain_extent()`
- `byroredux/src/hud.rs:553-627` (`MenuXmlHud::render`) and `:642-667` (`upload_frame`)
- `crates/menuxml/src/menu.rs:342-356`: `render_frame` runs a full `EvalState::resolve_all` and a full `frame.clear` on every call
- `crates/renderer/src/texture_registry/mod.rs:899` (`write_rgba_inplace`) → `crates/renderer/src/vulkan/texture.rs:137-232` (`Texture::overwrite_rgba_pixels`) → `with_one_time_commands` (`texture.rs:808`, body `:839-990`)

**Status**: NEW. A sibling of open #3429, but a different route. Introduced by `ffda4ea95` / `dc306a6a0` (2026-09-18).
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

Any camera yaw changes the HUD signature: heading is quantised to 0.1° (`hud.rs:565-573`). The rate limit caps refreshes at one per 33 ms (`HUD_REFRESH_INTERVAL`). Each changed tick does this on the main thread, before `draw_frame`:

1. menu evaluation and layout, then a full clear and raster at swapchain resolution (`render_frame`);
2. `.to_vec()` of the whole RGBA frame into a fresh allocation (`app_frame.rs:886`);
3. a staging memcpy of the same size (`overwrite_rgba_pixels`);
4. a one-time command buffer plus fence create → submit → wait → destroy. This is `with_one_time_commands`, not the `with_one_time_commands_reuse_fence` variant.

The component's own docs size this policy at 720p:
- the raster costs "~16 ms" in the debug profile and "pinned a machine" before the rate limit existed (`hud.rs:297-303`);
- "30 Hz keeps the compass visually continuous at a bounded ~3.5 MB/33 ms transfer budget";
- the `.to_vec()` comment (`app_frame.rs:883-885`) prices the copy at "one 3.5 MB memcpy per *changed* frame" and calls it noise.

The buffer actually tracks the output extent: 8.3 MB at 1080p and 14.7 MB at 1440p, ×2.25 and ×4 the documented figure.

The blocking fence costs less than it looks, because `draw_frame` already waits on every in-flight fence (#4606). It is still redundant. The 3-texture rotation makes the in-place write race-free, so the copy could be recorded into the frame's own command buffer.

## Evidence

- The code sites above, read at the symbols.
- **Confirmed at publish time**, from the safety leg's "note, not a finding" (`docs/audits/AUDIT_SAFETY_2026-09-21.md`, Dimension 3). `overwrite_rgba_pixels` takes its staging buffer from the registry's `StagingPool` (`pool.acquire(image_size)`) but never calls `StagingGuard::release_to`. `StagingGuard::drop` → `cleanup()` therefore destroys the `VkBuffer` and frees the allocation (`crates/renderer/src/vulkan/buffer.rs:637-658`). The closing comment at `texture.rs:229-230` says the buffer "can go back to the pool now (StagingGuard::drop)"; it does not. Once any pooled buffer large enough has been used up, every HUD refresh also pays a fresh swapchain-sized host-visible staging create + allocate + destroy + free.

## Impact

While the player turns with `--hud` on, every second frame at 60 fps pays:
- the raster;
- about three full-frame memory passes (the `Vec` copy, the staging memcpy, the transfer);
- the staging allocation churn.

All of it scales with output resolution. The release-profile magnitude has not been measured. Confidence is high on the mechanism and low on the release-build magnitude. The route serves the MenuXml titles: Oblivion, FO3 and FNV (`HudGameProfile`).

## Related

- #3429 (open, MEDIUM) covers the FO4/Skyrim Scaleform `--hud` path through `TextureRegistry::update_rgba` (recreate plus deferred destroy). Its scope is `update_rgba`. The MenuXml driver moved off that path to `overwrite_rgba_pixels` (see #3429's latest comment, from `AUDIT_CONCURRENCY_2026-09-21.md`), so this cost is not covered there. One shared in-frame upload design would fix both.
- #4606 (PERF-D5-2026-09-21-01): the all-slots fence wait that makes this upload's own fence partly redundant.
- #4601 (CONC-D1-2026-09-21-01): the rotation's safety rests on that all-slots wait, which #4601 asks to pin.
- #4515, #4516 and #4526 (closed) covered this rotation's hazard contract, frames-in-flight tripwire and memory ledger. None of them covers the per-refresh cost.
- #4593 (SAFE-D2-2026-09-21-01): the staging-pool `release_to` capacity rule that a `release_to` fix here must follow (pass the *requested* size, per #4512).

## Suggested Fix

- Raster only the damaged rects (bars, compass). Alternatively, raster at a fixed HUD-native resolution and let the overlay quad scale it.
- Remove the `.to_vec()` by split-borrowing the renderer's frame, or by swapping an owned buffer.
- Upload only the damaged rects, recorded into the frame command buffer. This pairs with #3429's in-frame upload restructure.
- Independently of the above, return the staging buffer to the pool in `overwrite_rgba_pixels` (`staging.release_to(pool, image_size)`) and fix its "goes back to the pool" comment.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-03)

## Completeness Checks
- [ ] **UNSAFE**: If the copy moves into the frame command buffer or `unsafe` changes, the SAFETY comment states the upheld invariant (rotation vs frames in flight)
- [ ] **SIBLING**: The Scaleform `update_rgba` path (#3429) and the other `overwrite_rgba_pixels` / `with_one_time_commands` callers are checked for the same per-refresh pattern
- [ ] **DROP**: If Vulkan objects change (a persistent staging ring), the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins the fix, for example that the staging buffer returns to the pool after `overwrite_rgba_pixels`, or that there is no per-refresh `Vec` copy


---
