**HEAD**: `9e6f08870` · **Baseline**: `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` (@ `f97775ca8`, 292 commits ago). A volumetrics-only follow-up, `AUDIT_CONCURRENCY_2026-09-23.md` (@ `2237da9c3`), was used for dedup. · **Audited**: Dims 1–7. Every dimension's `Paths:` had commits since the baseline: D1 45, D2 38, D3 36, D4 12, D5 22, D6 32, D7 19. · **Unchanged since baseline (skimmed)**: none at the dimension level. `crates/core/src/ecs/{scheduler,access}.rs` have zero commits; their guards were spot-checked.

# Concurrency & Synchronization Audit — 2026-09-28

**Command**: `/audit-concurrency` (all dimensions, depth `deep`).
**Severity scale**: `.claude/commands/_audit-severity.md`.

## Method

- **Delta scope.** Each dimension was scoped by `git log f97775ca8..HEAD -- <Paths>` and run as a separate Task agent, at most 3 at once. Agent notes are in `/tmp/audit/concurrency/dim_{1..7}.md`.
- **Orchestrator work.**
  - Ran every cargo guard once, centrally, so agents did not compete for the build lock.
  - Pulled the CI job logs with `gh run view`.
  - Checked the egui crate claim and the `render/mod.rs` `rayon::join` coverage gap directly.
- **No engine launch.** None of the Vulkan-sync findings has validation-layer evidence yet. The live validation lane is inert (CONC-D3-2026-09-28-03), so every barrier finding below is marked **HYPOTHESIS** with its confirming signal.
- **Dedup.**
  - Checked against all 4,873 issues (`/tmp/audit/concurrency/issues_all.json`, all states).
  - Checked against the `docs/audits/` concurrency, renderer, ECS, safety and physics reports from 09-21 to 09-28.

### Guard runs at HEAD (local, `TMPDIR=/mnt/data/tmp`)

| Command | Result |
|---|---|
| `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace --no-fail-fast` | 8,860 ok, **0 `lock-order cycle`**, 1 unrelated failure. The failure is `cli_args::tests::renderer_config_defaults_to_fsr_quality` (`byroredux/src/cli_args.rs:391`): `RendererConfig::default().auto_exposure == true`, but a bare CLI parse gives `false`. It is also red in CI "Test + Check + Clippy". Out of scope; see Routed. |
| `cargo test -p byroredux -- scheduler_access system_access_declaration` | 24 passed. Dim 4 re-ran it with `fragment_activation_order` added: 28 passed. |
| `cargo test -p byroredux-physics sync` | 30 passed |
| `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux-physics` | 191 passed |
| CI run 36499415560 (`a070baaad`) | "ABBA lock-order detector": success (but see CONC-D3-2026-09-28-05). "Vulkan validation layers (lavapipe)": failure (CONC-D3-2026-09-28-03). |

## Summary

| Severity | NEW | Regression | Existing |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 1 | 1 | 1 (#4780) |
| LOW | 13 | 0 | 0 |

**Headline.**
- **No CRITICAL or HIGH finding.** All 292 commits in the window trace clean on every HIGH-floor surface:
  - queue-mutex scope;
  - AS build and publish barriers;
  - TLAS refit identity;
  - deferred destruction;
  - teardown ordering;
  - the static scheduler proof (9 of 9 parallel systems fully declared, 0 undeclared types);
  - the physics snapshot-then-acquire discipline;
  - the new rayon sections (physics newcomer shape conversion, render extraction joins, stream-pool texture prefetch, parallel plugin walk);
  - the lock-free BSA/BA2/CSG positional reads.
- **The main risk is evidence, not code.**
  - The only CI lane that runs the real parallel batch under the lock-order detector, and the only one that boots under the Khronos validation layer, has never reached a Vulkan device since #4596 was closed (CONC-D3-2026-09-28-03, -04).
  - The lane that *does* run the detector exits 0 on anything but a named cycle (-05).
  - Every clean verdict on the parallel batch and on barriers in this report therefore rests on reading, not on a green dynamic gate.
- **New code from today (`a070baaad`, player body)** gets two LOWs:
  - an unregistered render-skip sink, `HiddenFirstPerson` (D3-02);
  - an avoidable one-frame yaw lag from a Late writer feeding a PostUpdate consumer (D4-02).

| ID | Sev | Dim | Status | Title |
|---|---|---|---|---|
| CONC-D1-2026-09-28-01 | MEDIUM | D1 | NEW | egui-ash-renderer's partial texture update transitions the whole egui image from `UNDEFINED`, with no source scope |
| CONC-D3-2026-09-28-03 | MEDIUM | D3 | Regression of #4596 | The `vulkan-validation` CI lane has never reached a Vulkan device since #4596 closed, so the only live-world run of the lock-order detector (and of… |
| CONC-D1-2026-09-28-02 | LOW | D1 | NEW | Two egui texture hazards rest on the all-slots fence wait but are missing from the rider list |
| CONC-D2-2026-09-28-02 | LOW | D2 | NEW | The narrowed palette dispatch records back-to-back dispatches that write one SSBO with no barrier between them (sync-validation WAW noise; HYPOTHESIS) |
| CONC-D3-2026-09-28-01 | LOW | D3 | NEW | weather_system's #4416 image-space block says `wd` is still live and that it follows the canonical order; `wd` was dropped 110 lines earlier |
| CONC-D3-2026-09-28-02 | LOW | D3 | NEW | `HiddenFirstPerson` is a third render-skip sink that ecs.md does not name and no detector test can see |
| CONC-D3-2026-09-28-04 | LOW | D3 | NEW | After a Vulkan init failure, `about_to_wait` still runs the full scheduler against a world that was never set up, and it panics instead of exiting … |
| CONC-D3-2026-09-28-05 | LOW | D3 | NEW | The `lock-order-check` lane captures `tee`'s exit status, so it is green on every failure except a detected cycle, including a test-build compile e… |
| CONC-D4-2026-09-28-01 | LOW | D4 | NEW | `PARALLEL_SYSTEMS` leaves out the cross-file hops that carry 17 acquisitions, and the scan skips `world.get`; 7 physics_sync types have no mechanic… |
| CONC-D4-2026-09-28-02 | LOW | D4 | NEW | `player_body_facing_system` writes in Late for a PostUpdate consumer; the one-frame body-yaw lag is avoidable, its justification is wrong, and noth… |
| CONC-D5-2026-09-28-01 | LOW | D5 | NEW | `container_loot_system`'s Access row does not declare the `PhysicsWorld` write that #4818 added to `pickup_loot` |
| CONC-D5-2026-09-28-02 | LOW | D5 | NEW | Nothing pins that `register_newcomers`' rayon section runs with no World guard live and no `&World` inside the closure |
| CONC-D6-2026-09-28-01 | LOW | D6 | NEW | `GpuPerFrameTimers::new` leaks the earlier TIMESTAMP query pools when a later slot's `create_query_pool` fails, and the caller treats the error as … |
| CONC-D7-2026-09-28-01 | LOW | D7 | NEW | On Windows, "lock-free positional reads" still serialise in the kernel for each archive handle |
| CONC-D7-2026-09-28-02 | LOW | D7 | NEW | BSA and CSG went lock-free with no multi-threaded extract regression; only BA2 has one |
| CONC-D2-2026-09-28-01 | MEDIUM | D2 | Existing: #4780 | The #3685 skip-clear latch still skips the temporal reset, so a skip on a latched slot leaves `history_valid` true |

---

## Findings

### CONC-D1-2026-09-28-01: egui-ash-renderer's partial texture update transitions the whole egui image from `UNDEFINED`, with no source scope

- **Severity**: MEDIUM.
  - The layout discard is certain from the code. Whether it produces a *visible* artifact depends on the driver, so that part is a HYPOTHESIS.
  - It affects only the debug overlay, and the defect is in a third-party crate. That is why this is not rated HIGH.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - The call site is `crates/renderer/src/vulkan/egui_pass.rs:250-255`, inside `draw_frame` while `cmd` is recording. It holds the graphics-queue Mutex.
  - The defect is in the dependency `egui-ash-renderer-0.11.0/src/renderer/vulkan.rs:443-534` (`Texture::cmd_update`), reached from `src/renderer/mod.rs:344-365` (`set_textures`, the `delta.pos == Some(..)` arm).
- **Status**: NEW. I found no issue or audit covering it. #1421, #1420, #1713 and #2786 cover the queue lock, the pool and the dependency comments, not this layout.
- **Description**: egui sends partial `ImageDelta`s (`pos: Some([x, y])`) whenever it rasterizes new glyphs into its font atlas. This is routine when a new panel or new text appears. `egui_pass.rs` forwards `output.textures_delta.set` unchanged to `Renderer::set_textures`. For a partial delta, egui-ash-renderer does three things:
  - It records `old_layout = UNDEFINED → TRANSFER_DST_OPTIMAL` over the **whole** subresource, with `src_stage = TOP_OF_PIPE` and `src_access = empty`.
  - It copies **only** the delta rectangle.
  - It transitions the image to `SHADER_READ_ONLY_OPTIMAL`.

  This has two consequences:
  - **Content.** A transition from `UNDEFINED` allows the implementation to discard the image's contents. Every texel outside the delta rectangle, meaning every glyph already in the atlas, becomes undefined.
  - **Sync.** A `TOP_OF_PIPE` source with empty access gives no execution dependency on the previous frame's `FRAGMENT_SHADER` reads of the same image (the egui draw in frame N-1). The WAR ordering exists only because the host all-slots fence wait retired frame N-1 before this submit. That is a host-side edge, not a device edge.
- **Evidence**:
  ```rust
  // egui-ash-renderer-0.11.0/src/renderer/vulkan.rs:461-483
  .old_layout(vk::ImageLayout::UNDEFINED)
  .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
  .src_access_mask(vk::AccessFlags::empty())
  ...
  device.cmd_pipeline_barrier(command_buffer,
      vk::PipelineStageFlags::TOP_OF_PIPE, vk::PipelineStageFlags::TRANSFER, ...)
  // then cmd_copy_buffer_to_image with image_offset/extent = the delta rect only
  ```
  Call path: `draw_frame` → `EguiPass::dispatch` (`egui_pass.rs:250`, `queue.lock()`) → `set_textures` → `Texture::update` → `execute_one_time_commands` (`vulkan.rs:560-595`: `queue_submit`, then `queue_wait_idle`).
- **Trigger Conditions**: The debug UI overlay is visible and a frame's `textures_delta.set` holds a partial update, for example the first appearance of a new glyph or font size.
- **Impact**: Previously rasterized overlay glyphs may become garbled or black after a partial update, until egui re-uploads the whole atlas. This happens only on drivers or memory layouts that actually discard on an `UNDEFINED` transition, such as compressed color layouts. The engine's own rendering is unaffected.
- **Verification Path**: Not reachable by `cargo test`.
  - Run `BYRO_VALIDATION=1` with the debug overlay open and force new glyphs, for example by typing into a console field. Look for `SYNC-HAZARD-WRITE-AFTER-READ` (or `…-WRITE-AFTER-WRITE` from the layout transition) on the egui texture image in the one-time submit.
  - Visual check: glyph corruption after new text appears. The live runs of 2026-09-27 did not exercise the overlay.
- **Related**: #1713 and #1421 (the queue-lock scope around this call), #2786.
- **Suggested Fix**: Patch the dependency, via a `[patch]` override or upstream, so that a partial update transitions from `SHADER_READ_ONLY_OPTIMAL` with `src = FRAGMENT_SHADER / SHADER_READ`. Keep `UNDEFINED` only for a freshly created image. Alternatively, have `egui_pass.rs` coalesce partial deltas into a full re-upload (`pos: None`). Confirm with the validation run above before and after the change.

---

### CONC-D3-2026-09-28-03: The `vulkan-validation` CI lane has never reached a Vulkan device since #4596 closed, so the only live-world run of the lock-order detector (and of the validation layer) is inert
- **Severity**: MEDIUM. This is the same defence-in-depth rating #4596 carried. It covers the HIGH-floor classes "ECS deadlock" and "Vulkan spec violation".
- **Dimension**: ECS Lock Ordering. The lane is the dynamic supplement to the static scheduler proof.
- **Location**:
  - `.github/workflows/ci.yml:312-360`, the job `vulkan-validation`.
  - `crates/renderer/src/vulkan/instance.rs:48`, `api_version(API_VERSION_1_3)`.
- **Status**: Regression of #4596 (closed 2026-09-22 by `dc44de735`). Its completeness check "a main-branch run shows the bench reaching Vulkan device selection under the validation layer" is unmet.
- **Description**:
  - `dc44de735` added `libxkbcommon-x11-0` and `BYRO_ALLOW_CPU_VULKAN_DEVICE=1`. That fixed the old winit panic, and the lane now fails one step later.
  - Every sampled `main` run dies at `vkCreateInstance` with `ERROR_INCOMPATIBLE_DRIVER`. ash renders that as "Unable to find a Vulkan driver". It is the loader's result when no usable ICD is found.
  - The job then fails on the engine panic described in CONC-D3-2026-09-28-04.
  - What never runs as a result:
    - No `VkPhysicalDevice` is ever enumerated.
    - No frame is rendered under `VK_LAYER_KHRONOS_validation`.
    - The rayon parallel batch never runs against a real loaded world under `BYRO_LOCK_ORDER_CHECK=1`. `ci.yml:314-321` calls that "exactly the workload the cross-thread lock-order graph was built for".
  - Both pin tests stay green because they check only the YAML text: `vulkan_validation_job_enables_the_lock_order_detector` (`byroredux/src/scheduler_access_tests.rs:370`) and `vulkan_validation_job_fails_on_a_panic` (`:390`).
- **Evidence**: `gh run view <id> --log` for the Vulkan-validation job. Eight of eight sampled runs show the identical signature: 122121a27 (09-24), 5c82276df, 9e6f8a978, 078f650ec, e26441c34, bad6ef2e2, 7e9da5dcc, efc059f3a, and a070baaad (run 36499415560, 09-28). The log reads:
  ```
  ERROR byroredux::app_events] Vulkan init failed: Failed to create Vulkan instance: Unable to find a Vulkan driver
  thread 'main' panicked at crates/core/src/ecs/world.rs:713:13:
  Resource `byroredux_scripting::papyrus_demo::PapyrusPlayerEntity` not found
  bench exit status: 101
  ```
- **Trigger Conditions**: Every CI run on `main`.
- **Impact**:
  - Every cross-thread lock-order claim for the real parallel batch rests only on the static access proof (Dim 4) and on reading the code (Dims 3 and 5). This audit's Dims 3–5 found no live hole, but no dynamic evidence backs that.
  - No CI lane can see a `VUID-*` or sync-validation hazard. That includes the HYPOTHESIS rows of this report: CONC-D1-2026-09-28-01 and CONC-D2-2026-09-28-02.
- **Verification Path**: CI. A green lane run must show a `Selected physical device` log line and 5 rendered frames.
- **Related**: #4596, #2138, #1429; CONC-D3-2026-09-28-04 and -05.
- **Suggested Fix**:
  - Add `VK_LOADER_DEBUG=error,warn,driver` and `ls /usr/share/vulkan/icd.d/` to the step, so the missing or incompatible ICD is named. The `lvp_icd.x86_64.json` path on ubuntu-24.04 is unverified, and newer Mesa may ship `lvp_icd.json`. Consider `VK_DRIVER_FILES`.
  - Add a lane assertion that grep-fails when no device-selection line appears, so a lane that cannot boot turns red for the right reason.

---

### CONC-D1-2026-09-28-02: Two egui texture hazards rest on the all-slots fence wait but are missing from the rider list

- **Severity**: LOW. This is a test and documentation gap. It is safe today. The precedent is #4851 and #4852, both LOW `sync`/`test-gap`.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - `crates/renderer/src/vulkan/egui_pass.rs:229-237` (`pending_free` → `free_textures`) and `:250-255` (partial `set_textures` overwrite).
  - The rider list is at `crates/renderer/src/vulkan/sync.rs:45-120`. Its pin is `frames_in_flight_contract_names_every_dependent_resource` at `sync.rs:634`.
- **Status**: NEW. This is the same class as #4601, #4851 and #4852 (all closed), for a resource none of them names.
- **Description**: Two egui operations are safe only because the frame-start wait covers every slot:
  - At the start of frame N's dispatch, `EguiPass::dispatch` frees `pending_free`, the texture IDs from frame N-1's `textures_delta.free`. `Renderer::free_textures` (`egui-ash-renderer mod.rs:415-425`) destroys the image, view and memory **immediately** and frees the descriptor set. Frame N-1's egui draw may have bound and sampled exactly those textures: egui frees a texture after the frame that last paints it. That is safe only if frame N-1 has retired, which only the **all-slots** wait at `sync_and_acquire_frame.rs:81` guarantees. A per-slot wait on `in_flight[N % 2]` retires N-2 only. The code comment at `egui_pass.rs:229-232` ("the fence at the top of `draw_frame` has waited on the previous frame's command buffer") states the premise but is not in the list.
  - The partial `set_textures` overwrite described in finding 01 has no device-side WAR edge, so it rests on the same premise.

  The full-replacement arm (`pos: None` on an existing ID) is self-protected: `from_rgba8`'s `queue_wait_idle` drains frame N-1 before `previous.destroy`.
- **Evidence**: `sync.rs` has 13 listed riders plus the #3429 note on dynamic RGBA. `grep -n "egui\|pending_free" crates/renderer/src/vulkan/sync.rs` finds no match. `the_all_slots_wait_argument_is_pinned` (`sync.rs:750`) and the rider-list guard exist, but neither names egui.
- **Trigger Conditions**: No live hazard. It becomes a use-after-free of an image and descriptor set the moment #4606's throughput work narrows the wait to `&[in_flight[frame]]` without re-deriving the riders.
- **Impact**: None at HEAD. The risk is to future changes: freeing an image or descriptor set that is still in use leads to device loss or a GPU page fault in the debug overlay.
- **Verification Path**: `cargo test -p byroredux-renderer frames_in_flight_contract` after adding the entry; it is a source pin.
- **Related**: #870, #3643, #4601, #4606, #4851, #4852.
- **Suggested Fix**: Add a rider entry to the #870 block for `egui_pass.rs`'s `pending_free` and the partial `set_textures` overwrite, and add `("pending_free", include_str!("egui_pass.rs"))` to the test's resource table. Alternatively, retire egui frees through a `MAX_FRAMES_IN_FLIGHT`-deep per-slot ring, so the wait can be narrowed safely.

---

### CONC-D2-2026-09-28-02: The narrowed palette dispatch records back-to-back dispatches that write one SSBO with no barrier between them (sync-validation WAW noise; HYPOTHESIS)
- **Severity**: LOW, HYPOTHESIS. It is not a real data race, because the ranges are disjoint. The risk is validation noise that can hide real hazards.
- **Dimension**: Compute → AS → Fragment Chains (skin chain, M29 palette)
- **Location**:
  - `crates/renderer/src/vulkan/skin_compute.rs:1275-1289`: the per-range `cmd_push_constants` + `cmd_dispatch` loop, with no barrier inside it.
  - `skin_compute.rs:907-953`: `plan_palette_dispatch`, which sorts and merges the ranges so they are disjoint.
  - Caller: `context/dispatch_skin_and_cluster.rs:248-290`.
- **Status**: NEW. The loop was introduced by eb7c82043 (#4204, 2026-09-16), before the baseline. The 09-21 audit verified the loop's per-FIF behavior but did not raise this. No issue exists.
- **Description**:
  - Every range dispatch writes `palette_buffer` through binding 2, which is bound at `range = palette_buffer_size` (the whole buffer).
  - `skin_palette.comp:77` early-returns outside `[bone_base, bone_end)`, and the plan merges overlapping ranges. So invocations of different dispatches never write the same address, and there is no race under the Vulkan memory model.
  - Sync validation, however, treats a descriptor access as touching its whole bound range. It should therefore report `SYNC-HAZARD-WRITE-AFTER-WRITE` on the second and later dispatch whenever the plan has two or more runs. That happens whenever the dirty skinned slots are non-contiguous, for example several NPCs with only some of them moving.
- **Evidence**: No `cmd_pipeline_barrier` appears between loop iterations. The only palette barrier is the post-loop COMPUTE→COMPUTE|VERTEX|FRAGMENT one at `dispatch_skin_and_cluster.rs:270-288`.
- **Trigger Conditions**: A frame whose palette plan has two or more disjoint runs, under `BYRO_VALIDATION`.
- **Impact**: Validation noise only. This project decides barrier changes from sync-validation output (#4293's WAW capture), so persistent false WAWs on every skinned frame dilute that signal.
- **Verification Path**: Run `BYRO_VALIDATION=1` on a cell with several skinned actors, some of them idle. Confirm that a `SYNC-HAZARD-WRITE-AFTER-WRITE` names `vkCmdDispatch` / `skin_palette` on the palette buffer. If none appears, close this as not reproducing.
- **Related**: #4204, #4205, #4293.
- **Suggested Fix**: Only if confirmed. Prefer one dispatch that reads the run list from a small SSBO, or bind each run's sub-range through a dynamic offset so the validator sees disjoint ranges. Avoid adding a per-range COMPUTE→COMPUTE barrier: it would serialize the tiny dispatches that #4204 split out.

---

### CONC-D3-2026-09-28-01: weather_system's #4416 image-space block says `wd` is still live and that it follows the canonical order; `wd` was dropped 110 lines earlier
- **Severity**: LOW (the comment is wrong about lock state; no edge is recorded)
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/weather.rs:1209-1234`. `drop(wd)` is at `:1099`.
- **Status**: NEW. Introduced by afd6a73f7 (#4416).
- **Description**: The comment reads: "The cross-fade target is re-read in the canonical
  `WeatherDataRes -> WeatherTransitionRes` order while `wd` is still live". But `wd` (the
  `WeatherDataRes` read) is dropped at `:1099`. Between the drop and this block, the only
  `WeatherDataRes` access is the scoped `try_resource_mut` at `:1104-1106`. At `:1219` the
  `WeatherTransitionRes` read is therefore taken with no guard held. It is then released at the
  end of the `if transition_t > 0.0` arm, before `ImageSpaceBase` (write) at `:1231`. The code is
  safer than the comment says.
- **Evidence**: The acquisition sequence in the block is:
  1. `CellLightingRes` (R; consumed by `is_none_or`)
  2. `WeatherTransitionRes` (R; scoped)
  3. `ImageSpaceBase` (W)

  No two of these guards overlap. The only other `ImageSpaceBase` reader,
  `image_space_modifier_system` (`crates/scripting/src/cinematic.rs:396-399`), copies the value
  out before taking `CinematicPresentationState`.
- **Trigger Conditions**: None at runtime. The risk is that a maintainer relies on the comment.
  An edit that trusts "wd is live" could, for example, re-read `wd` fields here by re-acquiring
  `WeatherDataRes` under `tr`. That would record `WeatherTransitionRes → WeatherDataRes`, the
  reverse of the `:798`→`:893` order that #3263 documented.
- **Impact**: Documentation rot in a lock-order comment. It sits on the exact pair #3263 closed
  as undocumented.
- **Verification Path**: n/a for today's code. A regression of the shape described above fires
  under `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux -- weather`, because
  `:798`/`:893` already record the forward edge.
- **Related**: #3263 (closed; the WeatherDataRes→WeatherTransitionRes order), #4416, #1103.
- **Suggested Fix**: Reword the comment to "no weather guard is live here; `tr` is scoped to the
  cross-fade arm and drops before the `ImageSpaceBase` write".

---

### CONC-D3-2026-09-28-02: `HiddenFirstPerson` is a third render-skip sink that ecs.md does not name and no detector test can see
- **Severity**: LOW. The code is correct today. This is a coverage and documentation gap on a
  pattern that went HIGH (#4983) six days ago.
- **Dimension**: ECS Lock Ordering
- **Location**:
  - `byroredux/src/player_body.rs:338-366` (`set_player_view`)
  - `byroredux/src/render/skinned.rs:90`
  - `byroredux/src/render/static_meshes.rs:363`
  - `docs/engine/ecs.md:618-623`
- **Status**: NEW. Introduced by a070baaad (today).
- **Description**: The P3 player body adds `HiddenFirstPerson`. Both render skips read it under
  `GlobalTransform` / `SkinnedMesh`, exactly like `PickedUp` and `NpcAppearanceHidden`.
  `set_player_view` currently gets the order right: it walks `mesh_entities_under` (`Children`,
  `MeshHandle`) at `:346`, before the marker write at `:347`. Two things keep that from being
  guarded:
  - `ecs.md:618` names only `PickedUp` and `NpcAppearanceHidden` as sinks.
  - Nothing registers the storage (`grep register::<…HiddenFirstPerson>` finds no hits; it is
    created lazily by `world.insert` in `attach_assembled_root`). So no render test ever takes
    the `X → HiddenFirstPerson` guard, and the process-wide graph never learns the render half of
    the cycle.

  #4983 was pinned with detector-gated replay tests. This marker has no such test.
- **Evidence**: The hazard mirrors #4983 exactly. Moving the walk inside
  `if let Some(mut hidden_q) = world.query_mut::<HiddenFirstPerson>()` would record
  `HiddenFirstPerson → Children`. Together with `Children → GlobalTransform` (propagation) and
  `GlobalTransform → … → HiddenFirstPerson` (`skinned.rs:81-90`), that closes
  `HiddenFirstPerson → Children → GlobalTransform → HiddenFirstPerson`. A future equip-restamp
  path is the likeliest place for such an edit: newly equipped part meshes currently receive no
  `HiddenFirstPerson`.
- **Trigger Conditions**: Latent. There is no live cycle today.
  - `set_player_view` runs from the winit V-key handler (`app_events.rs:539`, `input` dropped
    first).
  - It also runs from `player.view` via the exclusive debug drain.
  - The render passes run on the main thread outside the scheduler.
- **Impact**: A regression would be invisible to the lock-order lane, the only mechanical guard
  for this class.
- **Verification Path**: Add a `BYRO_LOCK_ORDER_CHECK`-gated test that does three things:
  1. registers `HiddenFirstPerson`;
  2. runs `build_skinned_palettes` / `collect_static_mesh_draws` and transform propagation on a
     world with a stamped body;
  3. then calls `set_player_view`.

  This follows the #4983 pin pattern. It goes red if the walk moves under the guard.
- **Related**: #4983 (closed), #4571, ECS-2026-09-28-D1-02.
- **Suggested Fix**:
  - Add `HiddenFirstPerson` to the ecs.md sink sentence.
  - Register the storage at boot (`boot/world.rs`, beside `PickedUp`). This also removes the
    `query_mut → None` special case noted at `player_body.rs:285-287`.
  - Add the replay test above.

---

### CONC-D3-2026-09-28-04: After a Vulkan init failure, `about_to_wait` still runs the full scheduler against a world that was never set up, and it panics instead of exiting cleanly
- **Severity**: LOW. This is a failure-path robustness bug. A user without a working Vulkan driver gets an ECS panic and exit code 101 after the real error.
- **Dimension**: ECS Lock Ordering (system execution outside a booted world)
- **Location**:
  - `byroredux/src/app_events.rs:292-295`, the `resumed` `Err` arm: `log::error!` then `event_loop.exit()`.
  - `byroredux/src/app_events.rs:611` and `:908-910`, where `about_to_wait` calls `self.scheduler.run(&self.world, dt)` with no boot-completed gate.
  - `crates/scripting/src/papyrus_demo/mod.rs:294`, the `world.resource::<PapyrusPlayerEntity>()` panic site.
- **Status**: NEW
- **Description**:
  - `event_loop.exit()` only requests an exit. winit still delivers `about_to_wait` for the current iteration.
  - `about_to_wait` runs every stage.
  - `rumble_on_activate_system` gets `Some` from both `query_mut` calls, because the storages are registered at boot. It then calls `world.resource::<PapyrusPlayerEntity>()`, and that resource is inserted only by scene setup (`byroredux/src/scene.rs:1227`/`:1274`). Scene setup never ran.
  - Any other system that assumes scene resources would fail the same way. The rumble system is simply the first to hit it.
- **Evidence**: The CI backtrace. Frame 5 is `rumble_on_activate_system` (`papyrus_demo/mod.rs:294`), frame 6 is `Scheduler::run` (`scheduler.rs:513`), and frame 7 is `App::about_to_wait` (`app_events.rs:910`).
- **Trigger Conditions**: `VulkanContext::new` returns `Err`. This happens with no ICD, an unsupported device, or the CPU device rejected without `BYRO_ALLOW_CPU_VULKAN_DEVICE`.
- **Impact**:
  - The clean "Vulkan init failed" error is followed by an unrelated panic.
  - In CI this panic is what fails the job (CONC-D3-2026-09-28-03), so it masks the real device-selection result.
- **Verification Path**: `cargo run -p byroredux -- --bench-frames 5` with `VK_ICD_FILENAMES=/nonexistent`. Expect the error line and then the panic.
- **Related**: CONC-D3-2026-09-28-03, #4596.
- **Suggested Fix**:
  - Record a boot-failed or `renderer.is_none()` state and return early from `about_to_wait` before `scheduler.run`.
  - Alternatively, exit the process with a non-zero status directly from the `Err` arm.

---

### CONC-D3-2026-09-28-05: The `lock-order-check` lane captures `tee`'s exit status, so it is green on every failure except a detected cycle, including a test-build compile error
- **Severity**: LOW. This is a CI-guard vacuity. The grep gate still catches real `lock-order cycle` panics.
- **Dimension**: ECS Lock Ordering (CI dynamic supplement)
- **Location**: `.github/workflows/ci.yml:218-228`.
- **Status**: NEW. The pattern arrived with the #4603 rewrite.
- **Description**:
  - The step runs `cargo test --workspace --no-fail-fast --exclude byroredux-ui 2>&1 | tee /tmp/lockorder_test.log`, then `status=$?`, and later `exit $status`.
  - No `shell:` key appears anywhere in `ci.yml`. GitHub's default `run` shell is therefore `bash -e {0}`, with no `pipefail`.
  - `$?` is `tee`'s status, which is always 0. `exit $status` is dead, and only the `grep -q "lock-order cycle"` branch can fail the job.
  - The #4603 comment says the goal is for "a lock failure [to] be distinguishable from any other red". The other reds were meant to stay red.
- **Evidence**:
  - At a070baaad, CI "ABBA lock-order detector" passes. "Test + Check + Clippy" fails on the same tree, which has a failing `cli_args::tests::renderer_config_defaults_to_fsr_quality`.
  - The local `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace --no-fail-fast` reproduces that one failure.
- **Trigger Conditions**: Any compile error in a test target, or any test failure that is not a detector cycle.
- **Impact**:
  - A compile break in the test build means zero tests run under the detector while the lane reports green.
  - This is the exact "lane can't tell" failure mode #4603 was fixing.
  - Same-thread reentrancy panics (`ECS deadlock detected: …`, `lock_tracker.rs:103/206/213`) do not contain "lock-order cycle". They are also caught by the Test job, so they are not lost, but this lane does not flag them.
- **Verification Path**: Push a branch with a deliberate `#[test] fn f(){panic!()}`. The lane stays green.
- **Related**: #4603, #4595, CONC-D3-2026-09-28-03.
- **Suggested Fix**:
  - Use `status=${PIPESTATUS[0]}`, or add `set -o pipefail` or `shell: bash`.
  - Optionally extend the grep to `ECS deadlock detected`.

---

### CONC-D4-2026-09-28-01: `PARALLEL_SYSTEMS` leaves out the cross-file hops that carry 17 acquisitions, and the scan skips `world.get`; 7 physics_sync types have no mechanical pin
- **Severity**: LOW (test gap; no live hole today)
- **Dimension**: Scheduler Access Declarations
- **Location**: `byroredux/src/boot/schedule/mod.rs:486-545` (table + its doc), `:611-658` (`acquired_in`); hops at `crates/physics/src/sync.rs:152` (`crate::water::apply_buoyancy`), `byroredux/src/systems/character.rs:607,1087,1472`, `byroredux/src/systems/camera.rs:121`, `byroredux/src/systems/animation.rs:65`
- **Status**: NEW. Related: #4573 (closed; its fix added mode, whole names and comment stripping, but not the `get` forms or the cross-file hops), #4064 (closed; about which systems are covered), #3964 / #1787 / #2676 (the same class of gap shipping on this system).
- **Description**: The table's doc (`mod.rs:497-500`) says "a hop into a different file is listed here explicitly rather than followed". Three parallel systems have unlisted hops.
  - `physics_sync_system` lists only `sync.rs`. Its buoyancy phase lives in `water.rs`, called as `crate::water::apply_buoyancy(world, …)`, and `fn_body(sync.rs, "apply_buoyancy")` returns None, so the scan never goes there.
  - `acquired_in` recognizes only `query` / `query_mut` / `resource` / `resource_mut` (plus `try_`). It does not recognize `world.get::<T>` / `get_mut` / `has`.
- **Evidence**: I re-ran the scanner line for line in Python, then again with the hops added and the `get` forms recognized. Results (actual vs. what the guard sees):

  | System | Actual types | Guard sees | Invisible to the guard |
  |---|---|---|---|
  | physics_sync_system | 24 | 13 | PhysicsWaterConstants, Ragdoll, TotalTime, WindField, WaterPlane, WaterVolume, WaterSurfaceMesh, WaterFlow, WaterCurrentVolume, **WaterContact=write**, **WaterContactScratch=write** |
  | player_controller_system | 28 | 24 (PhysicsWorld only as read) | **PhysicsWorld=write** (`set_kinematic_translation`, `set_linear_velocity`), **PendingDeathReconciliations=write** (`combat.rs:105`), WindField (`water.rs:394`), ActorControlState and ActorVitals (`world.get`) |
  | make_animation_system | 17 | 16 | Children (`anim_convert.rs:23`) |

  Only 4 of the 11 physics_sync types are backstopped, by the hard-coded needle list in `scheduler_access_tests.rs:206-219`. The baseline Dim 4 cited that list as the guard for this hop. The other 7 have no pin, and 2 of them are writes.
- **Trigger Conditions**: A future edit to `water.rs`, `combat.rs::queue_dead_actor_reconciliation`, a physics `set_*` helper, or a `world.get` read inside a parallel body adds or changes an acquisition without updating the declaration.
- **Impact**: None live. All 17 are declared by hand today (0 undeclared across all 9 systems). The next slip ships green, and `known_conflict_count()==0` is computed from an incomplete row. On physics_sync, this exact slip has already shipped four times, each time through `water.rs` or diagnostic helpers.
- **Verification Path**: Add the hop tuples, then re-run `cargo test -p byroredux --bin byroredux -- system_access_declaration`. In my simulation, the extended table has 0 undeclared at HEAD, so the change lands green.
- **Related**: ECS-2026-09-21-D5-01 (its re-implementation already scanned `get`/`has`, but its suggested fix dropped them).
- **Suggested Fix**:
  - Add `(WATER_SRC, "apply_buoyancy")` to physics_sync.
  - Add `(COMBAT_SRC, "queue_dead_actor_reconciliation")`, `(PHYSICS_SYNC_SRC, "set_kinematic_translation")`, `(PHYSICS_SYNC_SRC, "set_linear_velocity")` and `(WATER_SRC, "weather_wave_adjustment")` to player_controller.
  - Add `(ANIM_CONVERT_SRC, "build_subtree_name_map")` to animation.
  - Teach `acquired_in` the `.get::<` / `.get_mut::<` (write) / `.has::<` forms.

---

### CONC-D4-2026-09-28-02: `player_body_facing_system` writes in Late for a PostUpdate consumer; the one-frame body-yaw lag is avoidable, its justification is wrong, and nothing pins it
- **Severity**: LOW
- **Dimension**: Scheduler Access Declarations (cross-stage sequencing)
- **Location**: `byroredux/src/boot/schedule/late.rs:64-83`; `byroredux/src/player_body.rs:388-422` (doc + body)
- **Status**: NEW (a070baaad, today)
- **Description**:
  - **What it does.** The system writes the body root's `Transform.rotation` from `InputState.yaw`. The root is a child of the capsule (`player_body.rs:240-242`). Its only consumer is PostUpdate `transform_propagation`, which runs before Late. So the body is drawn with frame N-1's yaw, while `camera_follow_system` (Late, parallel) orbits the third-person boom with frame N's yaw (`character.rs:697-712`). On a fast turn, the body visibly trails the camera by one frame.
  - **Why the comment's reason fails.** The registration comment accepts this as "the same one-frame staleness every Late pose consumer here already accepts". That doesn't hold here. The other Late consumers sit in Late because their input, the post-Physics pose, only exists there (#3180/#3652). This system's only input is `InputState.yaw`, which is written between frames (`ui_input.rs:80`) and is final before the scheduler runs. No Physics-stage output is involved.
  - **Doc drift.** The fn doc (`player_body.rs:390`) says it "Runs beside `camera_follow_system` in the Late batch". It is registered exclusive, and would trip the analyzer (Transform WriteWrite) if someone "restored" it to the batch as the doc describes.
  - **No pin.** No test pins its stage or exclusivity. The four precedents (#3652, #3653, #4185, #4186) each have a pin test.
- **Evidence**: `late.rs:75-83` registers it as `add_exclusive_with_access(Stage::Late, …, .writes::<Transform>())`. `post_update.rs:11` is the propagation in the earlier stage. `grep player_body_facing` finds only its unit tests (`player_body.rs:672/686/702`) and the registration.
- **Trigger Conditions**: Third-person view (V key / `player.view third`) in Character mode, with mouse yaw changing between frames.
- **Impact**: The third-person body rotates one frame behind the camera (≈3° at 180°/s at 60 fps). This is cosmetic, but it is structural and permanent, and the registration comment wrongly calls it unavoidable.
- **Verification Path**: In third person, sweep the mouse and compare the body root's `GlobalTransform` rotation with `InputState.yaw` in the same frame via byro-dbg. Today they differ by one frame's delta.
- **Related**: #3652 (billboard, MEDIUM), #3653 (particle rate lag, pinned by `particle_emitter_rate_lag_is_structural`).
- **Suggested Fix**:
  - Register it as an Update exclusive, which also runs after the animation parallel batch, so the overwrite semantics are unchanged. PostUpdate propagation then composes it in the same frame.
  - Fix the fn doc.
  - Pin the placement (Update exclusive, before PostUpdate propagation) the way `billboard_runs_after_camera_follow_in_late` does.

---

### CONC-D5-2026-09-28-01: `container_loot_system`'s Access row does not declare the `PhysicsWorld` write that #4818 added to `pickup_loot`
- **Severity**: LOW. The system is exclusive, so no parallel pair exists today. This is promotion-baseline drift of the #4574 / #4821 class.
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `byroredux/src/boot/schedule/update.rs:169-190` (the declaration); `byroredux/src/inventory.rs:1108-1160` (`pickup_loot`, reached from `container_loot_system` at `inventory.rs:1026`); `byroredux/src/npc_spawn/loot_appearance.rs:129-180` (`collision_entities_of` and `remove_collision_bodies`)
- **Status**: NEW. It was introduced by 6c5555c70 (#4818, 2026-09-24), two days after #4574 (0f0287519) completed this declaration. `AUDIT_ECS_2026-09-28` Dim 5 did not catch it, because the #4573 mode-aware guard scans parallel systems only.
- **Description**: #4818 made `pickup_loot` remove the taken item's Rapier bodies:
  - `collision_entities_of` reads `FormIdComponent` and `PhysicsSourceForm`.
  - `remove_collision_bodies` reads `RapierHandles`, drops that guard, then takes `try_resource_mut::<PhysicsWorld>()`.

  None of these appears in the declaration. It lists PlayerNotifications, Dead, EquipmentSlots, EquippedWeapon, EquipmentEventBatch, PlayerEntity, InventoryCatalog, ActivateEvent, SceneAliasCandidate, Locked, PickedUp, Owned, FactionRanks, PlacedItemCount and Inventory.

  The same function also reaches undeclared `Children`/`MeshHandle` (through `mesh_entities_under`, `inventory.rs:1147`) and a `PersistentReferenceStates` write (through `mark_picked_up`, `reference_state.rs:337-360`). This makes it a second `PhysicsWorld` writer outside Stage::Physics that `sys.accesses` and the conflict analyzer cannot see.
- **Evidence**: `container_loot_system` → `pickup_loot` → `remove_collision_bodies`, which acquires in this order:
  1. `world.get::<FormIdComponent>(root)` (temporary guard)
  2. `query::<PhysicsSourceForm>` (collect, drop)
  3. `query::<RapierHandles>` (collect, explicit `drop`, `loot_appearance.rs:167-172`)
  4. `try_resource_mut::<PhysicsWorld>()` (`:173`)

  The guard order itself is correct: snapshot, then acquire, with no overlap.
- **Trigger Conditions**: Only if `container_loot_system` is promoted to the Update parallel batch, or a parallel Update system that reads `PhysicsWorld` is added. Update already has five parallel-adjacent `reads_resource::<PhysicsWorld>` declarations (`update.rs:125,203,237,257`). The analyzer would then report no conflict for a real `PhysicsWorld` write/read race.
- **Impact**: None at runtime today. The declaration is the only record of this write, so it is wrong exactly where a future promotion would rely on it.
- **Verification Path**: `grep -n "PhysicsWorld\|RapierHandles\|PhysicsSourceForm" byroredux/src/boot/schedule/update.rs` shows no hit inside the `container_loot_system` block (lines 169-190). `remove_collision_bodies` is at `loot_appearance.rs:160`.
- **Related**: #4574 (closed, same class), #4821 (open, same class on `npc_combat_ai`), #4818, #4983. Overlaps with the Dim 4 scheduler-declaration sweep; file once.
- **Suggested Fix**: Add these to the `container_loot_system` Access row:
  - `.writes_resource::<PhysicsWorld>()`
  - `.reads::<RapierHandles>()`, `.reads::<PhysicsSourceForm>()`, `.reads::<FormIdComponent>()`, `.reads::<Children>()`, `.reads::<MeshHandle>()`
  - `.writes_resource::<PersistentReferenceStates>()` (and `FormIdPool` if `identity` reads it)

  Then add the system to `p2_gameplay_exclusives_declare_non_empty_access`'s type-level assertions.

---

### CONC-D5-2026-09-28-02: Nothing pins that `register_newcomers`' rayon section runs with no World guard live and no `&World` inside the closure
- **Severity**: LOW (latent; no current trigger)
- **Dimension**: RwLock Patterns (Resource↔Storage, Physics)
- **Location**: `crates/physics/src/sync.rs:1005-1043` (`register_newcomers`; `into_par_iter` at 1035-1041, `resource_mut::<PhysicsWorld>` at 1043)
- **Status**: NEW. The pattern was introduced by ad1d53a11 (today).
- **Description**: The conversion itself is clean (see Verified clean §1). The closure captures only the owned `Newcomer` and `&cfg`, a `Copy` snapshot of `ContactConfig` taken at `:1010-1013` whose guard dies in `.map(|r| *r)`. No World guard is live at the `par_iter`.

  Two properties are load-bearing, though, and the lock-order detector cannot see either:
  1. **Cross-thread waits are invisible.** Pool workers run with an empty thread-local held set. A future closure that touched `world` (for example, reading `ContactConfig` per newcomer, or `ActorBoneCollider`), while the caller held a conflicting guard, would block across threads. This shape does not record an edge. It is exactly the blind spot #313 / O-1 describe.
  2. **The waiting thread can run other jobs.** `physics_sync_system` itself runs on a rayon worker (the scheduler's `par_iter_mut`, `scheduler.rs:500-503`). While it waits in `collect`, rayon may run another pool job on the same thread. Today Stage::Physics has one parallel entry, so there is nothing else to steal. If a system joins Stage::Physics and a guard is later hoisted above the `par_iter` (for example, moving `let mut pw = …` up), the stolen system could try to take that lock on the thread that already holds it, and `std` RwLock is not reentrant.

  Neither is true at HEAD. Only the ordering of the source lines stands between this code and both hazards.
- **Evidence**: Current order at `sync.rs:1010-1043`:
  1. `try_resource::<ContactConfig>().map(|r| *r)`: guard dropped
  2. `newcomers.into_par_iter().map(|n| collision_shape_to_parts(&n.shape, n.global.scale, &cfg)).collect()`: no guard live, no `world`
  3. `resource_mut::<PhysicsWorld>()`

  The same function is also reached from the main thread via `register_newcomers_and_refresh_queries`. Callers were checked at `scene.rs:1037`, `systems/character.rs:941` and `commands/view.rs:377`: none holds a guard at the call. The `if try_resource::<PhysicsWorld>().is_some()` at `view.rs:457-459` is a plain `if` condition, so its temporary drops before the body runs.
- **Trigger Conditions**: A future edit that moves any `world.*` acquisition into the closure, or hoists a guard above line 1035.
- **Impact**: Latent. It would be a cross-thread deadlock or self-deadlock that the `BYRO_LOCK_ORDER_CHECK` lane cannot catch.
- **Verification Path**: Read `sync.rs:1005-1043`.
- **Related**: #313 (tracker cannot see cross-thread ABBA), O-1, #2404 (snapshot-then-acquire discipline).
- **Suggested Fix**: Add a `source_scan` test next to `physics_diagnostics_resolve_forms_after_storage_guards_drop`. It should assert two things about the `into_par_iter` closure body in `register_newcomers`:
  - the closure body contains no `world`;
  - the `resource_mut::<PhysicsWorld>()` offset is greater than the `.collect()` offset that ends the parallel map.

---

### CONC-D6-2026-09-28-01: `GpuPerFrameTimers::new` leaks the earlier TIMESTAMP query pools when a later slot's `create_query_pool` fails, and the caller treats the error as non-fatal
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:629-651` (TIMESTAMP pool loop); caller `crates/renderer/src/vulkan/context/init.rs:658-667`
- **Status**: NEW. The code dates from e5774b19c (#1194, 2026-05-21), but no issue title covers it. #1483 and #1478 are about the allocator-None drop path and the hostQueryReset gate.
- **Description**: The TIMESTAMP loop fills `pools[i]` and exits through `.with_context(...)?` if a slot fails. When slot 1 fails, slot 0's `VkQueryPool` is already created but only exists in a local array, so no `Self` exists to destroy it. `init.rs:658-667` maps that `Err` to `None` and logs a warning. The engine then runs the whole session with a pool that nothing will ever destroy, and it is still alive at `vkDestroyDevice`. The fragment-invocation branch added in this window (`gpu_timers.rs:652-690`) handles the same partial failure correctly: it destroys every non-null candidate at `:676-683`. The two branches in the same function now use different cleanup policies.
- **Evidence**: `*slot = unsafe { device.create_query_pool(&info, None).with_context(|| format!("create TIMESTAMP query pool slot {i}"))? };` (`:636-640`). There is no cleanup before the `?`. Compare `for pool in candidate { if pool != vk::QueryPool::null() { unsafe { device.destroy_query_pool(pool, None) }; } }` (`:677-683`).
- **Trigger Conditions**: `vkCreateQueryPool` must return an error (OOM) for slot ≥ 1 after slot 0 succeeded. That is very unlikely on a desktop driver.
- **Impact**: One small leaked `VkQueryPool` (56 queries), and a validation error at shutdown. There is no memory-safety hazard.
- **Verification Path**: Inject a failure on slot 1 (for example, a test-only hook), run with `BYRO_VALIDATION=1`, and expect `VUID-vkDestroyDevice-device-05137` naming a `VkQueryPool`.
- **Related**: #1483, #1478, #4891 (inconsistent failure-path policy across subsystems).
- **Suggested Fix**: Before propagating the error, destroy every non-null entry in `pools`, using the same shape as the fragment-invocation branch. Alternatively, build `Self` first and let `destroy()` handle the partial state.

---

### CONC-D7-2026-09-28-01: On Windows, "lock-free positional reads" still serialise in the kernel for each archive handle
- **Severity**: LOW (doc accuracy + perf on a secondary platform; no correctness defect)
- **Dimension**: Worker Threads
- **Location**: `crates/bsa/src/read_at.rs:1-7` (module claim), `:24-48` (Windows impl); the claim is repeated at `byroredux/src/streaming.rs:669-672`, `:1557-1558` and `:1866-1869`, and in `docs/engine/archives.md:360-361,466`
- **Status**: NEW
- **Description**: The module doc says positional reads let "concurrent extracts from one archive need no lock and never wait on each other". That holds on Unix (`pread`). On Windows, `seek_read` calls `ReadFile` with an `OVERLAPPED` offset. `File::open` returns a *synchronous* handle (no `FILE_FLAG_OVERLAPPED`), and the NT I/O manager serialises every I/O request on a synchronous file object through its file-object lock. So:
  - **Correctness holds.** Each call carries its own offset, the cursor side effect is inert (no cursor read happens after `open`; see `archive/open.rs:445-452`, `ba2.rs:367`, `csg.rs:152-189`), and the short-read loop handles partial reads.
  - **The reads do wait on each other.** Only the syscall is serialised. Inflate still runs in parallel, so #3659's goal survives.
- **Evidence**: `read_at.rs:27-28` (the comment covers the cursor side effect but not the serialisation). I cross-compiled with `cargo check -p byroredux-bsa --target x86_64-pc-windows-gnu`: it compiles cleanly. There is no Windows CI (`.github/workflows/*` are all self-hosted Linux), so the Windows loop has never run.
- **Trigger Conditions**: A Windows build streaming exterior cells, where the stream-pool tasks and main-thread texture resolves hit one archive handle.
- **Impact**: Per-archive read throughput on Windows is serial. The docs overstate the win, which could mislead a future perf investigation.
- **Verification Path**: Profile a Windows build with more than 8 fresh NIFs per cell, or ask for one handle per thread and compare.
- **Related**: #3659, #360, #1170
- **Suggested Fix**: Qualify the doc ("need no user-space lock; Windows synchronous handles still serialise the syscall"). If Windows throughput ever matters, open the handle with `FILE_FLAG_OVERLAPPED` through `OpenOptionsExt::custom_flags` and wait per call.

---

### CONC-D7-2026-09-28-02: BSA and CSG went lock-free with no multi-threaded extract regression; only BA2 has one
- **Severity**: LOW (test gap)
- **Dimension**: Worker Threads
- **Location**: `crates/bsa/src/archive/extract.rs:36-66,106` (BSA); `crates/bsa/src/csg.rs:309-367` (CSG `chunk_bytes`); the only concurrent test is `crates/bsa/src/ba2.rs:1201-1266` (`concurrent_extracts_read_their_own_entries`)
- **Status**: NEW
- **Description**:
  - **What changed.** `1b8b21f3f` removed `Mutex<File>` from all three readers, and only BA2 gained a thread-scoped extract test. The BSA reader serves the streaming worker's pool tasks for 4 of 5 games (Oblivion/FO3/FNV/Skyrim). It also serves main-thread texture resolves and the new prefetch tasks, all on one handle. The CSG reader is now hit by concurrent precombine-decode tasks (`e593770f0`, `streaming.rs:1348-1356`, `precombined.rs` `CsgHandleCache`).
  - **The hazard is future regression, not current code.** `impl Read for &File` and `impl Seek for &File` both exist. A later edit such as `(&self.file).seek(..)` followed by `read_exact` compiles without `&mut` and without a lock, and silently reintroduces the shared-cursor race the Mutex used to prevent. Nothing on the BSA or CSG side would catch it.
- **Evidence**: `git show 1b8b21f3f -- crates/bsa/src/archive/tests.rs crates/bsa/src/csg.rs` adds no threaded test. `grep thread::scope crates/bsa/src` returns only `ba2.rs:1250`.
- **Trigger Conditions**: A future edit to BSA or CSG extract.
- **Impact**: Silent wrong-bytes corruption: NIF parse failures, or wrong precombine geometry, that shows only under parallel streaming.
- **Verification Path**: Add 8 threads × N rounds that extract from a synthetic compressed + embed-name BSA and a multi-chunk CSG, and assert byte equality. The BA2 test is the template.
- **Related**: #3659, #1170, #877
- **Suggested Fix**: Clone the BA2 `concurrent_extracts_read_their_own_entries` shape for `BsaArchive` (compressed + `embed_file_names`) and for `CsgArchive::read_psg` across chunk boundaries.

## Existing issues re-confirmed at HEAD (not re-filed)

- **#4780** (OPEN): the volumetrics skip-clear latch still skips the temporal reset. The code is unchanged since 09-23. Dim 2's re-verification follows.
- **#4881** (OPEN): `StagingPool` capacity labels decay on reuse. The #4593/#4790 release-size fixes are in place, but this is not fixed (Dim 6).
- **#4599** (OPEN): the allocator lock `.expect/unwrap` is reachable from teardown and `Drop`. The window adds new instances: `texture.rs:528` (e26441c34), and `context/helpers.rs:50` `destroy_staging_buffer`, which Drop reaches through `teardown.rs:331-332` (Dim 6).
- **#4890 / #4891** (OPEN): resize-path `?` windows and inconsistent rollback. New examples are `restir.rs:191-203` and the TAA partial failure at `taa.rs:854-860`. Both are reachable only after the resize already returned `Err`, and the `framebuffers.is_empty()` guard at `draw.rs:2110-2112` stops `draw_frame` (Dim 6).
- **#4889 / #4892** (OPEN): a dynamic-RGBA staging failure is fatal, and there is no ledger row for it. The lifecycle itself is clean (Dims 1, 6).
- **#4821** (OPEN): the sibling of CONC-D5-2026-09-28-01, an exclusive-system Access row missing pairs.

### CONC-D2-2026-09-28-01: The #3685 skip-clear latch still skips the temporal reset, so a skip on a latched slot leaves `history_valid` true
- **Severity**: MEDIUM. Visual temporal corruption: stale fog and combustion history. No GPU safety issue.
- **Dimension**: Compute → AS → Fragment Chains (cross-frame ping-pong)
- **Location**:
  - `crates/renderer/src/vulkan/context/post_passes.rs:620-626` (the `ran = false` arm) and `:840-859` (the `Err` arm reports `true`).
  - `post_passes.rs:865-870`: the latch. `record_neutral_frame` runs only when `should_clear`.
  - `post_passes.rs:1478-1492` (`skip_clear_decision`).
  - `crates/renderer/src/vulkan/volumetrics.rs:2106-2142`: `record_neutral_frame`. This is the only reset site on the skip path.
  - `volumetrics.rs:1557` (`prev_camera_pos.w` ← `history_valid`) and `:1874-1882` (`mark_frame_completed`, a no-op without a dispatch).
- **Status**: Existing: #4780 (OPEN). The code is unchanged since the 09-23 report, and no fix has landed.
- **Description**: `history_valid` is cleared only inside `record_neutral_frame` or `signal_history_reset`. A skip on a slot whose latch is already set clears nothing and resets nothing. In the sequence *dispatch(x) → skip(y, latched) → dispatch(x)*, the inject reprojects slot y's froxel, emission and combustion history, which is stale by an arbitrary number of frames. The secondary trigger is also still present: on the `Err` arm, `ran = true` and `history_valid` keeps its prior value.
- **Evidence**: `skip_clear_decision(false, true)` returns `(false, true)`, and nothing then touches `vol` (`post_passes.rs:865-870`).
- **Trigger Conditions**: `requires_dispatch` or input availability flickers so that one dispatch lands between two skips on a slot that is already latched. For example, `local_emitters_present` toggling the interior dust coefficient (`post_passes.rs:602-608`).
- **Impact**: Ghosted fog and resurrected combustion transport for about 12 or more frames at the 0.92 history weight.
- **Verification Path**: A `cargo test` unit test on the latch plus `mark_frame_completed` sequence. No RenderDoc capture is needed.
- **Related**: #3685, #2507.
- **Suggested Fix**: Split `record_neutral_frame`'s CPU temporal reset (the `:2135-2141` fields) from its latched GPU clear. Run the reset on every `ran == false` frame and on the `Err` arm.

## Closure candidates (fixed in code, issue still OPEN)

- **#4866**: the ground-cover model tier now sits in a GPU timer bracket (`groundcover_models.rs:735`, `:810`). Found by Dim 2.
- **#4868**: `timestampValidBits` is now applied (`gpu_timers.rs:471-485`). Found by Dim 2.
- **#4739**: the combat feedback system now uses `byroredux_audio::SoundCache` (`combat_anim.rs:382-399`, 546366364). Found by Dim 3; route to `/audit-audio` to confirm.

## Routed (not concurrency; not filed here)

- **Failing test on `main` (a070baaad).** `cli_args::tests::renderer_config_defaults_to_fsr_quality` is red. `RendererConfig::default()` has `auto_exposure: true`, while the bare-CLI parse has `auto_exposure: false`. Owner is `/audit-tooling`, or a quick fix.
- **Newcomer-registration cache defeated by ragdolls.** The cache from 88c23887b never latches "fully registered" once a ragdoll is active. `activate_ragdoll` (`byroredux/src/ragdoll.rs:441-470`) strips `RapierHandles`/`RigidBodyData` but leaves `CollisionShape`, so `collect_newcomers` (`crates/physics/src/sync.rs:962-968`) falls back to a full shape scan every tick. Owners are `/audit-physics` and `/audit-performance` (Dim 5).
- **Dim 1 observations** (not filed):
  - The `hud.rs:722` comment still names the retired texture rotation as the safety mechanism.
  - `Texture::overwrite_rgba_pixels` (`texture.rs:137-160`) has no production caller.
  - Six `device_wait_idle` calls bypass the queue mutex; that is fine while all Vulkan work stays on the main thread.
  - A zero-instance TLAS frame (`tlas.rs:204`) depends on chained barriers. It is a validation lead once the lane works.
  - The blit-restore barrier (`frame_upscaler.rs:836-844`) waits only on FRAGMENT, and its next-frame COMPUTE reader is covered only by an indirect chain (Dim 2).

## Coverage note

Dim 7's spawn census found three parallel sections added in this window that the skill's dispatch lists do not name:
- `crates/physics/src/sync.rs:1036` (ad1d53a11). Covered by Dim 5.
- `byroredux/src/render/mod.rs:1100-1164` nested `rayon::join` (de808add3). The orchestrator verified it: every task closure takes read guards only. The write-guard sites in `render/mod.rs` (`:92`, `:173`, `:184`, `:342`, `:405`) sit outside the joins. The joins run on the main thread after `scheduler.run` returns, so no ECS writer is live.
- `byroredux/src/cell_loader/load_order.rs:603,670` `in_place_scope` (382fa9296). Covered by Dim 7.

A future skill sync should add these to Dims 3, 5 and 7's `Paths:`.

---

## Per-dimension verified-clean appendix

### Dim 1 — Vulkan Queue & AS Sync (45 commits)

#### Verified clean (with evidence)

- **Queue submission is serialized.**
  - `graphics_queue` and `present_queue` are `Arc<Mutex<vk::Queue>>`; present is an `Arc::clone` when the families match (`context/init.rs:122-128`).
  - Every `queue_submit`, `queue_present` and `queue_wait_idle` in renderer, byroredux and crates/ui is listed below. crates/ui uses wgpu's own device and has no ash queue calls.
    - `context/draw.rs:2541-2548`: guard bound, deref inside the call; `drop(queue)` before recovery on the error arm.
    - `context/draw.rs:2670-2679`: present guard `pq` held across `queue_present`.
    - `texture.rs:1017-1020`: guard scoped to the submit and released before `wait_for_fences`. Pinned by `one_time_lock_scope_tests::queue_guard_released_before_one_time_fence_wait` (`texture.rs:1078`), live.
    - `egui_pass.rs:250-255`: the guard spans egui's internal `queue_submit` + `queue_wait_idle`, which is required because `queue_wait_idle` also needs external sync.
    - `sky_cube/filter/tests.rs:272`: test only.
  - No per-frame blocking one-time submit remains in the frame path except egui `set_textures`. The HUD and Ruffle overlay moved in-frame (next bullet).
- **Dynamic RGBA path is sound (e2f99ad55 / #3429).** `write_rgba_inplace` and `update_rgba` now queue into `DynamicRgbaUploads`, recorded in the frame's own command buffer by `record_pending_rgba_uploads` (`texture_registry/dynamic_rgba.rs:83-214`, called at `begin_frame_recording.rs:81-96`).
  - Staging is per FIF slot (`staging: [Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT]`, `:21`). Its grow-time destroy is gated by `fence_confirmed_idle_slot == Some(frame)` (`:90-93`), which `begin_frame` sets after the fence wait (`sync_and_acquire_frame.rs:240`, `texture_registry/mod.rs:679-689`).
  - The image WAR against earlier frames is a device edge: `ALL_COMMANDS, MEMORY_READ|MEMORY_WRITE → TRANSFER` (`:184-192`). Publication is `TRANSFER → ALL_COMMANDS SHADER_READ` (`:200-208`).
  - Only a successful submit consumes the pixels (`note_frame_submitted` at `draw.rs:2587` → `submitted(slot)`). Release drops pending updates (`release.rs:44`).
  - `sync.rs` rider 6 says the HUD rotation no longer depends on the all-slots wait. That is correct: `hud.rs` `texture_handles: [u32; 3]` is no longer hazard protection.
  - Pinned by `rgba_updates_use_the_frame_command_buffer_and_keep_descriptors` and by the `staging: [Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT]` assert in the #3643 test.
- **Frame-in-flight guards are live.**
  - The const-assert `MAX_FRAMES_IN_FLIGHT == 2` is at `sync.rs:121-131`.
  - These tests are live, and none of the files in scope contain `#[ignore]`:
    - `frames_in_flight_contract_names_every_dependent_resource` (`sync.rs:634`)
    - `the_all_slots_wait_argument_is_pinned` (`sync.rs:750`, #4601), which asserts `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` (present at `sync_and_acquire_frame.rs:81`) and rejects the per-slot spelling
    - `render_finished_is_sized_and_indexed_per_swapchain_image` (`sync.rs:548`)
  - Semaphore indexing: `image_available[frame]` is the wait semaphore (`draw.rs:2480`) and `render_finished[img]` the signal (`:2494`). The new `record_pending_rgba_uploads` error arm recreates `image_available[frame]` (`begin_frame_recording.rs:89-95`).
- **"Overlap cell preparation" (5eb07a4f3) touches no GPU resource.** The `byroredux/src/streaming.rs` `parse_nif_pipeline` / `ParseInputBudget` is CPU parsing on the private rayon pool. Its caller is the dedicated `cell_pre_parse_worker` std::thread (`streaming.rs:879-881`), not a pool worker, so the Condvar backpressure cannot starve the pool. No Vulkan handle is touched.
- **The TLAS refit/rebuild decision is correct.**
  - `decide_use_update` (`acceleration/predicates.rs:275-320`) returns UPDATE only when all of the following hold: the list is not empty, `needs_full_rebuild` is false, the BLAS map generation is unchanged, the addresses in canonical `(address, EntityId)` order are equal, and the entity membership is equal.
  - It is followed by the `instance_count != built_primitive_count` guard (VUID-03708, `tlas.rs:133`) and the UPDATE debug assert.
  - Bookkeeping, including `last_entity_ids` refreshed only on BUILD, is committed after `cmd_build_acceleration_structures` records (`tlas.rs:360-420`).
  - `invalidate_tlas_recording` forces BUILD after an unsubmitted recording (`tlas.rs:1166-1170`, reached from `rollback_skin_frame_state` at `skinned_blas_refit.rs:54-66`). Any `draw_frame` `Err` also exits the app (`app_frame.rs:715-718`).
  - Scratch is sized for `padded_count` (`tlas.rs:906-907, 972-977`), so a later BUILD at up to `max_instances` fits even after `shrink_tlas_scratch_to_fit`.
  - Flag parity: `UPDATABLE_AS_FLAGS` is used at both the size query and the build.
- **AS barriers are in place.**
  - The frame-scope `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ` barrier before `build_tlas` is unconditional (`dispatch_skin_and_cluster.rs:306-315`, #4179).
  - The TLAS publish `AS_BUILD → FRAGMENT|COMPUTE AS_READ` runs on both arms (`:404-412`, #2931).
  - The instance input uses `TRANSFER_WRITE → SHADER_READ @ AS_BUILD`, not AS_READ (`tlas.rs:243-258`, #1436/#507945d8).
  - The skinned vertex input uses `COMPUTE SHADER_WRITE → AS_BUILD|FRAGMENT|COMPUTE SHADER_READ` (`skinned_blas_refit.rs:641-650`), pinned by `every_compute_consumer_of_the_skinned_vertex_ssbo_is_in_the_publish_dst_mask`.
  - Refit publish is at `:834-841`. `record_scratch_serialize_barrier` uses dst `AS_WRITE|AS_READ` (`blas_skinned.rs:712-728`) and is used before the static batch's first build (`blas_static.rs:760-764`, #4177).
  - On the WAR side, the TLAS instance-buffer copy chains from the previous use of the slot as `AS_BUILD → [publish] → FRAGMENT → [#4602 TRANSFER|FRAGMENT→HOST, draw.rs:2443-2450] → HOST → [HOST→TRANSFER, tlas.rs:204-222] → TRANSFER`.
- **The #4779 fix is in place (09-23 CRITICAL CONC-D1-2026-09-23-01, now issue #4779, closed).**
  - `ray_query_tlas` filters on `tlas_build_succeeded_last_frame` (`dispatch_skin_and_cluster.rs:575-580`). Volumetrics (`post_passes.rs:573`), ground-cover scatter (`:515`) and ground-cover models (`:536`) use it.
  - Caustic (`post_passes.rs:456-459`) and water or fragment consumers use the raw handle but early-out on `sceneFlags.x`, which the failure arm patches to 0 (`caustic_splat.comp:323`, `water.frag:423/614`, `triangle.frag:964`).
  - Pinned by `stale_tlas_compute_gate_tests::compute_ray_query_passes_take_the_build_gated_tlas` (`:896`).
- **Deferred destruction holds.**
  - BLAS eviction and drop go through `pending_destroy_blas`; scratch retirement goes through `pending_destroy_scratch` (`blas_static.rs:120-145, 681-682`; `memory.rs:84-110`). The tick runs after the fence wait (`sync_and_acquire_frame.rs:247`).
  - `StaticBlasWorkingSet` (88c23887b / the 5eb07a4f3 area) only narrows `can_evict`, so eviction stays deferred.
  - Mid-batch `destroy_acceleration_structure` sites at `blas_static.rs:632/839/997-1081` are unchanged since baseline and are batch-local.
  - The #4767 move (`context/shrink_frame_scratch.rs`) is CPU Vecs only.
  - The tail TLAS shrink targets the *next* slot (`draw.rs:2714-2737`), covered by rider 7. `shrink_tlas_to_fit` only records intent (#2929).
  - #4833: `grow_instance_ssbos` now runs in `begin_frame_recording` (`:183`). Old buffers retire through `retired_instance_buffers` with `DEFAULT_COUNTDOWN` (`scene_buffer/upload.rs:1286-1289`), and the caustic rebind is for this slot only.
  - `ensure_tlas_state`'s immediate destroy of the old slot sits behind `device_wait_idle` (`tlas.rs:1079-1098`, #1390 belt-and-braces) plus allocate-then-swap (#2673).
- **Swapchain recreate.** `recreate_swapchain_core` calls `device_wait_idle` first (`context/resize.rs:57-60`). The geometry-SSBO low-headroom path at `mesh/geometry_ssbo.rs:865` calls `device_wait_idle` before its immediate destroys and runs between frames (`app_frame.rs:348`). Descriptors 8/9 are re-pointed per slot before use (`sync_and_acquire_frame.rs`, after the tick).
- **Precombine split (078f650ec).** It reuses the existing `mesh_registry` upload and `build_blas_batched` paths, which are synchronous and fence-waited, and runs between frames. It adds no new staging or host-visible buffer. The renderer side is GPU-timer brackets only; `active_bits` is set on end and there is no WAIT flag.

#### Observations (not filed)

- **Stale doc.** `byroredux/src/hud.rs:722` (`upload_frame`) still says "The overwritten buffer was last sampled three frames ago, so no in-flight frame still reads it". Since #3429 the registry's in-frame barriers are the protection, as the field doc at `:308-311` already says. This is doc rot for `/audit-tech-debt`.
- **Dead public API.** `Texture::overwrite_rgba_pixels` (`texture.rs:137-160`) has no production caller left. It performs a blocking submit plus fence wait, and its "Hazard contract" still cites the HUD rotation.
- **Latent contract.** Six `device_wait_idle` sites run without taking the queue Mutex: `context/mod.rs:1250`, `resize.rs:60`, `resize.rs:1429`, `teardown.rs:259`, `tlas.rs:1083` and `geometry_ssbo.rs:865`. VUID-vkDeviceWaitIdle requires external sync of every queue. All Vulkan queue use is on the main thread today, so this is not a race. It becomes one if the "future second graphics-queue thread" that `texture.rs:1011-1015` anticipates is ever added.
- **Validation lead.** On an empty-instance TLAS frame (`copy_size == 0`, `tlas.rs:204`), the `HOST→TRANSFER→AS_BUILD` pair is skipped. The device-side WAR edge from the slot's previous ray-query reads to the new BUILD then relies on other barriers happening to chain. The 09-27 live runs never covered a non-empty → empty transition, such as unloading into an empty scene or the `--menu` route. If `BYRO_VALIDATION=1` reports `SYNC-HAZARD-WRITE-AFTER-READ` on the TLAS buffer there, widen the #4179 frame-scope barrier's source to include `FRAGMENT|COMPUTE`. Host-side it is safe because of the all-slots wait.
- **Degraded-path gap.** If `patch_camera_rt_flag(.., 0.0)` returns `Err` on the build-failure arm (`dispatch_skin_and_cluster.rs:359-364`, which only warns), `rt_flag` stays 1.0 and the fragment and caustic passes would trace the stale TLAS. That is the #4779 class. It is reachable only through a failed mapped write or flush on the persistently mapped camera UBO, which is effectively the device-lost regime.

### Dim 2 — Compute → AS → Fragment Chains (38 commits)

#### Verified clean

- **Skin chain (M29).**
  - The palette publish barrier is COMPUTE→COMPUTE|VERTEX|FRAGMENT (`dispatch_skin_and_cluster.rs:270-288`, #4853).
  - The skin-output publish dst is AS_BUILD|FRAGMENT|COMPUTE with SHADER_READ (`skinned_blas_refit.rs:641-650`).
  - The unconditional AS_WRITE→AS_READ before `build_tlas` is in place (`dispatch_skin_and_cluster.rs:347-358`, #4179). So is the AS_BUILD→FRAGMENT|COMPUTE barrier on both build arms (`:432-440`, #2931).
  - #4829: the boneWorld descriptor now covers the whole buffer, and the cache key includes the ranges (`skin_compute.rs:1216-1258`).
  - #4611: the palette plan uses the persistent scratch (`dispatch_skin_and_cluster.rs:220-232`, `:290`).
- **#3582 consumer guard.**
  - `skin_publish_barrier_consumer_tests` (`skinned_blas_refit.rs:1016-1073`) still discovers every `shaders/*.comp`.
  - The new `.comp` files are `groundcover_models`, `groundcover_interaction`, `exposure_meter` and the bench shaders. Only `groundcover_models.comp` names `skinnedVertexAddress`, and it only writes `0ul` (`:462`), so it is not a consumer. The test's token check would still count it, which is harmless.
  - The only real compute dereference is still in `caustic_splat.comp:222`.
  - No `.comp` includes `include/ray_hit.glsl`, so no dereference can hide behind an include.
- **Ground-cover model tier (aabd99a05, #4413).**
  - Recorded after the instance upload and before the geometry pass (`draw.rs:2265`).
  - Traces the gated `ray_query_tlas` (`dispatch_skin_and_cluster.rs:537`, #4779), after the AS publish.
  - Its leading WAR barrier on the shared slab, counts and draws is DRAW_INDIRECT|COMPUTE|TRANSFER → COMPUTE (`groundcover_models.rs:744-755`).
  - Between PLACE, LAYOUT and EMIT there are COMPUTE→COMPUTE edges.
  - After EMIT there is a COMPUTE → DRAW_INDIRECT|VERTEX|FRAGMENT|TRANSFER edge with INDIRECT_COMMAND_READ|SHADER_READ|TRANSFER_READ (`:779-799`). That covers the `cmd_draw_indexed_indirect`, the instance and previous-model tail reads, and the stats copy.
  - The stats copy is covered by the #4602 tail edge (TRANSFER src, `draw.rs:2443-2450`).
  - `harvest` reads `stats_readback[frame]` only behind `pending_stats` and relies on the previous `draw_frame`'s all-slots wait (#4851 rider, `:486-491`). Per-FIF record, table and shape buffers are host-written after that wait.
  - Compute readers of the instance tail are ordered by the main render pass's outgoing dependency (dst FRAGMENT|COMPUTE with SHADER_READ, `helpers.rs:381-384`, chained through EMIT's VERTEX/FRAGMENT dst). Volumetrics also has its explicit COMPUTE→COMPUTE barrier (`post_passes.rs:699-706`).
  - The TLAS never references the tail.
- **Ground-cover scatter.**
  - #4293 TRANSFER→TRANSFER seed ordering is at `groundcover.rs:1541-1557`.
  - The #4181 publish includes TRANSFER (`:1645-1656`).
  - The interaction field has its leading and trailing COMPUTE edges (`:1719-1753`).
- **Frame-tail order.**
  - The order is SVGF → caustic → volumetrics → SSAO → composite → bloom → exposure meter → TAA → upscale → presentation (`post_passes.rs:294-351`).
  - This is pinned by `taa_resolves_the_post_bloom_scene_tap` (`:1966-2018`), which includes the #4591 raw-output gate assertions (#4841).
  - Bloom `apply_to_scene` publishes to COMPUTE|FRAGMENT|TRANSFER (`bloom.rs:933-943`). The meter reads the post-bloom scene and publishes the exposure texel to COMPUTE|FRAGMENT (`exposure_meter.rs:274-306`).
  - The TAA output slot enters the blit from GENERAL with a COMPUTE src and is restored to GENERAL (`frame_upscaler.rs:735-777`, `:817-832`).
  - #4592/#4976: all three `record_native_blit` calls pass `inputs.scene_color_layout`. This is pinned by `every_native_blit_sources_from_the_scene_images_actual_layout`.
  - `taa_resolved` (`post_passes.rs:1265-1270`) uses the same predicate as `record_taa_pass`.
  - *Observation, not filed.* The blit's restore barrier has dst `FRAGMENT_SHADER` only (`frame_upscaler.rs:836-844`). Yet the restored GENERAL slot's only consumers are the next frame's TAA compute read and write, and the TAA post-barrier comment (#653) insists on both stages. It is covered today only by an indirect chain: the #4602 HOST edge → the next frame's HOST→COMPUTE global barriers (`dispatch_skin_and_cluster.rs:478-485`) → the exposure-meter and bloom FRAGMENT→COMPUTE edges. If this is ever narrowed, add COMPUTE to that dst.
- **TAA reactive mask (#4944).** It samples attachment 6, which is always CLEAR/STORE with final layout SHADER_READ_ONLY (`helpers.rs:262-282`). The outgoing dependency covers the COMPUTE read, and views are rewired on resize (`resize.rs:605`).
- **ReSTIR (186234944).**
  - The per-frame curr clear has barriers on both sides (`restir.rs:122-175`): prior read/write → TRANSFER, fill, then TRANSFER → FRAGMENT SHADER_WRITE.
  - prev gets a FRAGMENT|TRANSFER write → FRAGMENT read edge.
  - Ping-pong uses `(f+N-1)%N` (`:110-112`), and `light_history` is per-FIF.
  - Only `triangle.frag`, including the `triangle_early.frag.spv` variant, touches `reservoirsCurr` and `reservoirsPrev`.
- **Opaque early-depth.** It is a pipeline variant (`PipelineKey::Opaque{early_tests}`, `geometry_pass.rs:313-320`), not a prepass. The render pass, depth attachment, load op and barriers are unchanged.
- **Volumetrics.**
  - Stage B history pairs, Stage D (with TRANSFER in the source scope, #3647), Stage F COMPUTE→FRAGMENT and the COMPUTE→HOST moment edge are all intact (`volumetrics.rs:1722-1860`).
  - The `tlas_written`, `lights_written` and `boundary_geometry_written` latches are asserted and reset symmetrically (`:1530-1550`) and set in the writers (`:2177`, `:2233`, `:2278`).
  - The TLAS comes from `ray_query_tlas` (`post_passes.rs:573`, #4779).
  - `VOLUMETRIC_OUTPUT_CONSUMED` gates the pass (`:577`).
  - The new inject and integrate timer brackets are nested inside the dispatch and do not reorder any barrier.
- **Sky-cube bake.** It runs mid-frame before the geometry pass (`build_and_upload_instances.rs:1029-1056`). The UNDEFINED→GENERAL transition runs from a FRAGMENT src to COMPUTE, and GENERAL→SHADER_READ runs to FRAGMENT|COMPUTE (`sky_cube.rs:517-545`). 0572bfd5a changed only the parameters.
- **Caustic, water-caustic, SVGF and bloom.** There are zero barrier-line changes since f97775ca8 (`git diff` filtered on stage, access, layout and fill/copy/clear/blit tokens).
- **GPU timers (88c23887b / 39c6f0aff).**
  - There are per-FIF TIMESTAMP pools and optional pipeline-statistics pools.
  - Both are reset host-side (`hostQueryReset`-gated, `gpu_timers.rs:620-650`, `:669`) in `read_and_reset` after the all-slots fence (`sync_and_acquire_frame.rs:33-36` → `:150`). No `vkCmdResetQueryPool` is recorded, and the queries are read without WAIT, gated on `active_bits`.
  - Query indices 0-45 are unique, and the geometry phases occupy 46-55 (`QUERIES_PER_FRAME = 56`).
  - Every start and begin is written at most once per frame: the phase latches are at `geometry_pass.rs:249-300` and `:571`, the draw is latched at `:282`, and `skin_palette_timer_started` guards its bracket.
  - The pipeline-statistics query begins and ends inside the same subpass.
- **Stale-open issues (fixed in code, still OPEN).**
  - #4866: the model tier is bracketed at `groundcover_models.rs:735`/`:810`, pinned by `model_timer_encloses_all_phases_and_stats_copy`.
  - #4868: the `timestampValidBits` mask is applied at `gpu_timers.rs:471-485`.
- **#4602 host flush.** It is the last command before `end_command_buffer` (`draw.rs:2443-2450`, pinned at `:3350`). The new readbacks are the model-tier stats (a TRANSFER copy, covered) and the RT-LOD telemetry (written only by `triangle.frag`, so a FRAGMENT write, covered). Volumetrics keeps its own COMPUTE→HOST edge.
- **MaterialBuffer.** It is still a host-mapped write in `build_and_upload_instances.rs:789`, before submit. It did not move into a compute path.

### Dim 3 — ECS Lock Ordering (36 commits)

#### Verified clean

**Static proof (parallel batch).**
- `install_runtime_registries` has release `assert_eq!(…, 0)` on `undeclared_parallel_count`,
  `known_conflict_count` and `unknown_pair_count` (`byroredux/src/boot/registries.rs:26-49`).
- `build_scheduler_reports_zero_access_conflicts` (`boot/schedule/mod.rs:419`) and
  `scheduler_access_invariants_hold_on_the_real_schedule` (`scheduler_access_tests.rs:266`) are
  present, not ignored, and passed (orchestrator run).
- Parallel set at HEAD:
  - Early: `player_controller_system`, `timer_tick_system`
  - Physics: `physics_sync_system`
  - Update: animation
  - PostUpdate: propagation
  - Late: `camera_follow`, `reverb_zone`, `log_stats`, `metrics_sample`
- The only delta change to a parallel body is `camera_follow_system`'s new `PlayerCameraView`
  read (`systems/character.rs:703-707`). It is declared (`late.rs:56-57`), consumed by `.map`,
  and nothing else is held at that point. Its only writer, `set_player_view`, never runs inside
  the batch.
- The new `player_body_facing_system` is **exclusive** Late (`late.rs:74-83`). Its body
  (`player_body.rs:395-423`) takes one guard at a time: `PlayerMode`, `PlayerBodyRootEntity`,
  `PlayerBodyRoot`, `InputState` (dropped), then `Transform` (W).
- `log_stats_system`'s new `SchedulerSystemTimings` read (`systems/debug.rs:228-231`, held under
  `DebugStats`) is declared (`late.rs:278`). The scheduler writes it only after all stages
  (`crates/core/src/ecs/scheduler.rs:521-527`), single-threaded.

**#4605 fixes hold.**
- `combat_feedback_system_inner` copies `DraugrCombatClips` via `.map(|c| *c)`
  (`systems/combat_anim.rs:100-105`).
- `npc_combat_ai_system_inner` resolves `attack_reach_bu` / `WalkSpeed` / `attack_damage` /
  `attack_cooldown_seconds` in `resolve_pending_strikes` with no storage guard live
  (`systems/combat_ai.rs:84-157` gather scope, then `:163-229`).
- `PhysicsWorld` is taken only around `step_toward` (`:239-247`, #4325).
- #4613 scratch (`FeedbackScratch`, `CombatAiScratch`, `HostilityScratch`, ground-cover
  `GroundCoverCollectScratch`) is closure- or App-owned, not a Resource. So there is no
  Resource↔Storage pair. `InteractionCandidateScratch`'s take/return is guard-free
  (`interaction.rs:868-870`, `:917`, `:1187-1190`).

**#4982 / #4983 / #4984 fixes hold.**
- `reference_state::capture` runs one guard per pass, with `FormIdPool` scoped and
  `ItemInstancePool` after the `Inventory` guard (`cell_loader/reference_state.rs:109-170`).
- The `PickedUp` walk-first rule is documented at `ecs.md:618-623`.

**Changed system bodies traced (guard lifetime, reentrancy, order).**
- `combat_anim.rs:94-358` + `play_oneshot_cached :382-414` (546366364, 2f8538334, 3978b5184):
  - Read pass: `DraugrCombatAnim` → {`Dead`, `AnimationTarget`, `RagdollActive`,
    `AnimationPlayer`} reads.
  - Write passes are sequential.
  - Sound runs after all writes: `SoundCache` (W) → `SoundArchiveProvider` (R) inside
    `get_or_load`, consumed, then `AudioWorld` (W).
  - No reverse edge. `SoundCache`'s other reader is `ownership_sample.rs:67` (leaf read).
    `RagdollActive`'s writer is `ragdoll.rs:424`, with no `DraugrCombatAnim` held.
- `combat_ai.rs` (a41202008, 327d4bddd, 6c517bc7a): `suspend_ambient_behavior_for_combat` /
  `clear_ambient_behavior` (`npc_spawn/ai_package.rs:436-523`) is single-guard sequential. The
  `if let … .map(|s| *s)` scrutinee releases its guard in the closure (edition 2021 checked).
- `faction_hostility.rs:211-391` (#4414): one storage at a time. `LoadOrderIdentity` is held only
  over `form_ref` (pure). `PhysicsWorld` is taken per cast with no storage held.
  `faction_relationships()` is an `Arc` clone.
- `combat.rs` (d8b849b04, 34b53c464): `player_can_act` (`systems/character.rs:100-105`) consumes
  its guards. At each call site (`combat.rs:139-143`, `interaction.rs:1397`,
  `inventory.rs:1035/1451`) the preceding guard is consumed first.
  `combat_damage_system :313-366` is sequential.
- `extensions/commands.rs:484-520` `commit_actor_value_deaths` (#4702):
  - Records `ActorVitals → ActorValues → Dead` (reads), scoped. Then `Dead` (W) and
    `PendingDeathReconciliations` (W).
  - Consistent with `water_damage_system` (`systems/water.rs:20-63`: `WaterContact → ActorVitals
    → Dead`, all dropped before `ActorValues` (W)), `apply_player_drowning_damage`
    (`character.rs:1455-1475`), `restoration_system` and `combat_damage_system`.
  - The `Dead → ActorValues` inversion that #4982 removed has not returned.
- `player_body.rs` (a070baaad):
  - `attach_player_body` / `attach_assembled_root` are `&mut World`.
  - `set_player_view` walks first, then does the `HiddenFirstPerson` (W), then the
    `PlayerCameraView` (W). There is no overlap and no same-type reentrancy.
  - `toggle_third_person` copies both resources out.
  - The V-key site drops `InputState` before calling (`app_events.rs:532-539`).
  - `status_line` is read-only and sequential.
- `weather.rs` (a4a68fa92, afd6a73f7, 95b9d4f55): exclusive (`early.rs:30`). The #4416 block is
  guard-free (see finding 01). The `promote_weather_transition_target` destructure is a pure
  refactor.
- `character.rs` `player_water_state :1080-1160` (0963d675d, 0e607cbac, 17c01a4e5):
  - Order: `WaterPlane → WaterVolume → WaterFlow → WaterSurfaceMesh → WaterCurrentVolume`.
  - Same order as `crates/physics/src/water.rs:428-451` and `commands/water.rs:53-56`.
  - Resources are sampled before any storage (#3265).
- `metrics.rs :153-393` (0925f7926): the NVML sample runs under `MetricsState` (W) plus its
  internal `nvml` Mutex (leaf). The GPU-budget reads are consumed.
- `debug.rs`, `billboard.rs`, `audio.rs`: format/math only, or covered above.
- `hud.rs:793-812` / `scaleform_hud.rs:359-381` (#4675): `PlayerVitals` is held across `fraction`
  (`PlayerEntity`, `ActorValues` reads) and `HudControl` (W). `HudControl` is copied out by every
  caller (`app_frame.rs:437-441`, `scaleform_hud.rs:359`), so there is no same-thread read→write
  on `HudControl`.
- `ambient_ai_package_system` (`ai_package.rs:619-760`) holds `PackageRegistry` across
  `select_active_package` → condition evaluation. This predates the baseline (7473a387b). The
  only writer is `install_package_records(&mut World)` (`crates/scripting/src/package.rs:245`).
- `condition.rs:518-546` (a70b54f14, #4694): the `PapyrusPlayerEntity` read under the
  follow/escort/guard gather guards is a leaf. Every other `PapyrusPlayerEntity` site copies `.0`.

**Canonical order (`ecs.md` § Canonical acquisition order).**
- No new site takes `StringPool` before a storage.
- `Transform` still precedes cinematic state in the changed combat bodies.
- `player_body_facing_system` takes `Transform` alone.

**Guard ↔ sandbox boundary.**
- Only 34b53c464 touched `extensions/`. All dispatch systems scope `ExtensionHostSlot` before
  `host.lock()`, then run guest dispatch, then commit (`extensions/systems.rs:247-271`,
  `:546-597`).
- The new #4702 commit runs post-guest under the host Mutex. It is the same shape as the
  existing commits, and nothing under the host Mutex re-acquires `ExtensionHostSlot` or the
  host.
- Extension console commands enter guest code under the dispatcher's `CommandRegistry` read.
  This is the #1786 contract (`crates/debug-server/src/evaluator.rs:442-448`,
  `byroredux/src/main.rs:1570-1575`). `CommandRegistry` is written only at boot
  (`boot/registries.rs:63`), so there is no reentrancy.
- The three sync systems that hold the `ExtensionHostSlot` read across `host.lock()`
  (`systems.rs:338-347`, `:360-369`, `:380-389`) are pre-existing leaf `set_*` calls with no
  guest entry and no reverse path. The slot writer (`install.rs:824`) holds no host lock.

**CI pin tests.**
- `vulkan_validation_job_enables_the_lock_order_detector` (`scheduler_access_tests.rs:370`) and
  `vulkan_validation_job_fails_on_a_panic` (`:390`) exist and are not `#[ignore]`d.
- Both pin ci.yml *text* only. Neither can observe that the lane dies at vkCreateInstance and
  then panics in `rumble_on_activate_system` before the scheduler ever runs a parallel batch
  against a real world (O-1). The green pin coexists with zero live coverage.

**Dedup notes.**
- #4739 (OPEN, "combat feedback hand-rolls its own sound cache instead of SoundCache") appears
  to have been fixed by 546366364 (`combat_anim.rs:382-399` now uses `byroredux_audio::SoundCache`)
  but is still open. Route to `/audit-audio` for closure.
- #4821 and #4816 are still open. They are not re-reported (ECS-2026-09-28 Dim 5).

### Dim 4 — Scheduler Proof Soundness (12 commits)

#### Verified clean

**The proof is live**:
- `cargo test -p byroredux --bin byroredux -- scheduler_access system_access_declaration fragment_activation_order`: 28 passed, 0 failed, 0 ignored.
- No `#[ignore]` in `scheduler_access_tests.rs` or in `boot/schedule/mod.rs`.
- `add_to_with_access(` in `boot/schedule/` = 9 (early 2, update 1, post_update 1, physics 1, late 4) = `PARALLEL_SYSTEMS.len()` 9.
- `add_exclusive_with_access(` = 36, `add_exclusive(` = 45.
- The floors hold with **zero slack**: parallel systems 9 ≥ 9, and analyzed pairs = C(2,2) + C(4,2) = 7 ≥ 7. Every other stage has a single parallel system.
- The three release `assert_eq!`s are intact at `registries.rs:27-50`.
- The only non-`boot/schedule` registration is `DebugDrainSystem` (`debug-server/src/lib.rs:34`, `add_exclusive`), and its order is pinned before the report snapshot.

**078f650ec "reduce scheduler churn"**:
- It does not touch `scheduler.rs` or `access.rs`. Neither file has any commit since f97775ca8.
- Its `sync.rs` / `world.rs` edits only reorder Rapier `bodies.get` → `get_mut` inside an already-held `PhysicsWorld` write guard, so the ECS acquisition surface is unchanged.
- `access_report()` is rebuilt from the live `StageData` on every call (`scheduler.rs:556`). There is no cached batch and no stale list.
- `SchedulerSystemTimings` is written only after all stages (`scheduler.rs:517-528`), outside any parallel batch.

**Each parallel system: declared vs. actual (full transitive body, incl. `get`/`try_*`/cross-file hops/macros)**:

| Stage | System | Actual | Declared | Undeclared | Notes |
|---|---|---|---|---|---|
| Early | player_controller_system | 28 | 28 | 0 | 5 invisible to the guard (D4-01); `Dead` via `get` + `query_mut`, declared write |
| Early | timer_tick_system | 2 | 2 | 0 | disjoint from the controller |
| Update | make_animation_system | 17 (+5 colour sinks via `$Comp` macro) | 23 | 0 | over-declares `AnimationClipRegistry`/`StringPool` write, which is conservative only |
| PostUpdate | make_transform_propagation_system | 4 | 4 | 0 | unchanged |
| Physics | physics_sync_system | 24 | 24 | 0 | 11 invisible to the guard (D4-01); alone in its stage |
| Late | camera_follow_system | 8 | 8 | 0 | a070baaad `PlayerCameraView` read declared (`late.rs:57`) |
| Late | reverb_zone_system | 2 | 2 | 0 | — |
| Late | log_stats_system | 6 | 6 | 0 | 4dba10825 `SchedulerSystemTimings` read declared |
| Late | metrics_sample_system | 8 | 8 | 0 | — |

In each batch, the write sets are pairwise disjoint (Early: controller vs. `ScriptTimer`/`TimerExpired`; Late: Transform/GT vs. AudioWorld vs. Metrics*). No parallel system lost its declaration.

**New cross-stage producer/consumer pairs this window**:

| Resource / component | Writer (stage, mode) | Reader(s) | Order | Pinned |
|---|---|---|---|---|
| `PlayerCameraView` + `HiddenFirstPerson` (a070baaad) | `set_player_view`: V key `&mut World` between frames; `player.view` via Late-exclusive drain | camera_follow (Late ‖), renderer | writes are outside the parallel window; a console toggle applies next frame (benign) | n/a |
| body-root `Transform` (a070baaad) | player_body_facing (Late excl) | PostUpdate propagation | **writer after reader** | **no → D4-02** |
| `AiCombatState` 2nd producer (#4414/#4825) | faction_hostility (Update excl, `update.rs:229`) | npc_combat_ai (Update excl, `:255`), PostUpdate package/locomotion excl | writer ≤ reader | exclusive order unpinned; lag would be one frame on a throttled evaluation, so not filed |
| `ImageSpaceBase` (#4416) | weather_system (Early excl), cell_loader `&mut World` | image_space_modifier_system (Update excl) → renderer | Early < Update | structural |
| `SoundCache` (546366364) | combat_feedback (PostUpdate excl) | same system; ownership_sample outside the scheduler | single exclusive owner | n/a |
| `ActivateEvent` flush vs `container_loot_system` (#4712) | fragment_activation_flush (Update excl, `update.rs:167`) | container_loot (`:171`) + 4 others | flush < consumers | yes: `activation_flush_is_scheduled_before_every_activate_event_consumer` |
| npc_combat_ai `AmbientPackageRuntime`/`Seated`/`SeatReservations` (6c517bc7a) | Update excl | PostUpdate ambient-package excl | Update < PostUpdate | structural |

The existing pins #4186 (WindField), #3652 (billboard), #4185 (footstep) and #3180 (submersion) are all present and green.

### Dim 5 — RwLock Patterns / Physics (22 commits)

#### Routed (not concurrency; not filed here)

- **Newcomer generation cache never re-arms while a ragdolled corpse is resident.** This is from 88c23887b and goes to `/audit-physics` or `/audit-performance`.
  - `collect_newcomers` sets `all_registered = false` for any `CollisionShape` row without `RapierHandles` (`sync.rs:962-968`), even when the row can never register.
  - `activate_ragdoll` removes a bone's `RapierHandles` and `RigidBodyData` but leaves its `CollisionShape` (`byroredux/src/ragdoll.rs:441-470`). The only production `remove::<CollisionShape>` is in `player_body.rs:276`.
  - So a single activated ragdoll turns the #3477 cache off for as long as the corpse is resident, and every tick goes back to a full shape scan. That is no worse than before 88c23887b, but the cache stops doing its job.
  - Not measured at runtime.

---

#### Verified clean

1. **ad1d53a11 (rayon newcomer conversion)** (`sync.rs:1035-1041`)
   - The closure captures owned `Newcomer` plus `&ContactConfig` (Copy). There is no `&World`, no guard, and no query or resource acquisition on pool threads.
   - `collision_shape_to_parts` (`convert.rs`) has no statics or locks; it only logs.
   - `Vec::into_par_iter().map().collect()` preserves order, so insertion into `pw.bodies` / `pw.colliders` / `dynamic_bodies` stays serial and in newcomer order. Rapier registration is deterministic.
   - A panic unwinds before `pw` is taken, so no partial batch reaches the solver.
   - The global pool has no other World-touching users during Stage::Physics. The cell-stream worker uses its own `stream_pool` (`streaming.rs:677,1162-1171`). `render/mod.rs` `rayon::join` and `load_order.rs` `in_place_scope` run on the main thread outside the scheduler run.
   - `run_tracked` holds no Mutex across `system.run` (`scheduler.rs:67-90`).
2. **Phase 1 guard lifetimes**
   - `collect_newcomers` (`sync.rs:906-993`) takes its `try_resource::<PhysicsWorld>` read inside `and_then` and drops it at `:910-911`.
   - Storage order is RapierHandles → CollisionShape → RigidBodyData → GlobalTransform → ActorBoneCollider, the same as `push_kinematic` (`:1198-1206`). All five are dropped (`:986`) before `try_resource_mut::<PhysicsWorld>` (`:987`).
   - `register_newcomers` holds `pw` alone across insertion (`:1043-1163`, explicit `drop(pw)`), then takes `query_mut::<RapierHandles>` (`:1179`).
3. **Phases 2, 2.5, 3 and 4**
   - `push_kinematic` snapshots, then drops three guards (`:1224-1226`), then takes `pw` (`:1228`). 078f650ec's `get` → `get_mut` split is internal to Rapier with no ECS effect.
   - `apply_buoyancy`:
     - Resource snapshots come before storages (`water.rs:633-651`).
     - `pw` read is taken in its own block (`:678-689`).
     - Storages are collected, then dropped (`:773-775`), before the `pw` write (`:786`).
     - `clear_stale_water_contacts` follows the same shape (`:513-521`).
     - The new `WaterSurfaceMesh` read (17c01a4e5) is a storage read inside `collect_water_surfaces`, with no resource held.
   - The step takes `pw` alone (`sync.rs:158-161`).
   - `pull_dynamic`:
     - handles and body guards drop (`:1290-1291`) before the `pw` read;
     - `Parent`/`GlobalTransform` drop before `Transform` read (`:1334-1386`);
     - that read drops before `Transform` write (`:1391`).
4. **Guard tests live and asserting** (neither is `#[ignore]`d)
   - `pull_dynamic_does_not_close_transform_global_transform_lock_cycle` (`sync.rs:1709`, detector-gated) replays Transform→GlobalTransform, then runs `physics_sync_system`.
   - `get_actor_value_does_not_hold_actor_values_across_ruleset` (`crates/scripting/src/condition.rs:1658`) is a source-order check plus a detector replay.
5. **Helper order**
   - `set_linear_velocity` / `set_kinematic_translation` (`sync.rs:55-111`) drop the `query::<RapierHandles>().and_then(...copied())` temporary before `resource_mut::<PhysicsWorld>`.
   - No caller holds a `PhysicsWorld` guard or a storage guard at the call site:
     - `character.rs:607` (after `drop(pw)` at `:484`; the diagnostic `pw` at `:509` is block-scoped), `:857`, `:986`
     - `camera.rs:121`
     - `cinematic.rs:150,422,751,903`
     - `save_io.rs:744`
     - `commands/view.rs:507`

     `cinematic_horse_route_system` holds a `PackageTargetRegistry` read across its tail (`cinematic.rs:281-424`). That is pre-delta, and the only writers use `&mut World`, so it is benign.
   - 32f774c01's `character_capsule` (`world.rs`) is a stack value with no shared state.
   - The #4414 `line_of_sight_blocked` is taken per cast via `try_resource::<PhysicsWorld>().is_some_and(...)` with no storage live (`faction_hostility.rs:355-367`). `player_body` resolves its `RapierHandles` read through `and_then` (`:318-322`).
6. **ContactConfig**
   - It is snapshotted once per batch in `register_newcomers` (`sync.rs:1010-1013`), not per newcomer, and is not read from rayon threads.
   - The same holds in `character_controller_system` (`character.rs:378-381`, dropped before `pw` at `:382`) and `activate_ragdoll` (`ragdoll.rs:405-408`).
7. **Cell-unload teardown (#1520)**
   - `release_victim_rapier_bodies` (`unload.rs:649-685`) collects `RapierHandles` and `Ragdoll` in a scoped block, which drops them before `try_resource_mut::<PhysicsWorld>` (`:673`). It runs before `despawn_batch` (`:403` vs `:410`).
   - The unload.rs delta (fa6a551ce) is timing-only (`:256-260`).
   - #4982's inverted hoists live in `reference_state::capture` / `stream_snapshot::capture_actor_snapshots` and were fixed by 99933f87b, which is an ancestor of HEAD. `reference_state.rs:96-200` now takes one guard per pass, and the orchestrator's workspace detector run reports zero cycles.
8. **Placement**
   - `physics_sync_system` is the sole Stage::Physics system (`boot/schedule/physics.rs:7-58`; no other `Stage::Physics` registration in `byroredux/src`). The stage runs after PostUpdate propagation.
   - Its declaration covers every storage and resource read in phases 1-4, the water pass and the faller diagnostic (pinned by `scheduler_access_tests.rs:59,88`).
9. **Ragdoll and character**
   - `ragdoll_writeback_system` acquires `PhysicsWorld` last with no storage taken under it (`ragdoll.rs:519-541`, #3655 order).
   - 3ce2e1d7b and c9843d5d1 only add `HashSet` / `HierarchyTraversalGuard` locals. `world.next_entity_id()` is a plain field read.
   - `activate_ragdoll` snapshots, then writes `pw`, then writes storages (`:400-470`).
   - Water sampler (0963d675d, 0e607cbac): resources come first (`character.rs:1085-1088`), then read-only storages in the order WaterPlane → WaterVolume → WaterFlow → WaterSurfaceMesh → WaterCurrentVolume. That matches `water.rs:428-432`. There are no runtime `query_mut` writers of these storages, and all of it finishes before `pw` (`:382`).
   - a070baaad adds no `PhysicsWorld` / `RapierHandles` holder:
     - `attach_assembled_root` runs on `&mut World`, strips `CollisionShape`/`RigidBodyData`, and runs after the spawn bootstrap registration.
     - `player_body_facing_system` is exclusive and touches only `InputState` → `Transform`.
     - The `camera_follow_system` `PlayerCameraView` read is a map-copy temporary (`character.rs:703-706`).
10. **Internal-only PhysicsWorld changes**
    - fe80f4d76 (`FixedPairFilterBroadPhase`), 15ad2455c (incremental query pipeline), fa6235775, fb8fae288 and 6ca10bf4b (explosion recovery, `dynamic_bodies` index) mutate only `PhysicsWorld` fields under the caller's existing guard. None touches World, statics or thread-locals.
    - `dynamic_bodies` tolerates stale handles (`world.rs:868-869` `retain`).
    - `set_motion_type` feeds both `body_left_fixed` and the index (`world.rs:606-640`). Its only production caller holds only the `pw` write (`cinematic.rs:261-267`).

### Dim 6 — Resource Lifecycle (32 commits)

#### Existing issues relevant to this dimension (re-confirmed at HEAD, not re-filed)
- **Existing: #4881** (OPEN): `StagingPool` capacity labels shrink when a buffer is reused. #4593 and #4790 made every `release_to` record the *requested* size: `buffer.rs:909,1622,1778`, `texture.rs:255,328`, `texture_registry/upload.rs:612`, `scene_buffer/upload.rs:1050`. `acquire` (`buffer.rs:218-224`) can return an entry whose real size is larger than the request, and that buffer's recorded capacity then drops to the smaller request. Nothing is leaked or freed twice (every entry is still destroyed by `trim_to`/`destroy`), but retained memory can exceed `DEFAULT_STAGING_BUDGET_BYTES`. The #4790 fix itself is in place: the terrain ring releases at `previous_size`, the size stored with that guard (`scene_buffer/upload.rs:1037-1050`).
- **Existing: #4599** (OPEN): `.lock().expect/unwrap` on the allocator is reachable from teardown or `Drop`. New instances in this window follow the same pattern: `texture.rs:528` (the e26441c34 view-failure unwind) and `context/helpers.rs:50` (`destroy_staging_buffer`, reached from `Drop` via `destroy_screenshot_staging`/`destroy_depth_capture_staging`, `teardown.rs:331-332`). `GpuImage` still recovers from poison (`image.rs:231-234`, #4089).
- **Existing: #4890 / #4891**: `?` windows in the resize path and inconsistent rollback. Examples: a partial failure in `ReservoirBuffers::recreate_on_resize` (`restir.rs:191-203`), and TAA's partial failure leaving `post.taa = Some(destroyed)` (`taa.rs:854-860`, `resize.rs:1163-1183`). Both can only be reached after the resize has already returned `Err`, and the `framebuffers.is_empty()` guard (`draw.rs:2110-2112`) skips `draw_frame` before `reservoir_buffers.begin_frame` (`begin_frame_recording.rs:99`) or any TAA bind. No new hazard.
- **Existing: #4889 / #4892**: dynamic-RGBA staging is fatal on failure, and it has no memory-budget ledger row. Its lifecycle is covered under Verified clean below.

---

#### Verified clean

**Load-bearing teardown orderings (all three hold):**
- `skin_slots` drain (`teardown.rs:85-91`, unconditional, with the free gated inside the loop) runs before `SkinComputePipeline::destroy` (`teardown.rs:150-152`). The `skin_slot_drain_sits_outside_the_skin_compute_guard` test still pins this (`:507-551`).
- `frame_upscaler.destroy_device_objects()` runs in the allocator-independent block (`teardown.rs:327-329`), before `destroy_allocator_owned_resources` (`:355-357`), which runs `destroy_allocations` (`:204-206`).
- `post.exposure.destroy()` (`teardown.rs:199`) runs before `self.allocator.take()` + `Arc::try_unwrap` (`:437-438`).
- The 1×1 placeholders are destroyed inside the allocator block (`:178-183`), before `try_unwrap`.

**`Arc::try_unwrap` / SharedAllocator holders:** No field of type `SharedAllocator` / `Arc<Mutex<Allocator>>` was added in the window (the diff grep came back empty). The existing holders are all released before `try_unwrap` by earlier teardown steps:
- `ExposureResource` (`exposure.rs:90`, destroyed at `:199`).
- `WaterPipeline` (`water.rs:328`, `teardown.rs:313-315`).
- `StagingPool` (`buffer.rs:161`, destroyed via `scene_buffers.destroy`).
- `GpuBuffer`/`Texture`/`GpuImage` `Option` clones, taken inside `destroy`.
- `EguiPass`'s renderer, dropped when `egui_pass.take()` goes out of scope (`teardown.rs:265-267`).

**New resources traced (create → destroy → resize):**
- **GroundCoverModelTier (#4413, aabd99a05).**
  - Create: `init.rs:759-771`. `new()` cleans itself up on partial failure (`groundcover_models.rs:319-323`).
  - Destroy: `teardown.rs:137-140` → `groundcover_models.rs:830-876`, which covers the pipeline, layout, pool (and its sets), set layout, 4× per-slot `Vec<GpuBuffer>` and 3 shared buffers.
  - Resize: none needed. It is compute-only (no render-pass pipeline) and holds no extent-sized resources. Its per-FIF descriptor set is rewritten every `record` against this frame's `instance_buffers()[frame]` (`groundcover_models.rs:~693-712`), and `4ec1e48c5`'s source-scan pins the record-after-grow order.
  - No per-frame Vulkan allocation.
- **`pipeline_early` (186234944).**
  - Create: `pipeline.rs` `triangle_pipeline_inner`. Shader modules are destroyed on every error path, and a partial `create_graphics_pipelines` failure destroys the returned partial pipelines.
  - Destroy: `helpers.rs` `destroy_render_pass_pipelines`, called at `teardown.rs:399-405`.
  - Resize: on a format change it is destroyed at `resize.rs:224-230` and rebuilt at `:346-355`. That is the only rebuild site.
- **ReSTIR light identity / `begin_frame` (186234944, a37fcba3c).** No new GPU objects. `LightHeader` grew to 4112 B, and every size is derived from `size_of::<LightHeader>()` (`buffers.rs:542`, `upload.rs:134,1179`). `LightHistory` is CPU-only. Reservoir buffers are recreated on resize and scene set bindings 16/17 are rewritten (`resize.rs:664-686`).
- **TAA reactive binding 9 (#4944).** Pool sizes are derived from the bindings (`taa.rs:416-421`), so the new sampler is counted. `reactive_views` is threaded to all three builders: init (`init.rs:~1479`), resize (`resize.rs:1177`) and the upscaler switch (`resize.rs:1531-1541`). `recreate_on_resize` frees every per-FIF history slot (`taa.rs:826-834`).
- **GPU timers / fragment-invocation pools (88c23887b, 39c6f0aff, b978bb5a1).**
  - Pools: created per FIF at `gpu_timers.rs:652-690` (partial cleanup correct) and destroyed at `:1762-1771`, inside the allocator-independent block (`teardown.rs:296-306`).
  - Per-frame reset: host reset of both pools each frame (`:794`, `:828`), with no per-frame pool creation. Query indices are const-asserted to end exactly at `QUERIES_PER_FRAME` (`:437-444`, #4980).
  - Resize: not applicable.
- **Dynamic RGBA staging (`texture_registry/dynamic_rgba.rs`, in window).**
  - Allocation: per-FIF grow-only arena (`:150-164`). An old arena is destroyed only after the `fence_confirmed_idle_slot` check (`:129-132`).
  - Teardown: `DynamicRgbaUploads::destroy` (`:104-114`) is called from `TextureRegistry::destroy` (`texture_registry/mod.rs:1095`).
  - Handle release: queued updates are dropped when their handle is released (`release.rs:44`, `mod.rs:881`), so a released handle cannot be copied into.
- **MorphDelta `release_shared` (#4838, 5e3361a1b).**
  - If `try_unwrap` fails, only this handle is dropped (`morph_compute.rs:295-302`). The last strong holder destroys the delta. This is a deferred free, not a leak.
  - Run-time eviction happens only after the all-slots `wait_for_fences` (`sync_and_acquire_frame.rs:81`, then `skinned_blas_refit.rs:962-981`), so no in-flight frame can still be reading the device address.
  - The creation-failure path releases a freshly created delta (`resources.rs:130-135`).
  - Teardown drains `morph_slots` and then clears the weak cache (`teardown.rs:96-99`), before `try_unwrap`.
  - Latent, unreachable: `try_spawn_morph_slot` discards `morph_slots.insert`'s return value (`byroredux/src/cell_loader/spawn/mesh_instance.rs:1625`). A replaced slot would hit the `GpuBuffer::Drop` safety net. This requires an `EntityId` to be reused, and ids are monotonic in production: `restore_world`, the only `set_next_entity` caller (`crates/save/src/driver.rs:123-125`), is test-only.
- **Volumetrics / godrays (0572bfd5a, 88c23887b).** No new GPU objects. The window only changed CPU filtering and dirty-range uploads. `fog_cluster_dirty_range` starts as the full range (`volumetrics/init.rs:110`), so the first write after a rebuild covers the uninitialised allocation. `destroy` covers every `FroxelSlot` and `Vec<GpuBuffer>` field (`volumetrics.rs:2288-2378`). The whole pass is destroyed and rebuilt on resize (`resize.rs:866-918`), and its only view consumer (composite binding 6) is rewired (`resize.rs:1014-1019`).
- **Composite sky apertures (0572bfd5a).** Only the UBO grew; it is sized from `size_of::<CompositeParams>()`. The exposure meter writes its descriptor at every dispatch (`exposure_meter.rs:233-250`), so no stale view is possible after a resize.
- **TLAS entity-id stabilisation (5eb07a4f3, efc059f3a).** CPU-only `Vec<EntityId>` scratch and caches (`acceleration/mod.rs:186-189`, `tlas.rs`). No new GPU objects.

**AS shutdown:** `AccelerationManager::destroy` (`acceleration/mod.rs:446-517`) cleans up the following:
- It calls `drain_pending_destroys` first (`blas_static.rs:164-192`), which covers `pending_destroy_blas` and `pending_destroy_scratch` and resets the pending byte counter.
- It then destroys all `blas_entries`, every TLAS slot (accel, buffer, instance buffer and device instance buffer), drains `skinned_blas`, and destroys the per-FIF `scratch_buffers` and the `blas_scratch_buffer`.

**SceneBuffers:** every `GpuBuffer` / `Vec<GpuBuffer>` / `StagingPool` / `StagingGuard` / descriptor-pool field of `SceneBuffers` is named in its `destroy` (`scene_buffer/descriptors.rs`). That includes `retired_instance_buffers` (deferred queue), `terrain_tile_staging_*` and `selected_ray_probe_buffers`. Checked mechanically by comparing the field list against the destroy body.

**Egui:** `egui_pass.take()` + `destroy` runs first in `Drop` (`teardown.rs:265-267`). On resize, a failed framebuffer recreate destroys the pass it took (`resize.rs:1090-1104`), and a format change destroys it before rebuilding (`:1106-1133`). `init_egui` returns early if a pass already exists, so it cannot replace (and leak) one (`mod.rs:1384-1386`).

**Per-frame leaks:** the window adds no per-frame `create_*` / `allocate_descriptor_sets` / `allocate_command_buffers` / `create_query_pool` calls (checked by grepping the whole-window diff). The per-frame `update_descriptor_sets` calls write into sets allocated once.

### Dim 7 — Worker Threads (19 commits)

#### Verified clean (per checklist item)

**BSA/BA2/CSG positional reads (`1b8b21f3f`).**
- The checklist premise "reads serialised by a Mutex" is stale. `Mutex<File>` is gone from `archive/mod.rs:48`, `ba2.rs:125` and `csg.rs:134`.
- All extraction goes through `ReadAt::read_exact_at`:
  - BSA: `extract.rs:43,62,106,116`
  - BA2: `ba2.rs:479,497,914,918`
  - CSG: `csg.rs:326`
- No cursor read happens after open. The BufReader is only used during open, then `into_inner` (`open.rs:450`, `ba2.rs:367`).
- Unix `FileExt::read_exact_at` is a true `pread`.
- `Send`/`Sync` of `BsaArchive`, `Ba2Archive` and `CsgArchive` is automatic (File + HashMap + `Mutex<ChunkCache>`). No `unsafe impl` was added anywhere; `grep 'unsafe impl.*(Send|Sync)'` over crates, byroredux and tools returns 0.
- CSG `chunk_bytes` never holds the cache lock across the read or the inflate (`csg.rs:312-327,364`). A duplicate inflate on a racing miss is benign and documented.

**Streaming worker lifecycle.**
- `shutdown` (`streaming.rs:1061-1085`) takes `worker` first, drops `request_tx`, then calls `join_with_timeout` (`:1133-1161`, poll on `is_finished`, no watcher thread).
- `Drop` calls `shutdown(1 s)`, and an earlier explicit shutdown short-circuits it (`:1097-1101`).

**Rayon panics.**
- The parse fan-out uses `stream_pool.in_place_scope_fifo` (`:1596-1609`). Rayon joins all tasks and re-raises the first panic on the worker. That lands inside `pre_parse_cell_panic_safe`'s `catch_unwind` (`:1242-1254`, `:1292-1313`), so the worker survives.
- The only code a task runs outside `parse_one_nif`'s per-NIF guard (`:1336-1422`) is `extract_mesh` (`:1471-1475`).
- `ParseInputPermit` is owned by the task closure, so unwinding drops it and notifies the budget. The coordinator's Condvar wait (`:1524-1528`) cannot strand.
- `batch_keys` is extended only after a successful pipeline (`:1911-1915`), so a caught panic does not poison the batch memo.

**Bounded join while the worker is inside a scope.** The worker does not observe the closed channel until the scope finishes. The 1 s `join_with_timeout` still bounds the main thread; after that it detaches. The detached worker holds only `Arc`s (`TextureProvider`, `ExteriorWorldContext`, `ThreadPool`) plus a worker-local `CsgHandleCache`, so it is memory-safe.

**Worker ↔ main split.**
- Payloads cross over mpsc. `PartialNifImport: Send` is enforced at compile time (`:653-656`). No `&World` reaches the worker.
- The new worker-side work does not move material resolution or StringPool interning onto the worker:
  - Precombine decode interns into a per-task `StringPool::new()` (`:1348-1356`).
  - `merge_precombine_materials` (`&mut MaterialProvider`) and the re-intern both run on the main thread in `finish_partial_import` (`cell_loader/partial.rs:76-96`).
  - Input extraction (`pre_parse_one`, `:1466-1484`) reads only `&TextureProvider`.
- The NIF cache is read-only on the worker: it checks the `cached_keys` snapshot (`:1675-1689`), and write-back happens on the main thread (`partial.rs` `insert_cached_import`).

**Texture prefetch (`a3632909a`, `texture_prefetch.rs`).**
- Bytes are staged in `TextureProvider.prefetch`, a `Mutex` + `Condvar` store (`texture.rs:23`).
- Keys are queued only on the main thread (`streaming_helpers.rs:700-730`), after the import is finished with materials merged.
- An epoch guards staleness:
  - `clear()` bumps the epoch and drops every slot (`:164-179`). It is called on apply completion, cancellation and worldspace drain (`streaming_helpers.rs:694,814,851,855,881`).
  - `start` and `finish` ignore an old epoch (`:101-135`).
- `take()` never waits on a queued key; it withdraws it. It waits only on a `Running` read, which always completes because `catch_unwind` wraps `extract` and `finish` is poison-tolerant (`:218-229`). So `ThreadPool::spawn`'s abort-on-panic default cannot fire.
- Pool tasks never touch `World` or any lock the main thread holds, so the main thread's Condvar wait has no cycle.
- The `ResolveExtractCounters` atomics (`84c06e67a`) are written and read only on the main thread (Relaxed is fine).

**Debug server (`dd99cd0f3`).**
- The thread shape is unchanged: listener at `listener.rs:190`, client-count cap 8 (`:250-271`), queue cap 64 (`:89-106`).
- Clients never touch `World`. `DebugDrainSystem` runs on the main thread as a Late exclusive system.
- With the 30 s timeout, an abandoned command:
  - has its `rx` dropped, so the drain's `response_tx.send` fails harmlessly (`system.rs:190`);
  - is honoured via `cancel` on the screenshot path (`system.rs:72-83`, #1007/#3090);
  - still executes if it is a non-screenshot command. This was dismissed in AUDIT_CONCURRENCY_2026-08-27 note 6 and is bounded by the cap of 64.
- A parked client thread now sits up to 30 s in `recv_timeout`. `shutdown(Both)` does not wake a channel wait, but client threads are detached by contract (#855), so teardown never blocks.
- The screenshot readback path is unchanged in this window.

**Allocator sharing.**
- New sites lock only in statement-scoped temporaries inside allocate/free (e.g. `texture.rs:451-455,484-488,524-528`).
- `dynamic_rgba.rs` records into the frame command buffer and does no submit or wait; a source test pins this (`:222-228`). `groundcover_models.rs` delegates to `GpuBuffer` helpers.
- The `5226d73e2` live-VRAM gate calls `VulkanContext::live_memory_budget` on the main thread, before the rebuild (`app_frame.rs:341-347`). It queries `VK_EXT_memory_budget` properties with no allocator lock (`telemetry.rs:33-41`).
- The one-time submit still releases the queue Mutex before `wait_for_fences` (`texture.rs:1017-1029`, #1713).

**Send + Sync / Ruffle / kira.**
- Ruffle's wgpu device stays on its owning thread. `#4717` pacing (`crates/ui/src/pacing.rs`) and `e405774ef` add no threads, and futures run on a `NullExecutor` owned by the player (`navigator.rs:139-154`).
- `573170e1c` added a static `INIT_LOCK` in `get_or_try_init` (`player.rs:144-157`). The only caller is `shared_descriptors`, which does not re-enter, so it cannot self-deadlock.
- SoundCache (`546366364`) is loaded and decoded on the main thread inside the exclusive PostUpdate `combat_feedback` system (`combat_anim.rs:382-414`, `post_update.rs:136-139`). The SoundCache write guard is released before `AudioWorld` is taken, and `play_oneshot` only enqueues (`crates/audio/src/lib.rs:577-602`). No kira call runs under the SoundCache or provider guard.

**`unsafe impl Send/Sync` this window:** none. The only `unsafe impl` added is `NoUninit` (bytemuck POD) for two ground-cover GPU structs.

#### Thread / pool spawn census (production)
| # | Site | Kind | Window status |
|---|---|---|---|
| 1 | `byroredux/src/streaming.rs:879-882` | `byro-cell-stream` OS thread | changed (`67de801f8`, `e593770f0`, `5eb07a4f3`) — clean |
| 2 | `byroredux/src/streaming.rs:1182-1192` | dedicated rayon pool `byro-stream-parse-*` (N/2); used at `:1596/:1602` | changed — clean |
| 3 | `byroredux/src/asset_provider/texture_prefetch.rs:218` | `pool.spawn` onto #2 from main | **NEW** (`a3632909a`) — clean |
| 4 | `crates/debug-server/src/listener.rs:190` / `:273` | listener + per-client OS threads | timeout only (`dd99cd0f3`) — clean |
| 5 | `crates/core/src/ecs/scheduler.rs:502` | global-pool `par_iter_mut` | changed (`078f650ec`) — Dim 3/4 owns |
| 6 | `crates/physics/src/sync.rs:1036` | global-pool `into_par_iter` (shape conversion) | **NEW** (`ad1d53a11`) — thread-safety clean: `collision_shape_to_parts` is pure (`convert.rs`, no statics/TLS), no ECS guard held across it (`sync.rs:1010-1013` guard is statement-scoped); ECS side → Dim 5 |
| 7 | `byroredux/src/render/mod.rs:1100,1138,1154,1161` (+`:924` par_sort) | global-pool `rayon::join` | **NEW/expanded** (`de808add3`) — closures take read-only guards and never nest rayon while holding one, so no same-thread steal/re-entry; ECS side → Dim 3 |
| 8 | `byroredux/src/cell_loader/load_order.rs:603,670` | global-pool `in_place_scope` parallel plugin walk | **NEW** (`382fa9296`) — not in the dispatch list; clean: ESM walk has no internal rayon, `StringsTableGuard` restores the previous TLS value (`plugin/.../common.rs:111-121`), main blocks in the scope |
| 9 | `tools/byro-launcher/src/engine.rs:86` | child stderr tail reader | unchanged |
| 10 | `tools/byro-dbg/src/tui.rs:405` | `byro-dbg-net` | unchanged |
| — | kira/cpal audio thread (external, inside `AudioWorld`); no Ruffle threads | | unchanged |

Test-only (excluded): `scripting/src/quest_stages.rs:1588,1622`; `bsa/src/ba2.rs:1250`; `ui/src/player.rs:871`; `core/src/ecs/resources/mod.rs:2184`; `core/src/ecs/lock_tracker.rs:532,959,970`; `papyrus/src/parser/script.rs:1268`; `asset_provider/texture_prefetch.rs:254`.

**Coverage-gap note for the orchestrator:** three parallel sites in this window were not in the dispatch list: #6, #7 and #8. I checked the thread-safety side of each. The ECS-guard side of #6 and #7 belongs to Dims 5 and 3.
