**HEAD**: `9fcfdc3fc` · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-09-21.md` (@ `f97775ca8`, 312 commits ago; the intervening `AUDIT_SAFETY_2026-09-23.md` @ `2237da9c3` was area-scoped to volumetrics and is used for dedup only) · **Audited**: Dims 1–8 (every dimension's `Paths:` had commits since the baseline) · **Unchanged since baseline (skimmed)**: none at the dimension level. Within Dim 8, only `extensions/commands.rs` (#4702), one `content_catalog.rs` enum arm and the workspace `Cargo.toml` moved. The gating surface (`runtime/capabilities.rs`, the other `runtime/host/*.rs`, `extensions/{install,dispatch}.rs`) had zero commits and got a guard spot-check only.

# Safety Audit — ByroRedux — 2026-09-29

**Command**: `/audit-safety` (default scope), one leg of `/audit-suite --preset comprehensive`
**Severity scale**: `.claude/commands/_audit-severity.md`

## Method

- **Scoping.** Delta-scoped against `f97775ca8`. Commits per dimension's `Paths:`: D1 11, D2 12, D3 35, D4 82, D5 82, D6 21, D7 46, D8 3.
- **No sub-agents.** Every dimension was analysed synchronously. Per-dimension notes are in `/tmp/audit/safety/dim_{0..8}.md` (`dim_0` is the census). This report was reconciled against each of them.
- **Commands run.** No engine or GPU process was launched, and no source was edited.
  - `cargo test -p byroredux -- rapier_release bone_palette_overflow`: **13 passed, 0 failed**. This covers 9 `rapier_release_tests`, including `release_sweeps_both_ragdoll_and_rapier_handles`, and 4 `bone_palette_overflow_tests`.
  - `cargo test -p byroredux-renderer --lib -- gpu_material gpu_instance gpu_camera gpu_light gpu_terrain volume_far depth_capture shader_constants triangle_frag_scales bindings_glsl the_pin_test name_diverging scene_descriptor_reflection`: **114 passed, 0 failed**. Every Dim 6 size/offset pin, the #3570 depth-format guard, the #3611 far-plane lockstep, the glass loop-bound pin and the SPIR-V reflection tests ran.
  - Unsafe-documentation lints on every unsafe-bearing crate, with the local toolchain (rustc/clippy 1.96): `cargo clippy -p byroredux-renderer -p byroredux-fsr3-sys -p byroredux-nif -p byroredux-core -p byroredux-pex -p byroredux --no-deps -- -A clippy::all -W clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc`.
    - renderer, fsr3-sys, core and pex: clean.
    - nif: the 3 known `unsafe impl` adjacency false positives (`bs_geometry.rs:364-366`).
    - byroredux: 3 warnings, reported as SAFE-D4-2026-09-29-01.
  - `cargo tree -p byroredux-mod-runtime | grep -i wasi`: prints nothing.
- **CI evidence.** Read with `gh run view` from HEAD's CI run `36609043348`:
  - job `109545443996`, "Test + Check + Clippy";
  - job `109545444434`, "Vulkan validation layers (lavapipe)";
  - the clippy-step history of the previous 60 runs on main.
- **Dedup.** Checked against:
  - `/tmp/audit/issues.json` (163 open issues);
  - `gh issue list --state all` searches: `clippy`, `byte_view`, `app_events unsafe SAFETY`, `NVML`, `reservoir barrier`, `08114`, `SYNC-HAZARD`, and the prior SAFE IDs;
  - today's sibling reports (`AUDIT_{ECS,PERFORMANCE,RENDERER}_2026-09-29.md`);
  - the in-flight concurrency scratch (`/tmp/audit/concurrency/dim_2.md`).

## Census (re-measured at HEAD)

| Crate | Word tokens | `unsafe {` blocks | declared `unsafe fn` | `unsafe impl` | Notes |
|---|---|---|---|---|---|
| `crates/renderer/src` | 946 (09-21: 891) | 733 (683) | 95 | 34 | +50 blocks, all accounted for (below) |
| `crates/fsr3-sys` | 12 | 10 (1 test-only) | 2 `pub` (`create`, `dispatch`), both with `# Safety` | 0 | new test-only `byro_fsr3_abi_layout` probe |
| `crates/nif` | 13 | 1 (`read_pod_vec_from`) | 0 | 1 macro + 3 (`bs_geometry.rs`) | site rewritten by #4594/#4796 |
| `crates/core` | 7 | 6 (4 `query.rs`, 1 `string/mod.rs`, 1 test) | 0 | 0 | no commits |
| `crates/pex` | 4 | 1 (`transmute`) | 0 | 0 | no commits |
| `byroredux` | 6 | 3 | 0 | 0 | **2 new, uncommented** (`app_events.rs`, 0925f7926) |
| `tools/byro-launcher` | 3 | 1 | 1 (`describe`) | 0 | #4598 fixed |
| `crates/plugin` | 4 | 2 (test-only env edits) | 0 | 0 | |
| `crates/cxx-bridge` | 1 | 0 (one `unsafe extern "C++"`) | 0 | 0 | still only `native_hello() -> String` |

- **Where the +50 renderer blocks are:**
  - `vulkan/gpu_timers.rs` +19 (43→62);
  - `vulkan/groundcover_models.rs` +16 (a new file, #4413);
  - `vulkan/pipeline.rs` +3;
  - `vulkan/texture.rs` +2;
  - `context/begin_frame_recording.rs` +2;
  - +1 each in `vulkan/allocator.rs`, `texture_registry/dynamic_rgba.rs`, `vulkan/restir.rs`, `context/geometry_pass.rs`, `context/resize.rs`, `context/teardown.rs`, `context/init.rs` and `acceleration/tests/predicates_tests.rs`.
- **No `unsafe` newcomer** in any previously unsafe-free crate. Only `crates/sdk` carries `#![forbid(unsafe_code)]`; the `crates/bsa/src/ba2.rs` hit is a comment.
- **New FFI-adjacent dependency:** `nvml-wrapper = "0.11"` (0925f7926), which dlopens `libnvidia-ml`. It is used only through its safe API, in `byroredux/src/systems/metrics.rs`, where `MetricsState` owns an `Nvml` that is dropped with the `World`. No `unsafe` sits at our boundary. The skill's line "`crates/fsr3-sys` is the workspace's only real FFI crossing" is now true only of *our own* code; flagged for skill sync.

## Findings summary

| ID | Severity | Dim | Status | Title |
|---|---|---|---|---|
| SAFE-D4-2026-09-29-01 | MEDIUM | 4 | NEW | Two `unsafe` blocks in `byroredux/src/app_events.rs` (the NVML GPU-name probe) have no SAFETY comment |
| SAFE-D4-2026-09-29-02 | MEDIUM | 4 | Regression of #4595 | CI clippy is red on rustc 1.98.1 in `byroredux-sdk` and `byroredux-nif`, so the run aborts before the renderer is checked; `#![deny(clippy::undocumented_unsafe_blocks)]` is enforced nowhere in CI |
| SAFE-D2-2026-09-29-01 | LOW | 2 | NEW | `groundcover_models.rs::push_bytes` hand-rolls a `from_raw_parts` byte view that bypasses `byte_view` / `NoUninit` |

Counts: **0 CRITICAL · 0 HIGH · 2 MEDIUM · 1 LOW** (2 NEW, 1 regression).

**Cross-referenced, not re-filed.** The first live validation-layer evidence since #4596 shows two HIGH-class Vulkan errors. They are owned by today's concurrently running `/audit-concurrency`, as CONC-D2-2026-09-29-01 and CONC-D2-2026-09-29-02; see *Cross-referenced findings*.

## Findings

### SAFE-D4-2026-09-29-01: Two `unsafe` blocks in `byroredux/src/app_events.rs` (the NVML GPU-name probe) have no SAFETY comment
- **Severity**: MEDIUM (special-rule floor: an `unsafe` block without a safety comment).
- **Dimension**: Unsafe-Block Discipline
- **Location**: `byroredux/src/app_events.rs:236-241`, in `resumed`. Introduced by 0925f7926 ("feat: extend GPU memory and workload telemetry", 2026-09-26).
- **Status**: NEW. No open or closed issue matches (`app_events unsafe SAFETY`, `NVML`). The skill's census names these blocks as uncommented but no issue was ever filed.
- **Description**: The NVML adapter-matching code reads the selected physical device's properties and wraps `device_name` in a `CStr`. It does this in two bare `unsafe` blocks. Neither has a `// SAFETY:` comment, and the `byroredux` crate has no `undocumented_unsafe_blocks` lint, so nothing flags them. That lint lives only in the renderer; see SAFE-D4-2026-09-29-02 for why even that one is currently inert in CI.
  - The invariants do hold:
    - `ctx` was created a few lines earlier, so `ctx.instance` and `ctx.physical_device` are live;
    - the spec guarantees `VkPhysicalDeviceProperties::deviceName` is a NUL-terminated `char[VK_MAX_PHYSICAL_DEVICE_NAME_SIZE]`.
  - The finding is therefore the missing justification, not a false invariant.
  - The renderer already performs the same two calls with SAFETY comments: `crates/renderer/src/vulkan/device.rs:454-456`.
- **Evidence**:
  ```rust
  let selected_gpu = unsafe {
      ctx.instance
          .get_physical_device_properties(ctx.physical_device)
  };
  let selected_gpu_name =
      unsafe { std::ffi::CStr::from_ptr(selected_gpu.device_name.as_ptr()) }
          .to_string_lossy();
  ```
  The local lint run reports `warning: unsafe block missing a safety comment` at `app_events.rs:236:36` and `:241:21`. A third warning, at `cell_loader/unload.rs:452`, is adjacency only: its SAFETY comment sits above the enclosing `if let`.
- **Impact**: None at runtime today. This is discipline debt on the engine's startup path, and it grows the uncommented tail that only a hand sweep can find.
- **Related**: #1904 (the renderer lint), SAFE-D4-2026-09-29-02.
- **Suggested Fix**: Improve the existing code rather than duplicating it: either
  - have the renderer expose the selected device's `vendor_id` and name, which `device.rs` already reads, on `VulkanContext`; or
  - use ash 0.38's `PhysicalDeviceProperties::device_name_as_c_str()`, which removes the `CStr::from_ptr` block.

  Document whichever `get_physical_device_properties` block remains.

### SAFE-D4-2026-09-29-02: CI clippy is red on rustc 1.98.1 in `byroredux-sdk` and `byroredux-nif`, so the run aborts before the renderer is checked; `#![deny(clippy::undocumented_unsafe_blocks)]` is enforced nowhere in CI
- **Severity**: MEDIUM. This is the same class as SAFE-D4-2026-09-21-01 (#4595): the only mechanical guard on the renderer's 733 `unsafe` blocks is off.
- **Dimension**: Unsafe-Block Discipline
- **Location**:
  - `.github/workflows/ci.yml:130` (`dtolnay/rust-toolchain@stable`, unpinned) and `:177-179` (`cargo clippy --workspace -- -D warnings`, no `--keep-going`);
  - the new-lint sites `crates/sdk/src/event.rs:447` and `crates/nif/src/{anim/controlled_block.rs:97, blocks/bs_geometry.rs:442/455/462, blocks/legacy_particle.rs:687, blocks/node.rs:1231, blocks/skin.rs:502, import/mesh/bs_tri_shape.rs:193, import/mesh/normal.rs:127, import/mesh/skin.rs:75, import/types.rs:1531}`.
- **Status**: Regression of #4595 (CLOSED 2026-09-22). #4595's remedy still holds: clippy runs even when tests are red. But the gate it restored no longer reaches the renderer. #4765 (CLOSED 2026-09-29, 554ef5c44) greened clippy on the local 1.96 toolchain only. No open issue covers this.
- **Description**:
  - CI installs `stable`, currently **rustc 1.98.1** (`48a229cea 2026-09-01`). The workstation runs 1.96.0, and the repo has no `rust-toolchain.toml`.
  - Clippy 1.98 adds `chunks_exact_to_as_chunks` and fires `question_mark` on one more pattern, giving 1 error in `byroredux-sdk` and 11 in `byroredux-nif`.
  - Cargo stops scheduling once those crates fail. The log shows `Compiling byroredux-renderer` (its build script) but never `Checking byroredux-renderer`, so the renderer's crate-level `#![deny(clippy::undocumented_unsafe_blocks)]` (`crates/renderer/src/lib.rs:21`) is never evaluated.
  - The clippy step has concluded `failure` on every main run in the sampled history, from `839b8dcea` (2026-09-25T14:08Z) through HEAD. The per-run causes before HEAD were not individually read; at HEAD, after #4765 landed, the only errors are the 12 rustc-1.98 lint sites below.
- **Evidence**: HEAD run `36609043348`, job `109545443996`, step "cargo clippy":
  ```
  error: using `chunks_exact` with a constant chunk size
     --> crates/sdk/src/event.rs:447:23
  error: could not compile `byroredux-sdk` (lib) due to 1 previous error
  error: this block may be rewritten with the `?` operator
     --> crates/nif/src/anim/controlled_block.rs:97:16
  … (10× chunks_exact_to_as_chunks in byroredux-nif)
  error: could not compile `byroredux-nif` (lib) due to 11 previous errors
  ```
  The same step printed `Checking` lines for 15 workspace crates (plus `Compiling` for the build scripts of fsr3-sys, cxx-bridge and renderer), and `byroredux-renderer` is never `Checking`.
- **Impact**: A comment-less `unsafe {}` added to the renderer today would pass CI. The local lint run above shows the renderer is clean at HEAD, so no such block exists yet. Every other clippy-enforced invariant in crates downstream of `sdk`/`nif` is also unchecked, and the job is permanently red, which trains readers to ignore it.
- **Related**: #4595, #4765, #4567 (the previous toolchain-bump red), SAFE-D4-2026-09-21-01. Memory note *Clippy --keep-going*: workspace clippy aborts at the first failing crate.
- **Suggested Fix**:
  - Fix the 12 sites. `as_chunks::<N>().0` is the suggested rewrite; `?` for `controlled_block.rs:97`.
  - Stop a toolchain bump from silently shadowing the renderer gate, by either:
    - pinning the CI toolchain with a `rust-toolchain.toml` that the workstation shares; or
    - adding `--keep-going` plus a dedicated `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks` step that cannot be pre-empted by an unrelated crate.

### SAFE-D2-2026-09-29-01: `groundcover_models.rs::push_bytes` hand-rolls a `from_raw_parts` byte view that bypasses `byte_view` / `NoUninit`
- **Severity**: LOW. The view is sound today; this is a hardening and consistency gap.
- **Dimension**: Memory Corruption / UB
- **Location**: `crates/renderer/src/vulkan/groundcover_models.rs:125-149` (`ModelPush` at :127, `push_bytes` at :140) (`ModelPush`, `push_bytes`). New in aabd99a05 (#4413).
- **Status**: NEW. #4445 and #4521 (both CLOSED) made `buffer::byte_view` the single sanctioned `T → &[u8]` path and removed hand-rolled siblings. They covered SSBO and hash views, not push constants. This is the only *new* `from_raw_parts` site in the window; the baseline-vs-HEAD grep diff is in `dim_2.md`.
- **Description**: `ModelPush` is `#[repr(C)]`: 2 × `[f32; 4]` plus 8 four-byte scalars, 64 B with no padding. Its size is pinned by `host_mirrors_match_the_shader_strides`, and its field order matches GLSL `GcModelPush` field for field. So the SAFETY text ("every byte is initialised") is true. But the proof lives in prose. `ModelPush` does not `impl NoUninit`, so a future field that introduces padding (for example a `u16` or `bool` lane) compiles silently, and the pushed range then includes uninitialised bytes. That is the exact hazard the `NoUninit` gate exists to stop at compile-review time.

  Several pre-existing push-constant casts use the same shape (in `svgf.rs`, `skin_compute.rs`, `morph_compute.rs`, `water.rs`, `presentation.rs`, `groundcover.rs` and `groundcover_bench.rs`). None is new.
- **Evidence**:
  ```rust
  fn push_bytes(push: &ModelPush) -> &[u8] {
      // SAFETY: `#[repr(C)]` over 4-byte scalars with no padding, so every byte
      // is initialised; the slice borrows `push`.
      unsafe {
          std::slice::from_raw_parts(
              (push as *const ModelPush).cast::<u8>(),
              std::mem::size_of::<ModelPush>(),
          )
      }
  }
  ```
- **Impact**: None today. The regression shape is latent UB (reading uninitialised padding into `vkCmdPushConstants`) that no test would notice.
- **Related**: #4445, #4521, #3990, Dim 6 layout pins.
- **Suggested Fix**: Add `unsafe impl NoUninit for ModelPush {}`, with the same one-line padding proof its sibling `GpuGroundCoverModelRecord` carries. Then call `buffer::byte_view(std::slice::from_ref(&push))` and delete `push_bytes`. Optionally sweep the older push-constant casts onto the same path in one follow-up.

## Cross-referenced findings (not re-filed)

### CONC-D2-2026-09-29-01 / -02 (HIGH, owned by `/audit-concurrency`): first live validation errors
The `vulkan-validation` lane now reaches a device. 6d05c2bc0's ICD glob resolves `/usr/share/vulkan/icd.d/lvp_icd.json`, the bench reports `rt_supported=1 tlas_build=1` and `bench exit status: 0`, and job `109545444434` fails on 35 `[Vulkan]` lines in two classes. Dim 5 of this audit independently confirmed both in code, but GPU sync is `/audit-concurrency`'s domain and its in-flight run already records them.

1. **20× `SYNC-HAZARD-READ-AFTER-WRITE`**, set-1 binding #16 (`ReservoirCurrBuffer`). The verbatim message: "usage: SYNC_FRAGMENT_SHADER_SHADER_STORAGE_READ, prior_usage: SYNC_COPY_TRANSFER_WRITE … command: vkCmdFillBuffer".
   - Cause: the `after` barrier in `ReservoirBuffers::begin_frame` (`crates/renderer/src/vulkan/restir.rs:143-145`, from 186234944) publishes the fill to `SHADER_WRITE` only.
   - Tracked as CONC-D2-2026-09-29-01.
2. **10× `VUID-vkCmdDispatch-None-08114`**, set 0 binding 9, per-FIF sets. The verbatim message: "has never been updated via vkUpdateDescriptorSets()".
   - By elimination this is the caustic splat set. `caustic.rs` writes 9/10 only through `write_geometry_buffers`, which `sync_and_acquire_frame.rs` calls only when a global vertex/index SSBO exists (the content-free CI demo has none).
   - The layout has no `PARTIALLY_BOUND`, and `post_passes.rs` gates the dispatch on a TLAS only.
   - Tracked as CONC-D2-2026-09-29-02.

Separately, **#4987** (the lane never reaches a device) is still OPEN, but its premise no longer holds at HEAD. That is a close-out note for `/audit-publish`, not a finding.

### Other existing items touched by this window
- **#4599** (MEDIUM, open): `.expect("allocator lock poisoned")` is reachable from `Drop`/teardown. No new such site was added.
- **#4865** (open): the FSR3 Rust/C layout pin. ab255cfd2 added the `byro_fsr3_abi_layout` probe and C++ `static_assert`s, which pin 13 sizes/offsets. Same-typed runs such as `jitter_x/y` and `camera_near/far` could still transpose unseen, so the issue is partially addressed.
- **#4896** (no pin for `AllocatorResource` removal order): the order itself is intact in both `App::shutdown` and `impl Drop for App`.
- **#4889 / #4892**: the dynamic-RGBA staging failure is fatal to `draw_frame`, and the arena has no ledger row.
- **REN-D5-2026-09-29-02**: the player's mid-life gear imports are never released. Filed today by `/audit-renderer`.
- **#4782 / #4863 / #4633 / #4268**: non-finite inputs to GPU-fed data. The code is unchanged.
- **#4119** (open): `queue_increment_own_i64` still skips the entity-visibility check.

## Prior-report findings: status at HEAD

| Prior ID | Issue | Status at HEAD |
|---|---|---|
| SAFE-D1-2026-09-21-01 (FSR recovery blit layout) | #4592 CLOSED | Fixed. All three `record_native_blit` calls pass `inputs.scene_color_layout` (#4976 closed the third arm), pinned by `every_native_blit_sources_from_the_scene_images_actual_layout`. |
| SAFE-D2-2026-09-21-01 (staging released at footprint) | #4593 CLOSED (+#4790) | Fixed at all sites plus the terrain ring, with source-scan pins. |
| SAFE-D2-2026-09-21-02 (`read_pod_vec_from` `Read` contract) | #4594 CLOSED | Fixed: concrete `Cursor<&[u8]>`, checked slice, `copy_nonoverlapping`, then `set_len`. |
| SAFE-D4-2026-09-21-01 (clippy gate not run) | #4595 CLOSED | **Regressed in effect**: SAFE-D4-2026-09-29-02. |
| SAFE-D5-2026-09-21-01 (lavapipe lane never reaches Vulkan) | #4596 CLOSED → #4987 OPEN | The lane now reaches lavapipe; its errors are cross-referenced above. |
| SAFE-D7-2026-09-21-01 (auto-exposure divisor) | #4597 CLOSED | Not re-examined (renderer Dim 11). |
| SAFE-D1-2026-09-21-02 (preflight `VkInstance` leak) | #4598 CLOSED | Fixed: destroyed on the enumeration-error path. |
| SAFE-D3-2026-09-21-01 (poison `.expect` in teardown) | #4599 OPEN | Unchanged. |
| SAFE-D4-2026-09-21-02 (stale SAFETY texts) | #4600 CLOSED | Fixed (9182b3961). |
| SAFE-D5-2026-09-23-01 (froxel extent vs `maxImageDimension3D`) | #4781 CLOSED | Fixed: `max_image_dimension_3d` is passed at both create sites (`context/init.rs:1081`, `context/resize.rs:885`). |
| SAFE-D7-2026-09-23-01 (V-buffer non-finite history) | #4782 OPEN | Unchanged. |
| SAFE-D6-2026-09-23-01 (`VolumetricsParams` lane pin) | #4778 OPEN | Unchanged. |

## Per-dimension results

### Dimension 1 — FFI lifetime safety: PASS, 0 new
- **fsr3-sys.** `create` and `dispatch` keep their `# Safety` sections. The ABI probe is test-only. Field order in `RawCreateDesc`, `RawImage` and `RawDispatchDesc` matches `byro_fsr3.h` exactly.
- **frame_upscaler.** #4592 and #4976 are fixed, and the `# Safety` text now states the caller-declared `source_layout` contract, which is true at all three call sites.
- **Launcher.** #4598 is fixed.
- **`crates/ui`.** Still safe Rust. The delta (render pacing, canonical menu names, a poison-recovering init lock) passes no raw or borrowed data into the renderer.
- **cxx-bridge.** Unchanged.
- **nvml-wrapper.** Safe API only; see the census.

### Dimension 2 — Memory corruption / UB: 1 LOW
- **Diff against the baseline tree.** The first-step grep (`transmute`, `from_raw_parts`, `set_len`, `from_utf8_unchecked`, `unsafe impl`) finds only these new sites:
  - `groundcover_models.rs`: two `NoUninit` impls, both proofs true, pinned by `host_mirrors_match_the_shader_strides` and `name_diverging_glsl_rust_mirrors_stay_in_lockstep`;
  - `push_bytes` (SAFE-D2-2026-09-29-01);
  - the rewritten nif POD site (#4594/#4796), which is sound.
- **`GpuBuffer::write_mapped_at` (new).** It goes through `byte_view`, bounds-checks with `checked_add` and `get_mut(range)`, then flushes.
- **LZ4 pin intact.** `safe-decode` and `checked-decode` are in the explicit feature list, and `crates/bsa` depends on it with `workspace = true`.
- **pex opcode bound intact.**

### Dimension 3 — Leaks and drop ordering: PASS, 0 new
- **Deferred destroy.** There are still four production `DeferredDestroyQueue<T>` instantiations.
- **`AllocatorResource`.** Removed before `renderer.take()` on both teardown paths.
- **New resource owners:**
  - `GroundCoverModelTier`: partial-create cleanup, a full `destroy`, and a teardown call.
  - The dynamic-RGBA per-FIF arenas: replaced only after a runtime-checked fence-idle slot, and destroyed at registry teardown.
  - `GpuPerFrameTimers`: releases partially created timestamp and pipeline-statistics pools (#4998). Its opaque begin/end query bracket is balanced on both exits.
  - egui `pending_free`: intact.
- **Physics.** The window's Rapier changes do not touch body or collider ownership, and the `rapier_release` guard is green.

### Dimension 4 — Unsafe-block discipline: 2 MEDIUM
- **Guard present.** `deny` is at `lib.rs:21` with no `allow` escape, and the renderer is clean under the lint locally. But CI never reaches it: SAFE-D4-2026-09-29-02.
- **Invariant truth.** Every new or changed renderer block was read against its call site; all hold. Notes:
  - `begin_frame_recording`: on error it leaves the command buffer recording, and the next frame resets it before `begin`.
  - `dynamic_rgba`: the fence-idle precondition is a runtime `ensure!`, not only a comment.
  - `pipeline.rs`: modules and partial pipelines are destroyed on every failure path, and `opaque_early` is destroyed with its siblings.
  - `texture.rs`: #4854 cleanup precedes recording, and bind precedes the view.
  - `resize.rs` / `init.rs`: the `maxImageDimension3D` query.
- **Outside the renderer:** SAFE-D4-2026-09-29-01.

### Dimension 5 — Vulkan spec compliance: 0 new (2 HIGH cross-referenced)
- **Live validation.** See *Cross-referenced findings*.
- **Code checks (pass):**
  - TLAS UPDATE requires an identical address list and entity membership (#4948), so the UPDATE count equals the BUILD count.
  - The #3570 depth-format guard, the #3611 far-plane lockstep and the SPIR-V reflection tests pass.
  - New objects are gated on their features: the early-test pipeline; the model tier behind `indirect_draws_supported()` with drawCount 1; the statistics pool behind `pipelineStatisticsQuery`; and `VK_EXT_memory_budget` only when enabled.
- **Still open:** #4888, #4890, #4895.

### Dimension 6 — GPU struct layout: PASS, 0 new
- **Pins.** Every size and offset pin passes: `GpuMaterial` 432 B, `GpuInstance` 160 B, `GpuCamera` 368 B, `GpuLight` 80 B, `GpuTerrainTile` 160 B.
- **New GPU-side `GpuInstance` producer (the model tier's tail).** It is bounds-safe:
  - host `tail_capacity = instance_capacity(frame) − main_instances`, clamped;
  - the grow is requested for main plus tail before upload;
  - the shader's `min(want, tailCapacity − cursor)` keeps every slot below `tailBase + tailCapacity`;
  - the previous-model buffer grows in lockstep.
- **`material_id`** comes from host-interned `DrawCommand`s.

### Dimension 7 — GPU-fed data and loop bounds: PASS, 0 new
- **Glass loop.** Bounded by `MAX_REFRACT_PASSTHRUS` (`triangle.frag:2321`).
- **#3991 latch.** Intact: `app_frame.rs:667`, set post-submit in `promote_skin_frame_state`.
- **New `Material` producers.** The #4632 texture-only MSN wrapper, the mid-life gear import and the player body all route through `translate_material` or the existing texture-only inner function.
- **Bone palette.** Overflow tests are green.

### Dimension 8 — Sandboxed mod runtime: PASS, 0 new
- No WASI in the tree, and the `wasmtime` feature list is unchanged.
- The gating surface had no commits: 22 `require_` guards.
- #4702's post-write death commit is a host-side consequence of an already-gated batch; it adds no new guest entry point.
- #4119 is still open.

## Skill-sync notes (for the next `/audit-safety` edit, not findings)
- The census text is current at HEAD (946 / 733 / 95 / 34). The byroredux line should name the three real blocks: `app_events.rs` ×2 and `unload.rs` ×1. `unload.rs` has a comment that clippy misses by adjacency.
- Dim 1's "only real FFI crossing" should mention `nvml-wrapper` as a third-party dlopen crossing on the startup path.
- Dim 4 should record that CI's clippy toolchain is unpinned `stable`, and that a single red upstream crate hides the renderer gate.
- Dim 5's "Known-open #4987" is stale: the lane reaches lavapipe at HEAD and is red on real errors.

Publish with: `/audit-publish docs/audits/AUDIT_SAFETY_2026-09-29.md`
