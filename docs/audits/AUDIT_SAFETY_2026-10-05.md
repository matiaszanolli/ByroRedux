**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-09-29.md` (@ `9fcfdc3fc`, 313 commits ago) · **Audited**: Dims 1–7 (every one of their `Paths:` had commits since the baseline) · **Unchanged since baseline (skimmed)**: Dim 8 (sandboxed mod runtime). Its only path deltas are the clippy sweep in `byroredux/src/extensions/tests.rs` (test-only) and a `toml_edit` line in the workspace `Cargo.toml`, so it got a guard spot-check only.

# Safety Audit — ByroRedux — 2026-10-05

**Command**: `/audit-safety` (default scope), one leg of `/audit-suite --preset comprehensive`
**Severity scale**: `.claude/commands/_audit-severity.md`

## Method

- **Scoping.** Delta-scoped against `9fcfdc3fc` (`git log 9fcfdc3fc..HEAD -- <Paths>`). Commit counts per dimension:

  | Dim | 1 | 2 | 3 | 4 / 5 | 6 | 7 | 8 |
  |---|---|---|---|---|---|---|---|
  | Commits | 6 | 11 | 26 | 79 | 23 | 26 | 2 |

  In Dim 1, every commit touched `crates/ui` or `texture_registry`. `fsr3-sys`, `frame_upscaler.rs`, the launcher preflight and `cxx-bridge` had no commits.
- **No sub-agents.** Every dimension was analysed synchronously. Per-dimension notes are in `/tmp/audit/safety/dim_{0..8}.md` (`dim_0` is the census). This report was reconciled against each one.
- **Commands run.** No engine or GPU process was launched, and no source was edited. All cargo commands used the rustc 1.96.0 toolchain binary, per CLAUDE.md.
  - `cargo test -p byroredux --bin byroredux -- rapier_release bone_palette_overflow`: **13 passed, 0 failed**. This covers 9 `rapier_release_tests`, including `release_sweeps_both_ragdoll_and_rapier_handles`, and 4 `bone_palette_overflow_tests`.
  - `cargo test -p byroredux-renderer --lib -- gpu_material gpu_instance gpu_camera gpu_light gpu_terrain volume_far depth_capture shader_constants triangle_frag_scales bindings_glsl the_pin_test name_diverging scene_descriptor_reflection allocator_lock teardown_path frames_in_flight host_mirrors`: **121 passed, 0 failed**.
  - `cargo clippy -p byroredux-renderer -p byroredux-fsr3-sys -p byroredux-nif -p byroredux-core -p byroredux-pex -p byroredux -p byro-launcher --no-deps -- -A clippy::all -W clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc`. Only the 3 known `unsafe impl` adjacency false positives in nif fired (`bs_geometry.rs:364-366`). The renderer was then re-linted alone with `-D clippy::undocumented_unsafe_blocks`: clean.
  - `cargo tree -p byroredux-mod-runtime | grep -ic wasi`: **0**.
- **CI evidence.**
  - The latest main CI run is `37168582805` @ `83fbbaaac`, 84 commits behind HEAD. `gh run list` shows no later main run.
  - Read: job `111336662415` (Test + Check + Clippy) and job `111336662550` (Vulkan validation layers (lavapipe)).
  - Two older lavapipe runs (`37091820948`, `37155077588`) were read for the timeline. So were the later lavapipe logs already cached by today's `/audit-concurrency` (`/tmp/audit/concurrency/vk*_*.log`, up to `37341506292` @ `23524b446`).
- **Dedup.** Checked against:
  - `/tmp/audit/issues.json` (97 open issues);
  - `gh issue list --state all` searches: `explosion_offences`, `keyframe_refusals_logged`, `device_name_as_c_str`, `CStr::from_ptr device_name`, `RUST_LOG vulkan-validation`, `Vulkan validation errors detected`, `OneTimeCommandError free_command_buffers`, `MaybeInFlight fence`, `08114`, `CONC-D2-2026-09-29`;
  - the state of every issue the baseline report cited;
  - today's sibling reports and scratch: `AUDIT_RENDERER_2026-10-05.md`, `AUDIT_ECS_2026-10-05.md`, `AUDIT_PERFORMANCE_2026-10-05.md`, `/tmp/audit/concurrency/dim_{2,3}.md`;
  - `AUDIT_RENDERER_2026-10-03.md` (the source of #5201).

## Census (re-measured at HEAD)

| Crate | Word tokens | `unsafe {` blocks | declared `unsafe fn` | `unsafe impl` | Notes |
|---|---|---|---|---|---|
| `crates/renderer/src` | 939 (09-29: 946) | 720 (733) | 96 (95) | 35 (34) | first net shrink; see below |
| `crates/fsr3-sys` | 12 | 10 (1 test-only) | 2 `pub` (`create`, `dispatch`), both with `# Safety` | 0 | no commits |
| `crates/nif` | 13 | 1 (`read_pod_vec_from`) | 0 | 1 macro + 3 (`bs_geometry.rs`) | no commits to `stream.rs` / `header.rs` |
| `crates/core` | 7 | 6 (4 `query.rs`, 2 `string/mod.rs` incl. 1 test) | 0 | 0 | no commits to `query.rs` |
| `crates/pex` | 4 | 1 (`transmute`) | 0 | 0 | no commits to `opcode.rs` |
| `byroredux` | 5 | **2** (09-29: 3) | 0 | 0 | #5120 replaced the `CStr::from_ptr` block with `device_name_as_c_str` |
| `tools/byro-launcher` | 3 | 1 | 1 | 0 | no commits |
| `crates/plugin` | 4 | 2 (test-only env edits) | 0 | 0 | |
| `crates/cxx-bridge` | 1 | 0 (one `unsafe extern "C++"`) | 0 | 0 | still only `native_hello() -> String` |

- **Renderer per-file block delta** (net −13):
  - `vulkan/buffer.rs` 40 → 19: the #4599 poison-policy extraction plus the #4882 `create_bound_buffer` single prologue.
  - `vulkan/groundcover.rs` 30 → 0, now split into `vulkan/groundcover/construct.rs` (18) and `vulkan/groundcover/frame.rs` (12). These are pure moves.
  - `acceleration/blas_static.rs` 25 → 29: `unwind_prepared`, #4883 / #5201.
  - `acceleration/tests/scratch_tests.rs` 0 → 2 (test-only).
  - `texture_registry/mod.rs` 12 → 14: the #4886 transactional `recreate_descriptor_sets`.
  - `context/resize.rs` 25 → 26 (#4890) and `device.rs` 21 → 22 (#4895).
  - `groundcover_models.rs` 16 → 15 (#5122 removed `push_bytes`) and `texture.rs` 17 → 16.
- **New declared `unsafe fn`:** `blas_static.rs::unwind_prepared`, which has a `SAFETY` contract. The groundcover `destroy` moved, unchanged, to `groundcover/frame.rs`.
- **New `unsafe impl`:** `NoUninit for ModelPush`. Its padding proof is true: the pad word is named, and the 64 B size is pinned.
- **No `unsafe` newcomer** in any previously unsafe-free crate. All remaining hits are prose, plus a source-scan string in `material_translate.rs:2570`. Only `crates/sdk` carries `#![forbid(unsafe_code)]`.

## Findings summary

| ID | Severity | Dim | Status | Title |
|---|---|---|---|---|
| SAFE-D5-2026-10-05-01 | LOW | 5 | NEW | `with_one_time_commands_inner` frees the command buffer and destroys/releases the fence on the `MaybeInFlight` wait-failure arm, though the error it returns says the commands may still be pending |
| SAFE-D3-2026-10-05-01 | LOW | 3 | NEW | `PhysicsWorld::remove_body` prunes `body_labels` but not its two #5161/#5246 siblings (`explosion_offences`, `keyframe_refusals_logged`), which grow for the whole session |
| SAFE-D4-2026-10-05-01 | LOW | 4 | NEW | #4895 added a third renderer `CStr::from_ptr(device_name)` block just as #5120 moved the identical byroredux block to ash's bounded `device_name_as_c_str()` |

Counts: **0 CRITICAL · 0 HIGH · 0 MEDIUM · 3 LOW** (3 NEW, 0 regressions).

**Cross-referenced, not re-filed.** The `vulkan-validation` lane has been red with zero real validation errors since 2026-10-01. This is owned by today's `/audit-concurrency` as **CONC-D3-2026-10-05-01** (MEDIUM). This audit independently confirmed it and adds evidence; see *Cross-referenced findings*.

## Findings

### SAFE-D5-2026-10-05-01: `with_one_time_commands_inner` frees the command buffer and destroys/releases the fence on the `MaybeInFlight` wait-failure arm, though the error it returns says the commands may still be pending
- **Severity**: LOW.
  - The decision tree's "Vulkan spec violation → at least HIGH" floor was weighed and not applied. The violation is reachable only when `vkWaitForFences` with an infinite timeout fails with `VK_ERROR_OUT_OF_HOST_MEMORY` or `VK_ERROR_OUT_OF_DEVICE_MEMORY`. After a true `VK_ERROR_DEVICE_LOST`, destroying and freeing these objects is valid per the spec's "Lost Device" section.
  - #5201 rated this exact window LOW for the AS objects the same command buffer writes. This finding is that issue's sibling inside the helper itself.
- **Dimension**: Vulkan Spec Compliance
- **Location**: `crates/renderer/src/vulkan/texture.rs:931-942`, the `wait_for_fences` error arm of `with_one_time_commands_inner`. The class contract is at `:671-674`, and the reusable-fence guard at `:869`.
- **Status**: NEW.
  - #5201 (CLOSED, `65b0217f3`) made `build_blas_batched` consult `OneTimeCommandError::may_be_in_flight` before unwinding its ASes.
  - #4891 (CLOSED, `9cc77cec0`) introduced the `NotSubmitted` / `MaybeInFlight` split for the callers.
  - Neither touched the helper's own `cmd` / fence disposal, which dates from #1861. No issue or audit names it (searched `OneTimeCommandError free_command_buffers` and `MaybeInFlight fence`).
- **Description**:
  - #4891 defines `MaybeInFlight` as "`vkQueueSubmit` or the fence wait failed… the commands may be pending, so a host-side destroy could race an in-flight transfer: the caller must keep what they reference alive". Every caller now honours that:
    - `buffer.rs:914/1528/1634` and `texture_registry/upload.rs` leak their staging and destinations;
    - `blas_static.rs` calls `mem::forget` on `prepared` and `compact_accels`.
  - The helper that returns the error does the opposite with the objects it owns. On the fence-wait failure it:
    - calls `free_command_buffers(pool, &[cmd])`, which is invalid while the buffer is pending (`VUID-vkFreeCommandBuffers-pCommandBuffers-00047`);
    - destroys the fence it created (`VUID-vkDestroyFence-fence-01120`);
    - or, on the reusable-fence path, drops the guard, so the *next* caller's `reset_fences` hits a fence possibly still tied to pending work (`VUID-vkResetFences-pFences-01123`).
  - The `vkQueueSubmit`-failure arm (`:920-929`) is spec-fine. A failed submit leaves the command buffer not pending, except on device loss. Only the wait arm contradicts its own class.
- **Evidence**:
  ```rust
  if let Err(e) = device.wait_for_fences(&[fence], true, u64::MAX) {
      if owned {
          device.destroy_fence(fence, None);
      }
      drop(fence_guard);
      device.free_command_buffers(pool, &[cmd]);
      return Err(OneTimeCommandError::maybe_in_flight(
          e,
          "wait for one-time commands",
      ));
  }
  ```
- **Impact**: None in normal operation. On an OOM-from-wait (the renderer is already failing), the helper frees or reuses objects the GPU may still be executing. That is undefined behaviour of the same class #5201 closed one layer up, and it leaves the "one helper, one failure contract" policy inconsistent.
- **Related**: #5201, #4891, #1861, #1713. Owner overlap: `/audit-renderer` Dim 1 raised #5201; `/audit-concurrency` covers fence discipline.
- **Suggested Fix**: On the wait-failure arm, branch on the `vk::Result`:
  - `ERROR_DEVICE_LOST` keeps today's free and destroy;
  - any other code leaks `cmd` and the owned fence, and poisons or marks the reusable fence so it is recreated rather than reset.

  Pin the arm with the existing `one_time_failure_class_tests` needle scan. Per the *Speculative Vulkan Fixes* rule, confirm the classification under `BYRO_VALIDATION=1` with fault injection before landing.

### SAFE-D3-2026-10-05-01: `PhysicsWorld::remove_body` prunes `body_labels` but not its two #5161/#5246 siblings (`explosion_offences`, `keyframe_refusals_logged`), which grow for the whole session
- **Severity**: LOW. This is CPU-side unbounded growth, keyed per event rather than per frame. Each entry is 8–12 B plus hash overhead, and an entry is added only when a body refuses a keyframe target or explodes.
- **Dimension**: Memory & Resource Leaks
- **Location**: `crates/physics/src/world.rs`:
  - fields `keyframe_refusals_logged` (`:315`, inserted at `:905`) and `explosion_offences` (`:343`, `entry(handle)` at `:1025`);
  - `remove_body` at `:536-560`, which prunes only `body_labels` (`:551`).

  Introduced by `44f7bab55` / `5ae7f8ad4` (#5161) and `e8de9f8c8` (#5246).
- **Status**: NEW. Searched `explosion_offences` and `keyframe_refusals_logged`: nothing found. Today's `/audit-physics` did not run in this suite.
- **Description**:
  - #5161 added three `RigidBodyHandle`-keyed diagnostics containers in one change. It documented and implemented the removal of exactly one of them: `body_labels` is "Dropped in `Self::remove_body`… the map must not accumulate one stale entry per despawned ragdoll bone".
  - The other two are never pruned, cleared or shrunk anywhere in the crate. `PhysicsWorld` is inserted once at boot (`byroredux/src/boot/world.rs`), so their lifetime is the session.
  - Every production removal path goes through `remove_body`, so it is the single place to fix: `cell_loader/unload.rs:788`, `ragdoll.rs:527`, `npc_spawn/loot_appearance.rs:187`, `crates/physics/src/ragdoll.rs:865` and `commands/ragdoll_status.rs:170`.
  - Keys are full handles (index + generation), so a stale entry can never alias a new body. The only cost is growth; correctness is unaffected.
- **Evidence**:
  ```rust
  if removed {
      // #5161 — the evidence-label dies with the body; the map must
      // not accumulate one stale entry per despawned ragdoll bone.
      self.body_labels.remove(&handle);
      self.wake();
      self.colliders_dirty = true;
  }
  ```
  `explosion_offences` is "deliberately never cleared on clean substeps" (`:335-342`). That rationale is about the escalation ladder for a *live* body, not about a removed one.
- **Impact**: There is no functional impact, only slow growth over a long session that despawns many exploding or refusing ragdolls. The FNV `SLscorpionBurrowINT` repro is the kind of content that drives it. The "Dim 3 CPU unbounded growth" rule asks for this class to be reported.
- **Related**: #5161, #5246, #4772.
- **Suggested Fix**: In `remove_body`'s `if removed` arm, also run `self.explosion_offences.remove(&handle)` and `self.keyframe_refusals_logged.remove(&handle)`. Extend the existing #5161 removal test to assert all three maps are empty after removal.

### SAFE-D4-2026-10-05-01: #4895 added a third renderer `CStr::from_ptr(device_name)` block just as #5120 moved the identical byroredux block to ash's bounded `device_name_as_c_str()`
- **Severity**: LOW. This is hardening and consistency. The block is commented, and its invariant holds for a conforming driver.
- **Dimension**: Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/device.rs:653`, which is new in `7f6ab8e8f` (#4895). The pre-existing siblings are at `:466` and `:481`.
- **Status**: NEW.
  - #5120 (CLOSED, `b7bc84722`) took SAFE-D4-2026-09-29-01's suggested fix in `byroredux/src/app_events.rs`: `selected_gpu.device_name_as_c_str().unwrap_or_default()`.
  - The renderer kept the raw form and gained one more instance.
- **Description**:
  - `CStr::from_ptr` scans for a NUL with no length bound. The spec guarantees `deviceName` is a NUL-terminated `char[VK_MAX_PHYSICAL_DEVICE_NAME_SIZE]`, so the SAFETY comment is true.
  - But the soundness rests on driver conformance, and a non-terminated name would read past the `properties` struct on the stack.
  - ash 0.38's `PhysicalDeviceProperties::device_name_as_c_str()` performs the same conversion through `CStr::from_bytes_until_nul` over the fixed array. That is bounded and needs no `unsafe`.
  - All three blocks feed only `log::warn!` / `log::info!` formatting.
- **Evidence**:
  ```rust
  // SAFETY: device_name is a fixed-size [c_char; 256] array
  // null-terminated by the Vulkan driver. The pointer remains valid
  // while `properties` is in scope.
  let name = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) };
  log::warn!("Rejecting GPU {name:?}: missing required Vulkan features {missing:?}");
  ```
- **Impact**: None on conforming drivers. It adds 3 avoidable `unsafe` blocks to the renderer's count, and the codebase now spells one conversion two ways.
- **Related**: #5120, #4895, SAFE-D4-2026-09-29-01.
- **Suggested Fix**: Replace all three with `properties.device_name_as_c_str().unwrap_or_default()`, the spelling #5120 already uses, deleting three `unsafe` blocks and their SAFETY comments. This improves the existing code rather than adding a helper.

## Cross-referenced findings (not re-filed)

### CONC-D3-2026-10-05-01 (MEDIUM, owned by `/audit-concurrency`): the `vulkan-validation` lane has been red on non-errors since #4987's fix
This audit's Dim 5 confirmed the finding independently. The detail below adds to the concurrency write-up.
- **Cause.** `6d5d8fa5f` (#4987, 2026-10-01) set `RUST_LOG=error,byroredux_renderer=info` (`.github/workflows/ci.yml:398`). `vulkan/debug.rs:69-72` logs the messenger's INFO and WARNING callbacks under the same `[Vulkan]` prefix as ERROR. The fail test at `ci.yml:413` is `grep -qF '[Vulkan]'`. The comments at `:393` ("every [Vulkan] line at this level is a real validation error") and `:412` ("ERROR-severity callback") are false since that commit.
- **What fires.** Each run since `36908954333` (2026-10-01T18:43) emits about 45 `[Vulkan]` lines, with **0** `Validation Error` / `VUID-` / `SYNC-HAZARD` lines:
  - 10 are WARN-level `Validation Performance Warning: [ WARNING-Shader-OutputNotConsumed ]`;
  - the rest are **INFO-level loader chatter** ("Inserted device layer", the `vkCreateDevice` layer callstack, physical-device sort order, `WARNING-cache-file-error`).

  So the noise goes beyond "WARN-level perf warnings", and trimming the unconsumed varyings alone would **not** turn the lane green. The fix must filter by severity: scope the info lift to `byroredux_renderer::vulkan::device`, or grep `ERROR .*\[Vulkan\]`.
- **Second casualty.** Because the bench step exits 1, the job's following step "Assert renderer-static scene-state determinism" has been **skipped** on every run since (`37168582805`: `= skipped`).
- **Timeline.** The 09-30 runs (`36712814522` … `36789466327`) had zero `[Vulkan]` lines and were green. Those runs also confirm **#5062** (ReSTIR reservoir RAW) and **#5064** (caustic `VUID-08114`) are fixed live.
- **Related, also owned by `/audit-concurrency`:** CONC-D2-2026-10-05-01. The #5188 `geometry_dead` arm turns RT off in the CI demo scene (`rt_flag=0 tlas_build=0` since `2c36c29d8`), so even a severity-correct lane no longer exercises the ray-query, caustic and volumetric-TLAS consumers.

### Other existing items touched by this window
- **#4119** (OPEN): `queue_increment_own_i64` still skips the entity-visibility check (Dim 8).
- **#4268** (OPEN): Starfield `.mesh` bone indices pass unbounded. Dim 7 cross-ref only; no new code.
- **Workspace clippy.** The main-CI workspace clippy step is red again on rustc 1.99 `stable`'s new `chunks_exact_to_as_chunks` lint, at 4 renderer sites (`context/depth_capture.rs:100`, `context/screenshot.rs:91`, `groundcover/frame.rs:183`, `pipeline.rs:25`) and in menuxml, hkx, debug-ui and plugin. This is lint drift with no safety content. Since #5121 it **no longer** shadows the renderer `undocumented_unsafe_blocks` gate; see Dim 4.

## Prior-report findings: status at HEAD

| Prior ID | Issue | Status at HEAD |
|---|---|---|
| SAFE-D4-2026-09-29-01 (NVML probe blocks uncommented) | #5120 CLOSED | **Fixed** (`b7bc84722`). The `get_physical_device_properties` block is commented and `CStr::from_ptr` is replaced by `device_name_as_c_str`. The local lint is clean on `byroredux`. |
| SAFE-D4-2026-09-29-02 (clippy red, renderer gate unreached) | #5121 CLOSED | **Fixed in effect** (`530c9e7aa`). `--keep-going` plus a dedicated `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks` step (`if: success() \|\| failure()`, `ci.yml:199-201`). At `37168582805` that step ran and finished clean while the workspace step was red. |
| SAFE-D2-2026-09-29-01 (`push_bytes` bypasses `byte_view`) | #5122 CLOSED | **Fixed.** `unsafe impl NoUninit for ModelPush` (`groundcover_models.rs:145`) plus `byte_view(std::slice::from_ref(&push))` at `:825`. The older `scatter_push_bytes` / `blade_push_bytes` casts moved unchanged to `groundcover/frame.rs:808-827`; they are padding-free and true. |
| CONC-D2-2026-09-29-01 / -02 (cross-ref'd) | #5062 / #5064 CLOSED | Fixed; absent from every lavapipe run since `36712814522`. |
| SAFE-D3-2026-09-21-01 (poison `.expect` in teardown) | #4599 CLOSED | **Fixed** (`e94075d75`): `lock_recovering` / `into_inner_recovering` / `free_allocation_recovering`. Extended by #5209 (`c22fc3f2a`). Pinned by `allocator_lock_direct_acquires_are_confined_to_allocation_and_reports` and `teardown_path_recovers_poisoned_locks_beyond_the_allocator_family`, both green. |
| SAFE-D7-2026-09-23-01 (V-buffer non-finite history) | #4782 CLOSED | Fixed (`17e17de5e`, per the skill). Not re-read in this run. |
| SAFE-D6-2026-09-23-01 (`VolumetricsParams` lane pin) | #4778 CLOSED | Fixed. Not re-read in this run. |
| #4865 (FSR3 Rust/C layout pin, partial at 09-29) | CLOSED | Closed; `fsr3-sys` had no commits in the window. |
| #4896 (`AllocatorResource` order unpinned) | CLOSED | **Fixed** (`0967b9afe`): `allocator_teardown_order_tests` in `app_events.rs` pins both `App::shutdown` and `Drop for App`. Needles are composed at runtime, so the guard is not vacuous. |
| #4889 / #4892 (dynamic-RGBA staging fatal; no ledger row) | CLOSED | Fixed. Staging failures degrade to a skipped overlay frame; Dim 1 verified that soundness. |
| #4987 (lane never reaches a device) | CLOSED | Fixed, but its fix introduced CONC-D3-2026-10-05-01 (above). |

## Per-dimension results

### Dimension 1 — FFI lifetime safety: PASS, 0 new
- **fsr3-sys, frame_upscaler, launcher, cxx-bridge.** Zero commits. `create` and `dispatch` keep their `# Safety` sections (`lib.rs:377/416`). The cxx bridge is still `native_hello() -> String` only.
- **`crates/ui`.** The delta (engine-name membership #4719/#4725, catalog request classification #4720/#4721, player doc and test-closure tweaks) is safe Rust. No raw or borrowed data crosses into the renderer.
- **Dynamic RGBA (#4889 / #4892).** Staging-arena growth failure and mapped-write failure now `return Ok(())`.
  - Updates recorded earlier in the same loop are complete barrier → copy → barrier triples, tagged `recorded_slot = Some(frame)`.
  - The failing update and later ones stay dirty, with `recorded_slot` cleared at the top.
  - Consume-only-on-submit (`submitted(slot)`) is intact.
  - Arena replacement is still behind the runtime fence-idle `ensure!`.
  - Released handles are purged from `updates` (`release.rs:44`, `mod.rs:919`), and #4879 refuses to revive a `ref_count == 0` slot.
- **`recreate_descriptor_sets` (#4886 / #4885 / #5207).** Now transactional:
  - the new pool, sets and samplers are created before the old pool is destroyed;
  - the error arms destroy only the new pool;
  - the sampler swap rewrites `texture.sampler`, and `Texture::destroy` never destroys samplers (`texture.rs:479`), so there is no double free;
  - `texture: None` slots are redirected to the dimension-matched fallback.

### Dimension 2 — Memory corruption / UB: PASS, 0 new
- **First-step grep diffed against the baseline tree.** The only changes are the `ModelPush` fix (#5122) and the groundcover push-constant casts moving files.
- **`buffer.rs` `create_bound_buffer` (#4882).** It is the single create → allocate → bind prologue for every `GpuBuffer` constructor:
  - it destroys the buffer on both unwind arms;
  - a bind failure frees through `free_allocation_recovering`;
  - `create_device_local_buffer`'s `from_raw_parts` (`:1486`) is pre-existing and `T: NoUninit`.
- **Remaining direct allocator acquires.** All are allocation-side or report paths and all are allowlisted: `allocator.rs:341/438`, `buffer.rs:441`, `texture.rs:295`, `context/resources.rs:671`, and `screenshot.rs:308` / `depth_capture.rs:300`.
- **LZ4 pin intact.** `Cargo.toml:165` names `safe-decode` explicitly, and `crates/bsa` is the sole dependent, with `workspace = true`. The only workspace `Cargo.toml` delta is `toml_edit = "0.22"`.
- **pex opcode.** The const assert (`opcode.rs:74`) and the runtime bound (`:137`) are intact.

### Dimension 3 — Leaks and drop ordering: 1 LOW
- **Rapier release.** The guard is green; see Method.
- **Deferred destroy.** Still 4 production `DeferredDestroyQueue<T>` instantiations: `mesh.rs:381`, `scene_buffer/buffers.rs:148` and `acceleration/mod.rs:271/305`. The module doc now says "Four".
- **`AllocatorResource` ordering.** Pinned (#4896).
- **Poison policy.** Landed (#4599 / #5209).
- **`mesh.rs` (#4891).** A failed single scene-mesh upload rolls back its global-pool append, and the slot cap is checked before the append in `upload_scene_mesh_global_only`.
- **New GPU-owning entity sets:**
  - The loading-cover model stage (`e60911864`), plus #5193, retires through `cell_loader::unload::release_entities`. `retired_stage` has one assignment (`cancel`), drained both by `begin` (`loading_screen.rs:295/394`) and by the App poll (`app_step.rs:866`).
  - The mid-life gear release (#5028) uses the same path.
  - `engine.quit` (`bae84e54b`) reaches the same `App::shutdown`.
- **Finding:** SAFE-D3-2026-10-05-01.

### Dimension 4 — Unsafe-block discipline: 1 LOW
- **Guard.** `#![deny(clippy::undocumented_unsafe_blocks)]` is at `crates/renderer/src/lib.rs:21` with no `allow` escape. CI now enforces it independently of other crates (#5121, see the prior-findings table). Locally, every unsafe-bearing crate is clean apart from the 3 nif adjacency false positives.
- **Invariant truth, new or changed blocks.** All hold:
  - `blas_static.rs::unwind_prepared` and the #5201 `may_be_in_flight` arms: nothing is recorded before Phase 4, and `mem::forget` is used on `MaybeInFlight`.
  - `texture_registry/mod.rs` (#4886).
  - `resize.rs` (#4890): old views are destroyed on a `create_swapchain` `Err`, and the old swapchain stays recorded for `Drop`.
  - The groundcover split carried its SAFETY texts with the moved code.
- **Finding:** SAFE-D4-2026-10-05-01 (`device.rs:653`).

### Dimension 5 — Vulkan spec compliance: 1 LOW (+1 MEDIUM cross-referenced)
- **Live validation.** See CONC-D3-2026-10-05-01 above. No `Validation Error` / VUID / SYNC line appears in any lavapipe run since 2026-09-30, but the CI demo scene runs with RT off (CONC-D2-2026-10-05-01). Not a single game-data path is covered.
- **AS scratch (#5195).** Scratch is sized to `max(build, update)`:
  - `blas_skinned.rs:189-191`;
  - `tlas.rs:1061`;
  - the shrink peak, through `scratch_requirement()` (`memory.rs:74-75`);
  - the refit check at `blas_skinned.rs:572` is a `debug_assert`, but the sizing invariant is structural: every grow path and the shrink walk use the max.
- **#4884.** The static batch allocates the replacement scratch before retiring the old one.
- **TLAS UPDATE == BUILD.** `decide_use_update` is unchanged. The #4633 / #5200 non-finite instance drops change the address list, which forces a BUILD.
- **Other pins.** The depth-format (#3570) and far-plane (#3611) pins and the SPIR-V reflection tests are green. #4888, #4890 and #4895 are closed with their code present.
- **Finding:** SAFE-D5-2026-10-05-01.

### Dimension 6 — GPU struct layout: PASS, 0 new
- **Size pins:**

  | Struct | Size | Notes |
  |---|---|---|
  | `GpuMaterial` | 432 B | |
  | `GpuInstance` | 160 B | |
  | `GpuCamera` | 368 B | |
  | `GpuLight` | **64 B** | was 80; #5055 / #4784 moved light identities CPU-side. Four-way GLSL lockstep plus `light_ssbo_docs_state_the_real_header_and_entry_size`. |
  | `GpuTerrainTile` | **176 B** | was 160; `TERRAIN_SPLAT_LAYERS`, #5113 / #5173. Field-offset pin green. |
  | `SelectedRayProbe` | 144 B | |
- **Offset and name pins.** Green for `GpuMaterial` and `GpuInstance`. `every_shader_struct_is_classified` covers all four mirrored structs.
- **Material ID bounds.** The `MAX_MATERIALS` intern cap (`material.rs:1298`) and the `upload_materials` `debug_assert` plus `.min(MAX_MATERIALS)` (`upload.rs:903-909`) are in lockstep.

### Dimension 7 — GPU-fed data and loop bounds: PASS, 0 new
- **Glass loop.** Bounded at `triangle.frag:2317`, and the pin test is green.
- **#3991 latch.** `app_frame.rs:727` reads `skin_state_submitted`. That is set only in `promote_skin_frame_state` (`skinned_blas_refit.rs:45`), which is called post-submit (`draw.rs:656`) and reset at `draw.rs:120`.
- **Bone palette.** The overflow tests are green.
- **New `Material` producer.** The loading-cover stage goes through `scene::load_nif_bytes`, which reaches `translate_material`.
- **Clippy sweeps.** The two sweeps (`3f0852e08`, `4ad847a81`) were scanned for NaN-semantics rewrites. The only `!(a < b)` removals are test asserts, one of them repaired by `fd26378da`. No `.max().min()` → `.clamp()` rewrite was introduced; that rewrite would panic on NaN or inverted bounds.

### Dimension 8 — Sandboxed mod runtime: unchanged (skimmed), 0 new
- **WASI.** None in the tree. The `wasmtime` feature list is unchanged (`Cargo.toml:119`).
- **Gating.** 22 `require_` guards, unchanged.
- **Path delta.** Only test-file clippy edits.
- **#4119** is still open.

## Skill-sync notes (for the next `/audit-safety` edit, not findings)
- The census text in the skill matches HEAD (939 / 720 / 96 / 35).
- **Dim 5's lane caveat understates the noise.** It says the info lift "admits WARN-level `[Vulkan]` performance warnings". It also admits INFO-level loader chatter, and the lane exits red on it every run, skipping the determinism step. The caveat should point at CONC-D3-2026-10-05-01 until that is fixed.
- **`_audit-common.md` layout traps.** Add `crates/renderer/src/vulkan/groundcover.rs` + `groundcover/` as a file+dir pair; the `.rs` keeps the struct and `NoUninit` impls, and the directory holds the construct/frame bodies.
- **Dim 7.** Could note `PhysicsWorld`'s per-body diagnostics maps as an owned-by-`/audit-physics` growth surface.

Publish with: `/audit-publish docs/audits/AUDIT_SAFETY_2026-10-05.md`
