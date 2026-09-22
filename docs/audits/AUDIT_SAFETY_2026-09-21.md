**HEAD**: `f97775ca8` · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-09-11.md` (@ `b3db49fa`, 385 commits ago) · **Audited**: Dims 1–8 (every dimension's `Paths:` had commits since the baseline) · **Unchanged since baseline (skimmed)**: none at the dimension level. Within Dim 8, only the WIT enum and Cargo.toml moved. The gating surface (`crates/mod-runtime/src/runtime/host/`, `runtime/capabilities.rs`, `byroredux/src/extensions/`) had zero commits, so it got a guard spot-check only.

# Safety Audit — ByroRedux — 2026-09-21

**Command**: `/audit-safety` (default scope), one leg of `/audit-suite --preset comprehensive`
**Severity scale**: `.claude/commands/_audit-severity.md`

## Method

- I delta-scoped against the 09-11 baseline and ran each dimension myself in order, with no sub-agents. Per-dimension notes are in `/tmp/audit/safety/dim_{1..8}.md`.
- **Renderer work split.** The 2026-09-20 renderer audit (HEAD `052891f22`) already ran live sync validation. For the renderer I therefore read two things:
  - the 25 renderer commits after `052891f22`: exposure meter, AgX/auto-exposure, BFECC transported field, SSAO kernel, caustic visibility gates, MSN basis, and debug modes;
  - the safety-specific invariants across the whole window.
- **Commands run (this checkout, low-parallelism):**
  - `cargo clippy -p byroredux-renderer --no-deps -- -A clippy::all -D clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc`: clean.
  - The same lint as a warning on nif/core/pex/fsr3-sys: only the 3 known adjacency false positives at `crates/nif/src/blocks/bs_geometry.rs:350-352`.
  - `cargo test -p byroredux-renderer --lib`, two filtered runs over the layout/contract/caustic/glass pins: 117 + 46 tests (the filters overlap), 0 failures.
  - The prebuilt `byroredux` test harness (built after `d54382415`; HEAD's only later commit touches renderer files), filtered to `rapier_release` and `bone_palette_overflow`: 13 pass.
  - `cargo tree -p byroredux-mod-runtime`: no WASI.
- **CI evidence.** Read with `gh run view` / `gh api` on main-branch runs from 2026-08-31 to 2026-09-21, including HEAD's run `35658431384`.
- **Engine launches.**
  - I made no intentional engine launch. Vulkan-spec claims below are code evidence, flagged where they need a validation-layer run.
  - **Process note:** while probing `target/debug/deps/byroredux-*` for a test harness, I invoked two *engine* binaries (`byroredux-98e91fabef5cd6a8`, `byroredux-b04d3d92c9fc2333`) with `--list`. Each ran about 60 s under `timeout 60` at around 21:55–21:57Z and was killed. A search of the repo, `~/.config`, `~/.local/share`, `~/.cache` and `/tmp` for that window found no files written by them.
- **Dedup.** Checked against `/tmp/audit/issues.json` (4,458 issues, 94 open), `gh issue view` for #4512, #4538 and #4567, and `docs/audits/`.

## Census (re-measured at HEAD)

| Crate | Word tokens | `unsafe {` blocks | `unsafe fn` | `unsafe impl` |
|---|---|---|---|---|
| `crates/renderer/src` | 891 (09-19: 879) | 683 (678) | 136 (133) | 36 (33) |
| `crates/fsr3-sys` | 11 | 9 | 2 | 0 |
| `crates/nif` | 14 | 2 | 0 | 6 |
| `crates/core` | 7 | 6 (4 `ecs/query.rs`, 2 `string/mod.rs`; :302 is `#[cfg(test)]`) | 0 | 0 |
| `crates/pex` | 4 | 1 (transmute) | 0 | 0 |
| `crates/plugin` | 4 | 2 (test env edits) | 0 | 0 |
| `byroredux` | 3 | 1 (`cell_loader/unload.rs:438`) | 0 | 0 |
| `tools/byro-launcher` | 3 | 1 | 1 | 0 |

- **Renderer growth** comes from `exposure_meter.rs` (new), `sky_cube.rs` + `sky_cube/{filter,irradiance}.rs`, `cloud_noise.rs`, `volumetrics/noise.rs`, the gpu_timers brackets, `Texture::overwrite_rgba_pixels`, and `SkinComputePipeline::bind`. The renderer has about 777 SAFETY mentions.
- **No `unsafe` at all** in bsa, save, scripting, sdk, ui, facegen, sfmaterial, mod-runtime, hkx, bgsm, **menuxml (new crate)**, physics or audio. Only `crates/sdk` carries `#![forbid(unsafe_code)]`. `crates/bsa/src/ba2.rs` only *mentions* lz4_flex's `forbid(unsafe_code)` in prose.

## Findings summary

| Severity | NEW / regression | Matched to open issues |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 2 | 0 |
| MEDIUM | 4 | 1 (#4119) |
| LOW | 3 | 0 |

| ID | Sev | Dim | Title |
|---|---|---|---|
| SAFE-D1-2026-09-21-01 | HIGH | 1/5 | FSR dispatch-failure recovery blit declares `scene_color` `oldLayout = GENERAL`; the image is in `SHADER_READ_ONLY_OPTIMAL` |
| SAFE-D2-2026-09-21-01 | HIGH | 2/5 | Three staging-pool release sites still record `allocation.size()` as capacity — the #4512 `vkCmdCopyBuffer` overrun class, unfixed on the mesh/terrain paths |
| SAFE-D2-2026-09-21-02 | MEDIUM | 2 | `read_pod_vec_from`'s SAFETY argument rests on a false `io::Read` contract |
| SAFE-D4-2026-09-21-01 | MEDIUM | 4 | The CI gate for `#![deny(clippy::undocumented_unsafe_blocks)]` has not run since at least 2026-09-15 |
| SAFE-D5-2026-09-21-01 | MEDIUM | 5 | The CI `vulkan-validation` (lavapipe) gate has never reached Vulkan since at least 2026-08-31, and cannot by design since 2026-09-17 |
| SAFE-D7-2026-09-21-01 | MEDIUM | 7 (route: renderer Dim 11) | Auto-exposure divides the 64-thread log-luminance sum by one thread's sample count |
| SAFE-D1-2026-09-21-02 | LOW | 1 | Launcher preflight leaks the `VkInstance` when `enumerate_physical_devices` fails |
| SAFE-D3-2026-09-21-01 | LOW | 3 | `.expect()` on a poisoned allocator lock is reachable from `Drop` / teardown (double panic → abort) |
| SAFE-D4-2026-09-21-02 | LOW | 4 | Three SAFETY / `# Safety` texts restate a pre-refactor invariant |
| (Existing #4119) | MEDIUM | 8 | `queue_increment_own_i64` still skips the entity-visibility check — unchanged, not re-filed |

---

## Findings

### SAFE-D1-2026-09-21-01: FSR dispatch-failure recovery blit declares `scene_color` `oldLayout = GENERAL`; the image is in `SHADER_READ_ONLY_OPTIMAL`

- **Severity**: HIGH (Vulkan spec violation)
- **Dimension**: 1 — FFI call-site contract (`frame_upscaler.rs`) / 5 — Vulkan spec
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs:629-636` (recovery call); `:701-735` (`record_native_blit` before-barrier); `:800-807` (after-barrier `restore_layout`)
- **Status**: NEW

**Description.**
- #3572 (`ba4c0efcf`, 09-18) inserted a `source_layout` parameter ahead of `output_layout` in `record_native_blit` and updated all three call sites. At the FSR dispatch-failure recovery call, the pre-existing `GENERAL` argument had been the *output* layout. The insertion turned it into the source layout: the call now passes `GENERAL, GENERAL`.
- In FSR mode, `scene_color` is always `composite.scene_image(frame)` in `SHADER_READ_ONLY_OPTIMAL`:
  - `record_upscale_pass` picks the TAA output only when `self.post.taa` exists (`context/post_passes.rs:1196-1222`).
  - TAA is built only for `UpscalerMode::Taa` (`context/init.rs:1414`; `set_upscaler_mode` rebuilds/destroys it).
  - `record_fsr_barriers_before` debug-asserts `scene_color_layout == SHADER_READ_ONLY_OPTIMAL` (#4538, `:865`).
  - That function's `fsr_input_read_barrier` for `scene_color` has `old == new == SHADER_READ_ONLY_OPTIMAL` (`:1147-1158`).
- The recovery blit therefore records `oldLayout = GENERAL → TRANSFER_SRC_OPTIMAL` on an image that is actually in `SHADER_READ_ONLY_OPTIMAL`. `restore_layout` is keyed off `source_layout`, so the after-barrier then leaves the image in `GENERAL`.

**Evidence.**
```rust
// frame_upscaler.rs:628-636 — dispatch-failure recovery (after record_fsr_barriers_before)
self.record_fsr_depth_restore(device, cmd, inputs.depth);
self.record_native_blit(
    device, cmd, frame, inputs.scene_color,
    vk::ImageLayout::GENERAL,   // source_layout — scene_color is SHADER_READ_ONLY_OPTIMAL here
    vk::ImageLayout::GENERAL,   // output_layout — correct (output moved to GENERAL by the FSR barriers)
);
// :723-730
.old_layout(source_layout)                       // GENERAL
.new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
```
The pre-#3572 call was `record_native_blit(device, cmd, frame, inputs.scene_color, vk::ImageLayout::GENERAL)` with the scene barrier hard-coding `old_layout(SHADER_READ_ONLY_OPTIMAL)` (see `git show ba4c0efcf -- crates/renderer/src/vulkan/frame_upscaler.rs`).

**Impact.**
- The path runs on every real FSR dispatch error, and deterministically under the fault injector `BYRO_FSR_FORCE_DISPATCH_FAIL=1` (`crates/fsr3-sys/src/lib.rs:170-206, 437`).
- #3572's sync-validation matrix (Cornell TAA, TAA + raw view, FSR default, FNV Prospector) never exercised the injector.
- Consequences:
  - VUID-VkImageMemoryBarrier-oldLayout-01197 on the failure frame. A wrong `oldLayout` is undefined behaviour.
  - On drivers where `SHADER_READ_ONLY_OPTIMAL` and `GENERAL` differ in compression state, the failure frame's blit reads undefined contents.
- The leftover `GENERAL` layout is benign afterwards. The composite colour attachment is `initial_layout = UNDEFINED` (`composite.rs:452`), and bloom and the exposure meter run before the upscale.
- This finding still needs confirmation from a validation-layer run: `BYRO_VALIDATION=1 BYRO_FSR_FORCE_DISPATCH_FAIL=1`. I did not run it.

**Related.**
- #4538 (closed) is the inverse, hypothetical case: the FSR barrier hard-codes `SHADER_READ_ONLY_OPTIMAL` while `GENERAL` is representable. Its fix added only the debug_assert and did not touch this call.
- #2140, #2145, #2519 and #3632 cover other facets of this recovery path.
- SAFE-D4-2026-09-21-02: `record_native_blit`'s `# Safety` still says `SHADER_READ_ONLY_OPTIMAL`.

**Suggested Fix.**
- Pass `inputs.scene_color_layout` (== `SHADER_READ_ONLY_OPTIMAL` in FSR mode) as `source_layout` at the recovery call, and keep `GENERAL` only for `output_layout`.
- Extend `fsr_scene_color_barrier_asserts_the_layout_it_hard_codes` to pin the recovery call's source argument.

---

### SAFE-D2-2026-09-21-01: Three staging-pool release sites still record `allocation.size()` as capacity — the #4512 `vkCmdCopyBuffer` overrun class, unfixed on the mesh/terrain paths

- **Severity**: HIGH (Vulkan spec violation; #4512 was filed HIGH)
- **Dimension**: 2 — Memory corruption / UB (`vulkan/buffer.rs`) / 5 — Vulkan spec
- **Location**:
  - `crates/renderer/src/vulkan/buffer.rs:1599-1605` (`GpuBuffer::create_device_local_buffer`)
  - `crates/renderer/src/vulkan/buffer.rs:1758-1764` (`GpuBuffer::copy_bytes_range`)
  - `crates/renderer/src/vulkan/scene_buffer/upload.rs:1033-1040` (terrain-tile staging ring)
- **Status**: NEW (unfixed siblings of closed #4512)

**Description.**
- `80813026a` traced #4512's live `+8 B` `vkCmdCopyBufferToImage` overrun to pooled staging being released at `allocation.size()`, the driver-rounded footprint above the `VkBuffer` create size.
- `StagingPool::acquire`'s best fit (`e.capacity >= size`, `buffer.rs:219`) then serves a `VkBuffer` smaller than the new request.
- That commit fixed `record_dds_upload` and `create_device_local_buffers_batched`. It also rewrote `StagingGuard::release_to`'s contract: "capacity must be the buffer's *requested* size … never the allocation footprint."
- Three callers still pass the footprint:
  - `create_device_local_buffer` and `copy_bytes_range` both feed `MeshRegistry::geometry_staging_pool` (`mesh.rs:332`): the global SSBO build at `geometry_ssbo.rs:286/293`, and the #3298 chunked rebuild at `geometry_ssbo.rs:608/638`.
  - The per-cell batched upload (`mesh.rs:878`) draws from that same pool.
- `Vertex` is 104 B (≡ 8 mod 16). A 64 MiB rebuild chunk is 645,277 vertices = 67,108,808 B, also ≡ 8 mod 16. So odd-sized vertex uploads leave pool entries whose recorded capacity is 8 B above their `VkBuffer`.

**Evidence.**
```rust
// buffer.rs:1599-1605 (create_device_local_buffer) and :1758-1764 (copy_bytes_range)
let capacity = staging.allocation.as_ref().map(|a| a.size()).unwrap_or(size);
staging.release_to(pool, capacity);
// scene_buffer/upload.rs:1034-1039
let capacity = previous.allocation.as_ref().map(|allocation| allocation.size()).unwrap_or(byte_size);
previous.release_to(&mut self.terrain_tile_staging_pool, capacity);
```
The guard `pooled_staging_releases_the_requested_size_not_the_allocation_footprint` (`buffer.rs:2345-2366`) is blind to these sites:
- It forbids only the spelling `.map(|allocation| allocation.size())`. The two `buffer.rs` sites spell it `.map(|a| a.size())`, and `upload.rs` is a different file.
- The new `debug_assert!(capacity <= alloc.size())` in `release_to` is trivially satisfied by the footprint.

**Impact.**
- A later batched or chunked request that falls inside that 8 B slack is handed the smaller buffer. Its `vkCmdCopyBuffer` region then exceeds `srcBuffer`'s size (VUID-vkCmdCopyBuffer-srcOffset-00113 / size family) — the #4512 defect on the mesh path.
- The CPU write stays in bounds (the mapped slice is allocation-sized), so the GPU reads slack bytes of the same allocation. The result is a validation error and spec-level undefined behaviour rather than a crash.
- The collision window is narrow (request ∈ (buffer size, footprint]), so this is rarer than the texture case, where DDS sizes cluster.
- The terrain ring (160 B tiles) is affected only on drivers that round buffer requirements above 16 B.

**Related.** #4512 (closed; its SIBLING checkbox was left unchecked); #4187 (the same `StagingPool`).

**Suggested Fix.**
- Release at the requested `size` / `byte_size` at all three sites.
- Better: have `StagingGuard` remember its requested size and drop the caller-chosen `capacity` argument, so a caller cannot pick the footprint.
- Widen the pin to every `release_to` caller in the crate.

---

### SAFE-D2-2026-09-21-02: `read_pod_vec_from`'s SAFETY argument rests on a false `io::Read` contract

- **Severity**: MEDIUM (a stated safety invariant that is false; sound today only by instantiation)
- **Dimension**: 2 — Memory corruption / UB
- **Location**: `crates/nif/src/stream.rs:805-838` — the single POD `unsafe` site behind `NifStream::read_pod_vec` and `header::read_pod_vec_from_cursor`
- **Status**: NEW (pre-existing since #3062; prior runs passed it on the stated contract)

**Description.**
- The function passes `read_exact` a `&mut [u8]` over uninitialised `Vec` capacity.
- It justifies this with "`read_exact` only WRITES through the slice — `Read::read_exact` is a pure writer interface per its contract."
- std documents the opposite. `Read` is a safe trait whose implementations may read `buf`; callers "must not assume any guarantees about how the implementation uses buf", and calling `read` with an uninitialised buffer "is not safe, and can lead to undefined behavior". `read_exact` defers to that text.
- The function is generic: `reader: &mut impl io::Read`.

**Evidence.**
```rust
pub(crate) fn read_pod_vec_from<T: AnyBitPattern>(reader: &mut impl io::Read, count: usize, byte_count: usize) -> io::Result<Vec<T>> {
    let mut out: Vec<T> = Vec::with_capacity(count);
    // SAFETY: … `read_exact` only WRITES through the slice — `Read::read_exact` is a pure writer interface per its contract …
    let byte_slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), byte_count) };
    reader.read_exact(byte_slice)?;
```
Both callers pass `Cursor<&[u8]>` (`stream.rs:70/457`, `header.rs:436/457`), whose `read_exact` is a memcpy. That is the only reason the code is sound today.

**Impact.**
- No undefined behaviour today.
- The generic signature and the false comment invite a future caller to pass a decompressing or custom reader and get undefined behaviour without touching the `unsafe` block.
- This is the NIF POD bulk-read path every geometry parse goes through.

**Related.** #3062 (removed the zero pre-fill); #1439 (`AnyBitPattern`).

**Suggested Fix.**
- Narrow the parameter to `&mut Cursor<&[u8]>` (the only instantiation) and restate the SAFETY argument against that concrete impl.
- Alternatively, copy from `cursor.get_ref()[pos..]` into `out.spare_capacity_mut()` with `copy_nonoverlapping` and advance the cursor.

---

### SAFE-D4-2026-09-21-01: The CI gate for `#![deny(clippy::undocumented_unsafe_blocks)]` has not run since at least 2026-09-15

- **Severity**: MEDIUM (defence-in-depth gap around the MEDIUM-floor "unsafe without SAFETY comment" rule)
- **Dimension**: 4 — Unsafe-block discipline
- **Location**: `.github/workflows/ci.yml:169-172` (the `cargo test` → `cargo clippy` step order in job "Test + Check + Clippy"); `crates/renderer/src/lib.rs:21`
- **Status**: NEW

**Description.**
- The renderer's `deny` is present and has no `allow` escape. It is enforced only by `cargo clippy --workspace -- -D warnings`.
- That step runs after `cargo test` in the same job and has no `if: always()`.
- All 10 sampled main-branch runs from 2026-09-15 to 2026-09-21, including HEAD's `35658431384`, show `cargo clippy=skipped`:
  - 9 runs because `cargo test` failed;
  - 1 run (`d54382415`) because `cargo check` had already failed: the `exposure_meter.comp.spv` `include_bytes!` target was not committed until `f97775ca8`.
- At HEAD the test failures are 8 `crates/ui` tests (`navigator::tests::*`, `player::resource_loads_tests::*`, `player::render_failure_tests::*`). They build a real Ruffle player and panic in CI with "Ruffle requires hardware acceleration, but no compatible graphics device was found supporting Vulkan".

**Evidence.**
- `gh api …/runs/<id>/jobs` shows the step conclusions above.
- The gate is inert in practice, not just in theory. `dc306a6a0` (09-18) landed a comment-less `unsafe` closure in `Texture::overwrite_rgba_pixels` (`texture.rs:191`). CI never flagged it, and the 09-21 tech-debt audit fixed it inline (#4567).

**Impact.**
- Comment-less renderer `unsafe` can merge unseen. It already did once this week.
- Every other `-D warnings` lint is equally unenforced.
- I ran the lint locally at HEAD and the renderer is clean, so there is no outstanding comment-less block.

**Related.** #4567 (closed) treats the gate as "red on the current toolchain" and does not record that CI skips it. #4090 and #4130 were earlier clippy-red episodes.

**Suggested Fix.**
- Run clippy in its own job or step with `if: always()`, or before `cargo test`.
- Gate the adapter-dependent `crates/ui` tests behind a wgpu adapter probe or `#[ignore = "needs a Vulkan adapter"]`, so `cargo test` can go green.

---

### SAFE-D5-2026-09-21-01: The CI `vulkan-validation` (lavapipe) gate has never reached Vulkan since at least 2026-08-31, and cannot by design since 2026-09-17

- **Severity**: MEDIUM (defence-in-depth gap for the HIGH-floor Vulkan spec class)
- **Dimension**: 5 — Vulkan spec compliance
- **Location**: `.github/workflows/ci.yml:280-359`; `crates/renderer/src/vulkan/device.rs:311-313, 351-361`
- **Status**: NEW

**Description.**
- This job is the skill's Dim-5 first step. It is the only CI lane that boots the engine under `VK_LAYER_KHRONOS_validation`, and also the only one under `BYRO_LOCK_ORDER_CHECK=1`.
- Every sampled run dies in "Run 5-frame bench" before any window or `VkInstance` exists:
  - runs sampled: 2026-08-31 (`33451855702`), 09-11 (`34546452899`), 09-21 (`35622136101`), and HEAD (`35658431384`);
  - error: `panicked at …/xkbcommon-dl-0.4.2/src/x11.rs:59:28: Library libxkbcommon-x11.so could not be loaded`;
  - cause: the apt step installs `mesa-vulkan-drivers`, `vulkan-validationlayers`, `libvulkan-dev`, `libasound2-dev` and `xvfb`, but not `libxkbcommon-x11-0`.
- The job is therefore red for a non-Vulkan reason on every run, and a genuine `[Vulkan]` error would look identical.
- Independently, `33a99ed94` (09-17) made `pick_physical_device` reject `VK_PHYSICAL_DEVICE_TYPE_CPU` (`is_hardware_render_device`). Lavapipe is a CPU device. Fixing the library would only turn the panic into the "no suitable GPU" bail the job explicitly tolerates, and the following `check-bench-determinism.sh` step would then fail on the non-zero exit.
- The self-hosted GPU workflows (`rt-correctness.yml`, `playable-smoke.yml`) are `workflow_dispatch`-only and do not enable validation.

**Evidence.** The CI logs quoted above. Also `device.rs:311-313`:
```rust
fn is_hardware_render_device(device_type: vk::PhysicalDeviceType) -> bool { device_type != vk::PhysicalDeviceType::CPU }
```

**Impact.**
- No CI lane can see a Vulkan validation error. That includes both HIGH findings above, and #4510's VUID-layout-10069 pair, which a manual run found on 09-20.
- The lock-order detector's only live-world lane is equally inert. That half is /audit-concurrency's to track.

**Related.** #2138 (closed; the job used to swallow exit codes).

**Suggested Fix.**
- Add `libxkbcommon-x11-0` to the apt step.
- Then either add a CI-only switch that admits CPU devices for the 5-frame validation boot (with a Mesa new enough to expose ray query), or move the gate to the self-hosted RT runner with `BYRO_VALIDATION=1`.
- Until one of those lands, the skill's Dim-5 first step should say the gate is inert.

---

### SAFE-D7-2026-09-21-01: Auto-exposure divides the 64-thread log-luminance sum by one thread's sample count

- **Severity**: MEDIUM (correctness, opt-in mode. Outside the safety scope proper: route to /audit-renderer Dim 11. Reported because I found it during Dim 7's GPU-fed-data read of the newest shader.)
- **Dimension**: 7 — GPU-fed data & shader loop bounds
- **Location**: `crates/renderer/shaders/exposure_meter.comp:57-94` (`c5663fe39`, 2026-09-21 — newer than every renderer audit)
- **Status**: NEW

**Description.**
- Each of the 64 invocations accumulates its own `log_sum` and its own `int count` over its 64 of the 4,096 grid samples.
- The shared reduction sums `log_sum` across all invocations. Invocation 0 then divides that total by its *own* `count` (≈64) instead of the total (≈4,096).

**Evidence.**
```glsl
int count = 0;                                                    // per invocation
for (int idx = int(gl_LocalInvocationID.x); idx < side * side; idx += 64) { … log_sum += …; count += 1; }
shared_log_sum[gl_LocalInvocationID.x] = log_sum;  /* tree-reduce across 64 */
float samples = max(float(count), 1.0);                          // invocation 0's count only
float avg_luminance = exp2(shared_log_sum[0] / samples);        // = exp2(64 × mean log2 L)
```

**Impact.**
- Metering is bang-bang:
  - mean log-luminance below 0 (L < 1, i.e. essentially any linear-HDR interior) → exposure pinned at `MAX_AUTO_EXPOSURE`;
  - mean above 0 → pinned at `MIN_AUTO_EXPOSURE`.
- Output stays finite only because of the final clamp against constant limits.
- Tests cannot see it: the host `auto_exposure()` and its tests (`exposure.rs:68, 266-293`) model the correct mean and never touch the shader.
- Scope: `--auto-exposure` / console `exposure auto` only. Fixed mode is the default (`cli_args.rs:216`).
- **Secondary issue, already filed elsewhere.** `imageLoad(dstExposure)` reads the per-frame-in-flight slot written two frames earlier, so the two slots adapt as independent chains and the effective time constant doubles. The concurrent renderer leg files this as REN-D11-2026-09-21-02 (`AUDIT_RENDERER_2026-09-21.md`); it is not a separate item here. That report does not cover the divisor bug above.

**Related.** None (the code postdates AUDIT_RENDERER_2026-09-20).

**Suggested Fix.**
- Reduce a `shared_count[]` alongside the sums, or divide by the known in-bounds total.
- (The adaptation-history fix belongs to REN-D11-2026-09-21-02.)

---

### SAFE-D1-2026-09-21-02: Launcher preflight leaks the `VkInstance` when `enumerate_physical_devices` fails

- **Severity**: LOW
- **Dimension**: 1 — FFI lifetime (`tools/byro-launcher/src/preflight.rs`)
- **Location**: `tools/byro-launcher/src/preflight.rs:186-202`
- **Status**: NEW (unchanged since `e05b4a9f8`, 2026-08-30; never reported)

**Description.** `instance.enumerate_physical_devices().map_err(|_| Blocker::NoAdapter)?` returns before `instance.destroy_instance(None)`. This fails the skill's own check that the instance is destroyed on every path.

**Evidence.** In `probe()`, the only `destroy_instance` call follows the `?`.

**Impact.** A one-shot `VkInstance` leak on the launcher's error path. The process continues or exits. No per-frame cost.

**Related.** None.

**Suggested Fix.** Destroy the instance before propagating the error (match on the result, or use a small drop guard).

---

### SAFE-D3-2026-09-21-01: `.expect()` on a poisoned allocator lock is reachable from `Drop` / teardown (double panic → abort)

- **Severity**: LOW
- **Dimension**: 3 — Leaks / drop ordering
- **Location**:
  - `crates/renderer/src/vulkan/buffer.rs:637-658` (`StagingGuard::drop` → `cleanup`)
  - `buffer.rs:1471-1475` (`GpuBuffer::destroy`)
  - `buffer.rs:343-347` (`StagingPool::trim_to`, reached from `destroy()` on every teardown since #4187)
  - `crates/renderer/src/vulkan/texture.rs:631-635` (`Texture::destroy`)
  - `crates/renderer/src/vulkan/context/teardown.rs:432` (`VulkanContext::drop`: `mutex.into_inner().expect("allocator lock poisoned")`)
- **Status**: NEW (pre-existing; the skill's Dim-3 rule names this pattern, but prior runs did not file it)

**Description.** `StagingGuard` is the RAII unwind guard (#2164). Its `Drop` does `allocator.lock().expect("allocator lock poisoned").free(alloc).expect("Failed to free staging allocation")`, and the teardown paths repeat the pattern. A panic while any thread holds the allocator mutex poisons it. The unwind, or the later context drop, then panics again inside `Drop`, which aborts the process.

**Evidence.** The listed sites. #4089 moved only `GpuImage` onto `into_inner()` recovery.

**Impact.**
- The abort skips the rest of teardown: the pipeline-cache save and the device/instance destroy.
- Triggering it requires a panic inside a lock-held region, hence LOW.

**Related.** #4089 (`GpuImage` policy), #2398 (silent recovery needs a rationale), #95.

**Suggested Fix.** Route free-side and `Drop`-side locks through the `GpuImage` recovery (`unwrap_or_else(PoisonError::into_inner)`), and log instead of `expect` on `free()` errors inside `Drop`.

---

### SAFE-D4-2026-09-21-02: Three SAFETY / `# Safety` texts restate a pre-refactor invariant

- **Severity**: LOW
- **Dimension**: 4 — Unsafe-block discipline (truth of stated invariants)
- **Location**:
  - `crates/renderer/src/vulkan/frame_upscaler.rs:691-700` and `:746-748`
  - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:500`
  - `crates/renderer/src/vulkan/scene_buffer/upload.rs:1049-1052`
- **Status**: NEW

**Description.**
- `record_native_blit`'s `# Safety` says "`scene_color` must be in `SHADER_READ_ONLY_OPTIMAL`" and its inner SAFETY says "scene composition left `scene_color` shader-readable". Since #3572 the function takes `source_layout`, and the TAA path legitimately passes `GENERAL`. This stale contract is part of why SAFE-D1-2026-09-21-01's wrong argument looks plausible.
- The skin dispatch SAFETY says "Each `dispatch` binds the compute pipeline + slot set". Since #4205 the pipeline is bound once per batch through `SkinComputePipeline::bind`, and `slot_dispatch_does_not_rebind_the_pipeline_per_entity` pins that `dispatch` no longer binds it.
- The terrain staging SAFETY says "GpuTerrainTile is #[repr(C)] with u32-only fields". Since #4057 the struct carries `f32` lanes.

**Evidence.** The quoted texts are at the lines above.

**Impact.** Documentation only at the second and third sites: the property actually relied on still holds. At the first site, the stale text masks a live bug.

**Related.** SAFE-D1-2026-09-21-01; #3572, #4205, #4057.

**Suggested Fix.** Restate each text against the current code.

---

### Existing #4119 (MEDIUM, open): `queue_increment_own_i64` still skips the entity-visibility check

`crates/mod-runtime/src/runtime/host/state.rs:6-51` is unchanged since the baseline's SAFE-D11-01. It gates capability, budget and schema/field/type, then goes straight from the guest `EntityRef` to `ExtensionCommand::IncrementI64` with no `entity_projections` check. Not re-filed.

---

## Per-dimension detail

### Dimension 1 — FFI lifetime safety — 1 HIGH, 1 LOW

- **fsr3-sys:** no commits. `Context::create` and `dispatch` keep their `# Safety` docs, `Drop` cites create's idle contract, and every block has a SAFETY comment.
- **`frame_upscaler.rs` blit call sites:**
  - the bridge path passes `inputs.scene_color_layout`, which is correct for both the TAA `GENERAL` slot and the `SHADER_READ_ONLY_OPTIMAL` composite;
  - the params-absent path's `SHADER_READ_ONLY_OPTIMAL`/`SHADER_READ_ONLY_OPTIMAL` is correct;
  - only the failure path is wrong (D1-01);
  - the #4221 barrier-helper swaps are field-for-field identical.
- **`crates/ui`:** still no `unsafe`.
  - `tick_hud_overlay` copies to an owned `Vec` before `upload_frame`.
  - `write_rgba_inplace` → `overwrite_rgba_pixels` validates extent and length against `creation_extent` (#4515 present), then fence-waits inside `with_one_time_commands`, so no borrow outlives the call.
  - The triple-buffer rotation rewrites a buffer only ≥3 frames after its last sample (> `MAX_FRAMES_IN_FLIGHT = 2`).
- **cxx-bridge:** still the pointer-free `native_hello() -> String`.

### Dimension 2 — Memory corruption / UB — 1 HIGH, 1 MEDIUM

- New `unsafe impl NoUninit` for `MeterParams`, `SkyCubeParams` and `GpuLight` are all `repr(C)` and padding-free.
- #4445 and #4521 route the material and scene hash views through `byte_view`. The `hash_indirect_slice` exemption is sound.
- **pex:** `from_u8` has a range check and the new compile-time `TryLockGuards as u8 == MAX_OPCODE - 1` assert (#4475).
- **NIF:** `checked_mul`, the allocation bounds, and `set_len` after `read_exact` all hold; #4165 restored `#[must_use]`; #4549's finite gate covers both `NiTransform` reader orders.
- **LZ4:** the pin holds — workspace `lz4_flex` `default-features = false` plus `safe-decode`/`checked-decode`, 0.11.6 in the lock, and `lz4_flex_is_pinned_to_the_safe_decoder` present.
- **Cargo:** the ratatui 0.30 bump removed the advisory-affected `lru` ^0.12; the lock now carries only `lru` 0.16.3 and 0.18.4, both at or past the fix.
- **menuxml:** tile nesting capped at 48 and op bodies at 64, so layout recursion is bounded.

### Dimension 3 — Leaks, drop ordering — 1 LOW

- **Rapier release:** 9 tests pass, and `release_sweeps_both_ragdoll_and_rapier_handles` still asserts emptiness. #4427 flipbook frame handles are dropped on unload.
- **Deferred destroy:**
  - still four `DeferredDestroyQueue` instantiations;
  - the texture, mesh, accel and instance ticks all run after the fence wait (`sync_and_acquire_frame.rs:209-231`);
  - `texture_pending_destroy_count` is now published (`e3131f5ef`).
- **`AllocatorResource`:** removed before `renderer.take()` on both `shutdown()` and `impl Drop for App`.
- **New GPU owners:** sky cube (with `SkyFilter` and `SkyIrradiance`), exposure meter and cloud noise are all destroyed before the allocator unwrap, and each constructor has a single cleanup path.
- **CPU growth:** `PersistentReferenceStates` is keyed by `FormIdPair` and consumed on respawn; geometry pools are hard-capped with an admission latch (`84bbc44ed`).
- **Note, not a finding:** `overwrite_rgba_pixels` acquires from the registry pool but lets `StagingGuard::drop` *destroy* the buffer; its comment says it "goes back to the pool". That costs performance only.

### Dimension 4 — Unsafe-block discipline — 1 MEDIUM, 1 LOW

- The `deny` is present with no escapes.
- The local clippy run on the renderer is clean. The same lint on nif/core/pex/fsr3-sys shows only the known `bs_geometry.rs` adjacency false positives.
- A script sweep confirms every non-renderer block has an adjacent SAFETY comment.
- **Post-09-20 exposure-meter blocks:** all commented, `# Safety` docs present, and the invariants hold (per-frame-in-flight set and slot; `SHADER_READ_ONLY_OPTIMAL` ↔ `GENERAL` round trip; `STORAGE` usage on the slots).
- **`SkinComputePipeline::bind` (#4205):** its single call site binds nothing else between the lazy bind and the `dispatch` calls, so the precondition holds.

### Dimension 5 — Vulkan spec compliance — 1 MEDIUM (plus the two HIGHs filed under Dims 1 and 2)

- New objects pair create and destroy correctly, children before parents.
- The exposure-meter slots carry `STORAGE` usage.
- **Acceleration-structure guards intact:** the TLAS/skin-refit count asserts, and the TLAS-resize `device_wait_idle`.
- **Depth capture:** refuses anything other than `D32_SFLOAT`.
- **Ray query:** mandatory at device selection.
- **`PipelineStageFlags::NONE`:** legal here, because synchronization2 is required and enabled (`device.rs:866`).
- **Pins all pass:**
  - `volume_far_shader_constant_agrees_with_the_volumetrics_config_default`;
  - `skip_clear_mask_pin_tests::*`;
  - `VOLUMETRIC_OUTPUT_CONSUMED = true` gates `vol.dispatch()`;
  - the SPIR-V reflection tests.

### Dimension 6 — GPU struct layout — PASS, no findings

- **Size pins pass:**
  - `gpu_material_size_is_432_bytes` (`detail_neutral` at offset 428 — the baseline's 432/428 drift is closed);
  - `gpu_instance_is_160_bytes_std430_compatible`, `gpu_camera_is_368_bytes`, `gpu_light_is_64_bytes`, `gpu_terrain_tile_is_160_bytes`.
- **Offset and field-name pins pass:**
  - Rust ↔ GLSL field order and hash coverage;
  - the "no vec3 in GLSL" pin;
  - the `NoUninit` upload bound;
  - `gpu_material_size_claims::*`;
  - `bindings_glsl_states_the_real_struct_size`.
- **Material limits:** the intern cap (`MAX_MATERIALS = 16384`), the overflow-to-0 tests, and the `upload_materials` `.min()` clamp are in lockstep.
- **Skill-text note:** the rule "new scalars must be zero in `GpuMaterial::default()`" no longer matches the code. `default()` carries non-zero neutral values, and slot 0 is seeded from it (`seed_neutral_default`).

### Dimension 7 — GPU-fed data & loop bounds — 1 MEDIUM (routed)

- **New shader loops are bounded:**
  - exposure meter: fixed trip counts;
  - BFECC: an 8-corner loop plus a threshold-gated TLAS query;
  - `cluster.count` loops are `min()`-clamped;
  - combustion steps are compile-time constants.
- **Glass passthrough:** the bound's pin passes.
- **Material producers:** both production producers run `resolve_pbr()`.
- **Console exposure inputs:** range-checked, and NaN is rejected.
- **Bone palette:** the 4 overflow tests pass, and the #3569/#3991 latch is intact (`app_frame.rs:598`).

### Dimension 8 — Sandboxed mod runtime — 0 NEW; #4119 still open

- WASI is absent (`cargo tree` is empty; `wasmtime` 47.0.3 with `default-features = false`).
- 22 `require_` sites and 28 `*_CAPABILITY` constants, with the host modules untouched.
- No mod-runtime statics.
- The new `item-category` enum cases lift exhaustively.

---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-09-21.md
```
