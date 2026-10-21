**HEAD**: 4a3e89d56 · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-10-19.md` (@ `fc9622f6f`, 163 commits ago) · **Audited**: Dims 2, 3, 4, 5, 6, 7, 8 (each had commits on its `Paths:` since the baseline) · **Unchanged since baseline (skimmed)**: Dim 1 (`crates/fsr3-sys`, `crates/cxx-bridge`, `crates/ui`, `frame_upscaler.rs`, `tools/byro-launcher/src/preflight.rs`: no commits; guards spot-checked).

# Safety Audit — ByroRedux — 2026-10-21

**Command**: `/audit-safety` (default delta scope), run inside the 2026-10-21 comprehensive suite
**Severity scale**: `.claude/commands/_audit-severity.md`

## Method

- **Scoping.** `git log fc9622f6f..HEAD -- <Paths>` per dimension. Commits per dimension: Dim 1 = 0, Dim 2 = 2 (`Cargo.toml` only), Dim 3 = 5, Dim 4/5 = 17 (`crates/renderer/src/vulkan`), Dim 6 = 6, Dim 7 = 7, Dim 8 = 2.
- **No sub-agents. No source edited. No engine or GPU process launched.** Scratch notes are in `/tmp/audit/safety/dims.md`. Cargo used rustc 1.96.0 (pinned by `rust-toolchain.toml`) and the repo `target/`.
- **Commands run at HEAD:**
  - `cargo test -p byroredux-renderer --lib`: **1458 passed, 0 failed, 1 ignored.** This includes `gpu_terrain_tile_is_208_bytes`, the terrain-tile offset and ledger pins, `scene_descriptor_reflection`, `every_shader_struct_is_classified`, `name_diverging_glsl_rust_mirrors_stay_in_lockstep`, `a_failed_layout_submit_leaks_instead_of_destroying`, `every_created_resource_is_pushed_before_anything_else_runs`, the `allocator_lock` / `teardown_path` poison guards and `cloud_column_threshold_selects_the_coverage_fraction_of_columns`.
  - `cargo test -p byroredux --bin byroredux -- rapier_release bone_palette_overflow allocator_teardown_order release_ no_source_scan unload_walk greyscale swap displaced height`: **128 passed, 0 failed.**
  - `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks`: **exit 0.** The #1904 `deny` is still at `crates/renderer/src/lib.rs:21` and nothing `allow`s it. CI still has both the workspace `--keep-going` step and the dedicated renderer re-lint (`.github/workflows/ci.yml:207,217,228`).
  - `cargo clippy -p byroredux-fsr3-sys -p byroredux-nif -p byroredux-core -p byroredux-pex -p byroredux -p byro-launcher -p byroredux-ui -p byroredux-terrain-height --no-deps -- -A clippy::all -W clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc`: the only output is the three known `unsafe impl AnyBitPattern` adjacency false positives (`crates/nif/src/blocks/bs_geometry.rs:364-366`). Nothing new.
  - `cargo tree -p byroredux-mod-runtime | grep -ic wasi`: **0**. `grep -c require_ crates/mod-runtime/src/runtime/capabilities.rs`: **22** (unchanged).
- **Dedup.** Sources checked: `/tmp/audit/issues.json` (119 open), `/tmp/audit/SUITE_FINDINGS.md` (read at start and again before finalizing), and the renderer, exterior and concurrency scratch notes for this suite. Closed-issue lookups: #6062, #6063, #6064, #6033, #5122, #3564 and #4956. The baseline's three findings are all fixed: #6062 (`e4d86a29f`), #6063 (`4a6128389`) and #6064 (`2a6f8be2b`). The lock-order lane is red again, but on two new cycles that ECS-2026-10-21-01/02 already report, so this report does not repeat them.

## Census (re-measured)

| Scope | Tokens | `unsafe {` blocks | `unsafe fn` | `unsafe impl` | Delta vs 10-19 |
|---|---|---|---|---|---|
| `crates/renderer/src` | **943** (932) | **716** (711) | **100** (98) | **38** (35) | +5 `vulkan/sky_cube/cloud_shadow.rs` (+ `unsafe fn record` / `destroy`), +2 `gpu_timers.rs` (shadow-filter bracket), +1 `shadow_filter.rs` (#6063 destroy arm), −3 ground-cover hand-rolled byte views (#6033), +3 `NoUninit` impls (`ScatterPush`, `BladePush`, `BenchPush`) |
| `crates/fsr3-sys/src` | 12 | 10 | 2 | 0 | unchanged |
| `crates/nif` / `core` / `pex` | 13 / 7 / 4 | 1 / 6 / 1 | — | 4 / 0 / 0 | unchanged |
| `byroredux` / `tools/byro-launcher` | 5 / 4 | 2 / 1 | 0 / 1 | — | unchanged |
| `crates/plugin` | 4 | 2 (test env edits) | — | — | unchanged |
| `crates/terrain-height` (**new**, `5017a64c2`) | 0 | 0 | 0 | 0 | No `unsafe`. Like every unsafe-free crate except `crates/sdk`, it does not declare `forbid(unsafe_code)`. Its dependency `rustfft 6` uses SIMD `unsafe` internally; that is third-party code. |
| every other crate / tool | 0 `unsafe {fn,impl,trait,extern}` matches | | | | no newcomer |

The renderer delta reconciles exactly: +5 +2 +1 −3 = +5 blocks.

## Findings summary

| ID | Severity | Dim | Status | Title |
|---|---|---|---|---|
| SAFE-2026-10-21-01 | LOW | 2/4 | NEW | Push-constant blocks outside ground cover are still hand-cast and not mirror-guarded. #6033 closed without the shadow-filter `Push` that the 10-19 report had added to its scope. |

Counts: **0 CRITICAL · 0 HIGH · 0 MEDIUM · 1 LOW** (1 NEW, 0 regressions). The notes after the finding are not counted.

## Findings

### SAFE-2026-10-21-01: Push-constant blocks outside ground cover are still hand-cast and not mirror-guarded; #6033 closed without the shadow-filter `Push`
- **Severity**: LOW. Every remaining cast is sound today. The gap is in tracking and guarding, not a live defect.
- **Dimension**: Dim 2 (`NoUninit` byte views) / Dim 4 (does the stated invariant stay true).
- **Location**: push-constant byte views at:
  - `crates/renderer/src/vulkan/shadow_filter.rs:382`
  - `crates/renderer/src/vulkan/svgf.rs:1246`
  - `crates/renderer/src/vulkan/presentation.rs:646`
  - `crates/renderer/src/vulkan/water.rs:789`
  - `crates/renderer/src/vulkan/skin_compute.rs:799` and `:1262`

  Bulk byte views at `crates/renderer/src/vulkan/morph_compute.rs:262`, `scene_buffer/upload.rs:750`, `mesh/geometry_ssbo.rs:690` / `:720` and `mesh.rs:1293` / `:1302`. The mirror walk is `every_shader_struct_is_classified` in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2117`.
- **Status**: NEW. This was a counted-out note in `AUDIT_SAFETY_2026-10-19.md` ("Add `Push` to #6033's mirror-table / `NoUninit` routing scope; no separate issue"). #6033 (`02b828fff`) then closed, routing only the three ground-cover blocks through `byte_view` and the mirror table, so the residue is now tracked nowhere. #5122 (closed) was the ground-cover-models instance. #3564 (closed) made the guards drive off a walk instead of a list, but that walk only sees `struct` declarations.
- **Description**: `byte_view` (`vulkan/buffer.rs:42-56`) documents itself as "the single place" byte views are taken, with one listed exemption (`hash_indirect_slice`). Twelve `std::slice::from_raw_parts` casts still bypass it. The six push-constant views are the ones that matter:
  - The structs are `#[repr(C)]`, all-scalar and padding-free: `Push` {`[u32;2]`, `u32`, `u32`}, `AtrousPush`, `PresentationPushConstants`, `WaterPush`, `SkinPushConstants`, `SkinPalettePushConstants`. So each cast is sound, and each `// SAFETY:` comment is true today (the clippy gate confirms the comments are present).
  - None of them has a `NoUninit` impl. A field later added at a different alignment would make the cast read uninitialised padding, and nothing would flag it.
  - Their GLSL counterparts are `layout(push_constant) uniform …` blocks. The classification walk matches only lines starting with `struct `, so those blocks are invisible to it.
  - A reorder or a `uint`↔`float` swap therefore reaches the GPU with a green suite. That is the exact hazard #6033 closed for ground cover, still open for the shadow filter, à-trous, presentation, water and skinning blocks.

  Push-range sizes are not at risk: every layout's range is `size_of::<T>()`, the same value the cast uses.
- **Evidence**: `shadow_filter.rs:49-53` (`struct Push { screen: [u32; 2], step: u32, flags: u32 }`, no `NoUninit` impl) and `:382-385` (`from_raw_parts((&push as *const Push).cast::<u8>(), size_of::<Push>())`). `shader_contract_tests.rs:2266-2275` (`line.strip_prefix("struct ")` is the only declaration matcher). A grep finds no `unsafe impl NoUninit for Push` / `AtrousPush` / `WaterPush` / `PresentationPushConstants` / `Skin*PushConstants`.
- **Impact**: None today. The value of fixing it is guard coverage: per-field corruption in a denoiser, presentation or skinning pass would show up as a visual regression rather than a test failure.
- **Related**: #6033, #5122, #3564, #4956. The two `geometry_ssbo.rs` casts say "mirrors `create_device_local_buffer`'s identical cast", which already requires `Vertex: NoUninit`, so they can route through `byte_view` with no new impl.
- **Suggested Fix**: Add `unsafe impl NoUninit` for the six push structs and replace the casts with `byte_view(std::slice::from_ref(&push))`, as #6033 did. Extend the classification walk to `layout(push_constant) uniform <Name>` blocks, putting each in `name_diverging_glsl_rust_mirrors_stay_in_lockstep` or marking it `ShaderLocal`. Then re-tighten `byte_view`'s "single place" doc to list the remaining bulk-slice exemptions.

## Notes that are not findings

- **Cloud-shadow readback initialisation is not flushed** (`sky_cube/cloud_shadow.rs:152`). The "clear sky until the first bake" seed writes `1.0` into a `GpuToCpu` buffer and never calls `flush_if_needed`. On memory that is not `HOST_COHERENT`, a later `invalidate_if_needed` leaves the unflushed bytes undefined. That includes the superset-widened invalidate of the sibling slot's 16-byte readback, which very likely shares a non-coherent atom with it (`buffer.rs:1260-1288`). The reader at `:261-266` rejects non-finite values and clamps the rest to `[0, 1]`. The worst case is one or two frames of wrong camera-column sun transmittance at startup, and only on non-coherent `GpuToCpu` heaps. Desktop NVIDIA and AMD expose those heaps as coherent. One `flush_if_needed` after the seed write would close it.
- **Doc rot**:
  - `crates/renderer/src/vulkan/groundcover_bench.rs:1623` still documents `push_bytes`, which #6033 deleted.
  - `CloudShadow::record` / `destroy` state their contract as plain `Safety:` text instead of a `# Safety` heading. They are reachable only from `pub(super)`, so `missing_safety_doc` cannot fire.
  - `Cargo.toml:126-134`: `rustfft` / `twox-hash` were inserted between the "FxHash for the render hot-path" comment and `rustc-hash`, so that comment now heads three crates.

  Owner `/audit-tech-debt`.
- **Existing, unchanged**: #6146 (the other eight `initialize_layouts` callers still carry the destroy-on-any-error premise that #6063 disproved for the shadow filter). Other audits in this suite already cover CONC-2026-10-21-01 / -02 (terrain-height pool panics and temp-file name collision), ECS-2026-10-21-01 / -02 (the lock-order lane is red again) and EXT-D4-2026-10-21-01 / -03 (cloud-shadow consumer coverage).

## Verified intact (regression guards and focus items)

1. **Dim 1 — FFI** (no commits). `crates/cxx-bridge` still exposes only `native_hello() -> String`. `crates/fsr3-sys/src/lib.rs` keeps three `# Safety` sections, and its one `CStr::from_ptr` reads the shim's static error string. No new `CStr::from_ptr` exists on a fixed-size `[c_char; N]`: the `fsr3-sys/examples` hits are the smoke harness, and `vulkan/debug.rs:63` reads the spec-null-terminated `p_message`.
2. **Dim 2 — UB.**
   - The `lz4_flex` pin is intact: `default-features = false`, with `std`, `safe-encode`, `safe-decode`, `frame` and `checked-decode`, and `byroredux-bsa` is the sole dependent (`workspace = true`).
   - `OpCode::from_u8` is unchanged: `MAX_OPCODE = 51` with a const-asserted last discriminant. The NIF `read_pod_vec_from` site is unchanged.
   - The new `NoUninit` impls (`ScatterPush` 48 B, `BladePush` 64 B, `BenchPush` 20 B) are `#[repr(C)]` over 4-byte scalars with no padding, pinned by size tests and by the #6033 field-for-field mirror rows.
   - The new `CausticParams.sun: [f32; 4]` keeps the struct padding-free. Its GLSL side (`vec4 causticSun`) matches, and the UBO is sized by `create_per_frame_uniforms::<CausticParams>`.
   - `CompositeParams` gained `cloud_origin` mid-struct. Every shifted offset is re-pinned (`cloud_origin` 496, `sky_aperture_count` 512, `sky_apertures` 528).
3. **Dim 3 — leaks / drop order.**
   - **#6195 terrain height maps** (`byroredux/src/terrain_height.rs`): each bound height handle is one registry acquire, owned by the tile record. Lanes bound at spawn, and lanes patched later by `apply_ready_heights` (only into a lane that is still `0`, and only while the slot is live), are both released on unload through `GpuTerrainTile::texture_indices` (`unload.rs:512-520`, which now chains `base_height_index` and `layer_height_index`). A full-table spawn failure releases them through `splat_tex_indices` (`terrain.rs:1368-1369`, `:1416`). No early exit follows the tile allocation.
   - Generation is bounded. The DDS decoder caps edges at 8192 (`crates/menuxml/src/tex.rs:103`), and `byroredux-terrain-height` reduces to `MAX_EDGE = 1024` before the FFT (`lib.rs:34,93-120`), so an oversized modded source cannot inflate the worker allocation.
   - #6110: a texture swap takes one ref per matched mesh and returns the displaced one, with the same 0/fallback skip rule as unload. #6112 makes the released-handle guard live. #6132 detaches swept pickup colliders before `despawn_batch`.
   - `release_victim_rapier_bodies` and `rapier_release_tests` pass. `DeferredDestroyQueue<T>` production instantiations are unchanged. `AllocatorResource` teardown ordering is untouched (`app_events.rs` changed only for the #5959 telemetry column), and `allocator_teardown_order` passes.
   - The `CloudShadow` constructor and its partial cleanup are sound. It submits nothing, every handle is retained before the next fallible call, `destroy` tolerates null handles, and `SkyCubePipeline::destroy` destroys it first (`sky_cube.rs:616`).
4. **Dim 4 — discipline.** Every new block carries a SAFETY comment, and its stated invariant holds at the call site:
   - `cloud_shadow.rs`: construction-time descriptor writes, the private pool, and the "never submitted" destroy.
   - `gpu_timers.rs`: `cmd_write_timestamp` at slots 46/47, inside `QUERIES_PER_FRAME = 58`. A const assert ties the geometry-phase base to the new pair, and readback stays non-`WAIT`.
   - The #6063 arms in `shadow_filter.rs` and `context/resize.rs` destroy only when `OneTimeCommandError::may_be_in_flight` answers false, and `forget` otherwise. They are pinned by `a_failed_layout_submit_leaks_instead_of_destroying`.
   - #6064 restored `memory_barrier`'s doc and `# Safety`, and `clamp_sampler` now carries its own.
5. **Dim 5 — Vulkan spec.**
   - **Cloud-shadow bake** (`CloudShadow::record`):
     - The bake transitions UNDEFINED→GENERAL (FRAGMENT→COMPUTE, a WAR against this slot's fenced readers), dispatches, then transitions GENERAL→`SHADER_READ_ONLY_OPTIMAL` (COMPUTE→FRAGMENT).
     - Every consumer is a fragment shader (`lighting.glsl` via `triangle.frag`, `water.frag`, `groundcover_blade.frag`). `volumetrics_inject.comp` only names `lighting.glsl` in a comment.
     - A separate SHADER_WRITE→HOST_READ buffer barrier covers the camera-column readback (#2740). The host reads it in `begin_frame_recording` only after that slot's in-flight fence wait.
     - The bake is recorded unconditionally before the geometry pass (`build_and_upload_instances.rs:1216`). Scene binding 24 is `PARTIALLY_BOUND`, left unwritten only when the sky cube failed, and the shader reads it only behind `exteriorSkyTint.w` and `jitter.w`.
     - The UBO block `SkyCubeParams { SkyDome dome; }` is identical in `cloud_shadow.comp` and `sky_cube.comp`.
   - The renderer reflection suite (`scene_layout_matches_triangle_shaders` / `_water_shaders`) passes with binding 24.
   - #6107 (no static BLAS for a bound skinned mesh) cannot leave an instance without geometry. A skinned mesh that falls back to rigid drawing gets a static BLAS lazily from `restore_missing_static_blas_for_draws`, and `acceleration/` changed only in #6035's budget arithmetic.
6. **Dim 6 — GPU layout.**
   - `GpuTerrainTile` grew 176 → 208 B (#6195 height handles plus `height_blend_band` in the old tail pad). The size, per-field offsets, GLSL lockstep, shipped `triangle.frag.spv` `ArrayStride 208` and memory-budget ledger pins all moved together and pass. The upload's SAFETY comment names the new pin.
   - `GpuMaterial`, `GpuInstance`, `GpuCamera` and `GpuLight` are unchanged.
7. **Dim 7 — GPU-fed data.**
   - `height_blend_band` comes from a registry slider (`0.0..=1.0`, step 0.01). `SettingsRegistry::set` rejects non-finite or out-of-range numbers (`crates/core/src/settings/mod.rs:293`), so `terrain.blend inf` / `nan` fail at the registry. The shader additionally gates on `band > 0.0` and floors at `1e-4`.
   - `camera_transmittance` returns only finite values clamped to `[0, 1]` before they reach `CausticParams.sun` or the volumetrics sun.
   - The height-blend loops are bounded by `TERRAIN_SPLAT_LAYERS`.
   - The first-sight `bind_inverses` rollback check (`app_frame.rs:714`) and `SkinSlotPool` overflow handling are untouched. Glass recursion guards pass with the shader-constant tests.
8. **Dim 8 — mod runtime.** No WASI is in the tree, the `require_*` count is unchanged, and `wasmtime` features are unchanged. The only extensions change, #5994, routes activation events through `papyrus_activations`, so a native-only activation no longer reaches extension handlers. That narrows what guests see.

## Out of scope / not run

- No game data, GPU or engine process was used. Nothing here covers the validation layers on a real cell load. The lavapipe CI lane boots only the 5-frame default scene.
- The `BYRO_LOCK_ORDER_CHECK=1` bin run was not repeated: ECS-2026-10-21-01/02 already report the lane red on two new cycles.
- Cloud-shadow *consumer* correctness (which lighting paths apply it) belongs to the exterior audit (EXT-D4-2026-10-21-01/-03). Only its lifetime, barrier and descriptor surface was audited here.

Publish with `/audit-publish docs/audits/AUDIT_SAFETY_2026-10-21.md`.
