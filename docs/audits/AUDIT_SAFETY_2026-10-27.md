**HEAD**: fa6be4a6b · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-10-21.md` (@ `4a3e89d56`, 395 commits ago) · **Audited**: Dims 1, 2, 3, 4, 5, 6, 7, 8 (each had commits on its `Paths:` since the baseline) · **Unchanged since baseline (skimmed)**: none. Dim 1's FFI crates themselves (`crates/fsr3-sys`, `crates/cxx-bridge`, `tools/byro-launcher/src/preflight.rs`) have no diff. The Dim 1 commits are a comment-only change in `frame_upscaler.rs` and safe-Rust `crates/ui/src/navigator.rs` edits.

# Safety Audit — ByroRedux — 2026-10-27

**Command**: `/audit-safety` (default delta scope), run inside the 2026-10-27 comprehensive suite
**Severity scale**: `.claude/commands/_audit-severity.md`

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 0 |

The headline finding is SAFE-2026-10-27-01. #6250 added a per-frame-in-flight `moved_surface_buffers` pair to `SceneBuffers`, but `SceneBuffers::destroy` was never taught to free it. Every shutdown since then fails the allocator `Arc::try_unwrap`:

- **Release builds** take the #665 leak-everything branch.
- **Debug builds** panic in `VulkanContext::drop`.

This panic is the only thing that has turned the CI `Vulkan validation layers (lavapipe)` lane red since 2026-10-25. In that lane's log the validation layer itself prints no `[Vulkan]` lines, and the RT integrity check passes.

## Method

- **Scoping.** `git log 4a3e89d56..HEAD -- <Paths>` per dimension:

  | Dim | Commits | Notes |
  |---|---|---|
  | 1 | 3 | |
  | 2 | 5 | renderer only; `Cargo.toml` unchanged, `Cargo.lock` = wasmtime 49.0.2→49.0.4 |
  | 3 | 19 | |
  | 4/5 | 52 | `crates/renderer/src/vulkan` |
  | 6 | 19 | |
  | 7 | 30 | |
  | 8 | 4 | |

- **Process.** No sub-agents. No source was edited and no engine or GPU process was launched. Scratch notes are in `/tmp/audit/safety/dim_{1..8}.md`. Cargo used rustc 1.96.0 and the repo `target/`; no private target dir.
- **Commands run at HEAD:**
  - `cargo test -p byroredux-renderer --lib`: **1501 passed, 0 failed, 1 ignored.** This covers the size and offset pins, `shader_contract_tests`, the `from_raw_parts` count pin (#6349), `no_initialize_layouts_caller_destroys_without_the_in_flight_check`, `a_failed_layout_walk_fails_closed_instead_of_warning_and_continuing`, the allocator-lock poison guards and `no_file_states_a_stale_gpu_material_size`.
  - `cargo test -p byroredux --bin byroredux -- rapier_release bone_palette_overflow allocator_teardown_order release_ no_source_scan unload_walk greyscale`: **62 passed, 0 failed.**
  - `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks`: **exit 0.**
  - `cargo clippy -p byroredux-fsr3-sys -p byroredux-nif -p byroredux-core -p byroredux-pex -p byroredux -p byro-launcher -p byroredux-ui -p byroredux-terrain-height --no-deps -- -A clippy::all -W clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc`: the only output is the three known `bs_geometry.rs:364-366` adjacency false positives.
  - `cargo clippy -p byroredux-plugin --tests --no-deps -- -A clippy::all -W clippy::undocumented_unsafe_blocks`: 2 warnings (SAFE-02).
  - `cargo tree -p byroredux-mod-runtime | grep -ic wasi`: **0**. `grep -c require_ crates/mod-runtime/src/runtime/capabilities.rs`: **22** (unchanged).
  - `gh run view 40596734340 --log-failed` (CI @ eae8fd740) plus the CI run history, used to place the lavapipe lane's first red run.
- **Dedup.** Checked against `/tmp/audit/open_issues_snapshot.txt` (84 open) and `SUITE_RULES.md` § 6 (empty at finalization). Closed-issue searches covered `outstanding Arc references` (#927, #4187, #665 and #732: same symptom, different fields, all still fixed) and `moved_surface_buffers` (no hits). #6250 is CLOSED. Possible overlap: the suite's regression audit has pulled the same failing CI logs (`/tmp/audit/regression/ci_c7463c04d.log`); if it reports the red lavapipe lane, that is the same root cause as SAFE-01.

## Census (re-measured at fa6be4a6b)

| Scope | Tokens | `unsafe {` blocks | `unsafe fn` | `unsafe impl` | Delta vs 10-21 report |
|---|---|---|---|---|---|
| `crates/renderer/src` | **948** (943) | **717** (716) | **102** (100) | **44** (38) | See the breakdown below the table. |
| `crates/fsr3-sys/src` | 12 | 10 | 2 | 0 | unchanged |
| `crates/nif` / `core` / `pex` | 13 / 10 / 4 | 1 / 6 / 1 | — | 4 / 0 / 0 | unchanged (core's extra tokens are prose in `source_scan.rs`) |
| `byroredux` / `tools/byro-launcher` | 5 / 4 | 2 / 1 | 0 / 1 | — | unchanged |
| `crates/plugin` | 8 | **6** (2) | — | — | +4 test-only env edits in `esm/test_paths.rs` (#6184, 3e4bfab51). Two of them lack a comment: SAFE-02. |
| every other crate / tool | 0 `unsafe {fn,impl,trait,extern}` | | | | No newcomer. `crates/sdk` remains the only `forbid(unsafe_code)` crate. |

Renderer delta breakdown:

- **Added:**
  - `submit_initial_layouts` (`unsafe fn`, #6146).
  - `sky_cube::record_cloud_shadow` (`unsafe fn`).
  - Six `NoUninit` impls from #6270: `Push`, `AtrousPush`, `WaterPush`, `PresentationPushConstants` and the two skin push structs.
- **Removed:**
  - The hand-rolled `from_raw_parts` push and bulk byte views (#6270, #6349).
  - Seven per-pass layout-walk submits, now folded into the shared helper.

## Findings

### SAFE-2026-10-27-01: `SceneBuffers::destroy` never frees the #6250 `moved_surface_buffers`, so every shutdown leaks the device (release) or panics (debug)
- **Severity**: HIGH
- **Dimension**: 3. Memory & Resource Leaks, Drop Ordering (also Dim 5: missing destroy)
- **Location**:
  - `crates/renderer/src/vulkan/scene_buffer/buffers.rs:237`: the field.
  - `crates/renderer/src/vulkan/scene_buffer/buffers.rs:663-668`: creation, one `GpuBuffer::create_host_visible` per frame in flight.
  - `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:361-426`: `destroy`, where the field is absent.
  - `crates/renderer/src/vulkan/context/teardown.rs:454-497`: the failing unwrap.
- **Status**: NEW. Regression introduced by bed162262 (Fix #6250, closed).
- **Description**: `SceneBuffers::destroy` destroys and `.clear()`s every other `Vec<GpuBuffer>`: light, camera, bone palette, instance, previous-model, material, DALC, indirect and selected-ray-probe. #6250 added `moved_surface_buffers`, seeded it at construction (`buffers.rs:1024`) and rewrote it each frame (`upload.rs:841-863`), but never added it to `destroy`.
  - Each `GpuBuffer` holds a `SharedAllocator` (`Arc<Mutex<Allocator>>`) clone.
  - So at `VulkanContext::drop` the allocator's strong count is 3: the context's own Arc plus the two frames-in-flight buffers.
  - `Arc::try_unwrap` therefore fails, and teardown takes the #665 branch.
- **Evidence**: CI run 40596734340 (eae8fd740), job `Vulkan validation layers (lavapipe)`:
  ```
  ERROR byroredux_renderer::vulkan::context::teardown] GPU allocator has 3 outstanding references — leaking allocator + device + surface + instance ...
  thread 'main' panicked at crates/renderer/src/vulkan/context/teardown.rs:496:25:
  GPU allocator leaked: outstanding Arc references
  bench exit status: 101
  ```
  The same log has `Selected GPU: "llvmpipe …"`, `rt-integrity: … rt_flag=1 tlas_build=1 … verdict=PASS`, and **no** `[Vulkan]` lines, so the panic is the lane's only failure. The run history places it:
  - The lane was last green at 79c8322f4.
  - It was first red at c7463c04d, the first CI run that includes bed162262.
  - It has been red on every run since.

  A field-by-field read confirms this is the only GPU-resource field added since the baseline that has no destroy. The new `ShadowFilterPipeline::point_sampler` is destroyed. The `init_stages.rs` view vectors are borrowed handles.
- **Impact**: Every engine exit is affected.
  - **Release:** teardown deliberately skips `vkDestroyDevice`, the surface, the instance and the debug messenger, and the OS reclaims them at process exit. The "Process must terminate to reclaim" path becomes the normal shutdown.
  - **Debug:** `debug_assert!(false)` panics inside `Drop`. Every debug bench exit returns 101.
  - **CI cost:** the lavapipe lane is blind. A real validation regression landing now would sit behind a lane that is already red.
  - **Not affected:** this is not a per-frame leak, and no device memory is used after free, because the Drop safety net runs against the deliberately leaked, still-live device.
- **Related**: #665, #927, #4187 and #732 (closed, same symptom on other fields); #6250 (closed, origin).
- **Suggested Fix**:
  1. Add `for buf in &mut self.moved_surface_buffers { buf.destroy(device, allocator); } self.moved_surface_buffers.clear();` beside the `selected_ray_probe_buffers` arm in `SceneBuffers::destroy`.
  2. Add a source-scan guard that every `Vec<GpuBuffer>` / `GpuBuffer` field declared on `SceneBuffers` appears in `destroy`. Today nothing pins this, and the lavapipe lane is the only detector.

### SAFE-2026-10-27-02: Two comment-less test `unsafe { set_var }` blocks in `test_paths.rs` that contradict the file's own "not unit-tested" note
- **Severity**: MEDIUM. This is the `_audit-severity` floor for an uncommented `unsafe` block; the code is test-only.
- **Dimension**: 4. Unsafe-Block Discipline (outside the renderer's lint)
- **Location**: `crates/plugin/src/esm/test_paths.rs:245-251` (blocks) and `:274-277` (contradicting doc)
- **Status**: NEW (introduced by 3e4bfab51, Fix #6184)
- **Description**:
  - **The uncommented blocks.** `require_or_skip_fails_only_when_the_strict_lane_asks_for_game_data` toggles `BYROREDUX_REQUIRE_GAME_DATA` with four `unsafe` env edits. Two carry or cite a SAFETY note. The `set_var(…, "0")` at :245 and `set_var(…, "1")` at :249 carry none; `clippy::undocumented_unsafe_blocks --tests` flags exactly these two.
  - **The cited justification.** The note those blocks lean on, "see `data_dir_accessors_…`", argues that the touched vars are read by no other test.
  - **The contradiction.** The `#3850` test's doc 25 lines below still says this exact variable is "deliberately not unit-tested: it is read process-globally by every resolver in the workspace, so setting it here would turn every concurrently-running real-data test in this binary into a panic."
- **Evidence**:
  - `warning: unsafe block missing a safety comment --> crates/plugin/src/esm/test_paths.rs:245:9` and the same at `:249:9`.
  - Every in-crate `require_or_skip` caller (`esm/reader.rs:1703`, `records/actor/tests.rs:2582`/`2617`, `cell/tests/wrld.rs:1095`) is `#[ignore]`d.
- **Impact**: No production impact. In the default test process the race is latent. Under `cargo test -p byroredux-plugin -- --include-ignored` on a machine with game data, a concurrently running real-data test that hits a missing file while the flag is `"1"` would panic spuriously. The safety argument for that case is unstated, and the file contradicts itself.
- **Related**: #6184 (origin, closed). Crates outside the renderer have no `undocumented_unsafe_blocks` gate, which is why this landed.
- **Suggested Fix**:
  1. Give the two blocks a SAFETY line stating why the race is acceptable: every reader is `#[ignore]`d.
  2. Update the #3850 doc so it no longer claims the variable is untested.
  3. Optionally, serialize env-mutating tests behind a shared `Mutex` so `--include-ignored` is also sound.

## Dimension results (PASS / regression guards intact)

1. **Dim 1 — FFI.**
   - **FFI crates, no diff.** `crates/fsr3-sys` and `crates/cxx-bridge` are unchanged; cxx still exposes only `native_hello() -> String`. `tools/byro-launcher/src/preflight.rs` is also unchanged.
   - **`frame_upscaler.rs`:** comment-only, from #5622.
   - **`CStr::from_ptr`:** no new production read on a fixed `[c_char; N]`. The hits are the `fsr3-sys/examples` smoke harness, `debug.rs:63` `p_message` and the shim's static error string.
   - **`crates/ui`.** It has no `unsafe`, and every production `decompress_swf(` is preceded by `ensure_declared_len_within_ceiling`:
     - `avm1_host.rs:121`
     - `avm2_host.rs:78/372`
     - `prepare.rs:367`
     - `profile.rs:25`
     - `navigator.rs:547/612`, plus the #6225 body check at `:282`
2. **Dim 2 — UB.**
   - **Remaining raw views.** The only raw byte views left in renderer production are `byte_view` itself (`buffer.rs:68`), the documented ash-type exemption (`scene_buffer/descriptors.rs:520`) and the pre-existing `read_unaligned` readbacks and mapped `copy_nonoverlapping` uploads.
   - **Push constants:** every `cmd_push_constants` source is `byte_view`, `to_ne_bytes` or a typed helper.
   - **Untouched UB sites:** nif `read_pod_vec_from`, pex `OpCode::from_u8` and core `query.rs` have no diff.
   - **LZ4:** the `lz4_flex` workspace entry keeps `default-features = false` plus `"safe-decode"`; `crates/bsa` is the sole dependent, via `workspace = true`.
3. **Dim 3 — leaks.**
   - **Finding:** SAFE-01.
   - **Teardown order:** `AllocatorResource` is still removed before the context drops (`app_events.rs:155`, `main.rs:893`, pinned at `app_events.rs:2116-2154`).
   - **Deferred destroy:** there are still four `DeferredDestroyQueue` instantiations.
   - **#6209:** the particle-emitter LUT refs are still collected, now in their own guard pass.
   - **Geometry admission tiers (22d6e1a80, ab9d46d18, bb97be77c):** the prop and actor latches reset with the global latch (`mesh/geometry_ssbo.rs:268-270`), and `abandon_scene_geometry_group` rolls the pools back.
   - **Physics:** #6324 removes relaunched bodies from `spawn_sleep_pending`.
4. **Dim 4 — unsafe discipline.**
   - **Finding:** SAFE-02.
   - **Lint gate:** the #1904 `deny` is at `crates/renderer/src/lib.rs:21` with no `allow`. CI still has both `--keep-going` workspace clippy steps (`ci.yml:212`, `:222`) and the dedicated renderer re-lint (`:233`).
   - **#6143 split (3e6f6b66a), checked for verbatim moves.** Comparing the removed and added line sets:
     - `init.rs` → `init_stages.rs` differs by one argument line.
     - The `build_and_upload_instances` split differs only by a `continue` that became a function return in `push_or_extend_batch`.
   - **Truth checks on new blocks:**
     - **`submit_initial_layouts` callers** pass owned, never-submitted images. srcStage `NONE` is legal because sync2 is required.
     - **Shadow-filter binding 5 (#6249):** the mesh-ID view is rebound on the only G-buffer recreate path, unconditionally (`resize.rs:717`).
     - **Timer queries:** the cloud-shadow slots are 48/49, below `QUERIES_PER_FRAME` = 60.
     - **#6343 resize arms** return `Err`, and both callers exit the event loop.
5. **Dim 5 — Vulkan spec.**
   - **lavapipe lane:** no `[Vulkan]` validation errors at eae8fd740; the red status is SAFE-01's teardown panic.
   - **Acceleration and depth capture:** `acceleration/` and `depth_capture.rs` are untouched.
   - **Underwater god-ray march (`composite.frag`):** a fixed 24 steps × 6 depth taps, with `clip.w` guarded. Its hard-coded IOR is Existing #6418.
6. **Dim 6 — GPU layout.** `GpuTerrainTile` grew from 208 to 224 B with #6197's three parallax fields plus `lod_soften_end`. It is pinned by `gpu_terrain_tile_is_224_bytes` and the field-offset contract, and mirrored in `bindings.glsl`. The `GpuMaterial` (432 B), `GpuInstance` (160 B), `GpuCamera` (368 B) and `GpuLight` (64 B) pins pass.
7. **Dim 7 — GPU-fed data.**
   - **Glass passthrough loop:** the `MAX_REFRACT_PASSTHRUS` bound is intact (`triangle.frag:2430`).
   - **Terrain parallax (#6346, c3b84ec32):** the step count is clamped after the float-to-int conversion, and the secant weight is finite.
   - **Moved-surface set:** capped on the CPU (`upload.rs:850`) and re-clamped in the shader (`triangle.frag:122`).
   - **Skin rollback:** the `skin_state_submitted` / `bind_inverse_upload_failed` rollback is intact (`app_frame.rs:790`).
8. **Dim 8 — mod runtime.**
   - **WASI:** none linked. 22 `require_` guards.
   - **#4119 (09cbce41d)** adds an entity-visibility `bail!` to `queue_increment_own_i64`: an `Err` to the guest, not a silent no-op.
   - **Isolation and limits:** no new shared state.

## Next step

`/audit-publish docs/audits/AUDIT_SAFETY_2026-10-27.md`
