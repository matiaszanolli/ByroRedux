**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-10-05.md` (@ `a2c24b16e`, 116 commits ago) · **Audited**: Dims 1–7 (each had commits on its `Paths:` since the baseline) · **Unchanged since baseline (skimmed)**: Dim 8 (sandboxed mod runtime). No commits touched `crates/mod-runtime` or `crates/sdk`. The only path deltas were an `extensions/commands.rs` routing fix, the deletion of a test helper, and a `kira` feature line in `Cargo.toml`, so Dim 8 got a guard spot-check only.

# Safety Audit — ByroRedux — 2026-10-08

**Command**: `/audit-safety` (default delta scope), one leg of `/audit-suite --preset comprehensive`
**Severity scale**: `.claude/commands/_audit-severity.md`

## Method

- **Scoping.** `git log a2c24b16e..HEAD -- <Paths>` was run for each dimension:

  | Dim | 1 | 2 | 3 | 4 / 5 | 6 | 7 | 8 |
  |---|---|---|---|---|---|---|---|
  | Commits | 3 | 1 | 6 | 12 | 2 | 8 | 3 |

- **No sub-agents.** Every dimension was analysed synchronously. Per-dimension notes are in `/tmp/audit/safety/dim_{0..8}.md`; `dim_0` is the census.
- **Commands run.** No engine or GPU process was launched and no source was edited. Cargo ran through the rustc 1.96.0 toolchain binary, per CLAUDE.md.
  - `cargo test -p byroredux --bin byroredux -- rapier_release bone_palette_overflow allocator_teardown_order release_`: **45 passed, 0 failed**.
  - `cargo test -p byroredux-renderer --lib -- gpu_material gpu_instance gpu_camera gpu_light gpu_terrain volume_far depth_capture shader_constants triangle_frag_scales bindings_glsl the_pin_test scene_descriptor_reflection allocator_lock teardown_path frames_in_flight one_time scratch every_shader_struct`: **170 passed, 0 failed**.
  - `cargo clippy -p byroredux-renderer -p byroredux-fsr3-sys -p byroredux-nif -p byroredux-core -p byroredux-pex -p byroredux -p byro-launcher -p byroredux-ui --no-deps -- -A clippy::all -W clippy::undocumented_unsafe_blocks -W clippy::missing_safety_doc`: only the 3 known `unsafe impl` adjacency false positives fired (`crates/nif/src/blocks/bs_geometry.rs:364-366`).
  - `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks`: clean.
  - `cargo tree -p byroredux-mod-runtime | grep -ic wasi`: **0**.
- **CI evidence.** HEAD run `37848932617` @ `00f580e09`, job `113556725946` (Vulkan validation layers, lavapipe). The full log is in `/tmp/audit/safety/vk_37848932617.log`. Job conclusions were also read for the previous run, `37833008690`.
- **Dedup.** Checked against:
  - `/tmp/audit/issues.json` (113 open issues);
  - closed and all-state `gh` searches for `DefineSprite` recursion, `normalize_scaleform_dialect` and the `chunks_exact` SAFETY text;
  - the state of every issue the baseline report cited (#5270, #5272, #5273, #4119, #4268, #5368, #5308);
  - today's sibling reports, `AUDIT_CONCURRENCY_2026-10-08.md` and `AUDIT_PERFORMANCE_2026-10-08.md`, by title.

## Census (re-measured at HEAD)

| Crate | Word tokens | `unsafe {` blocks | `unsafe fn` | `unsafe impl` | Δ vs baseline |
|---|---|---|---|---|---|
| `crates/renderer/src` | 939 | 720 | 96 | 35 | **none** (816 SAFETY mentions, also unchanged) |
| `crates/fsr3-sys/src` | — | 10 | 2 `pub` (`create`, `dispatch`), both with `# Safety` | 0 | none |
| `crates/nif/src` | 13 | 1 (`read_pod_vec_from`) | 0 | macro + 3 (`bs_geometry.rs`) | none |
| `crates/core/src` | 7 | 6 | 0 | 0 | none |
| `crates/pex/src` | 4 | 1 (`transmute`) | 0 | 0 | none |
| `byroredux/src` | 5 | 2 | 0 | 0 | none |
| `tools/byro-launcher/src` | 3 | 1 | 1 | 0 | none |
| `crates/plugin/src` | 4 | 2 (test-only) | 0 | 0 | none |
| `crates/cxx-bridge` | 1 | 0 | 0 | 0 | still only `native_hello() -> String` |

- **Renderer.** Across the 116 commits, the only diff lines that carry the word `unsafe` are three `unsafe impl NoUninit for GpuFog{Volume,VolumeUpload,ClusterEntry}`. They moved verbatim from `vulkan/volumetrics.rs` to `vulkan/volumetrics/fog_clusters.rs` in the #5094 split. No `transmute`, `from_raw_parts`, `set_len` or raw-pointer cast was added anywhere in the tree.
- **`crates/fsr3-sys/examples/vulkan_context_smoke.rs`** holds 20 more blocks. They predate this window and every one is commented. The skill's census counts `src/` only.
- **No unsafe newcomer** appears in any unsafe-free crate. Only `crates/sdk` carries `#![forbid(unsafe_code)]`.
- **`tools/nifskope`** is an untracked local C++ checkout, ignored by `/tools/*` in `.gitignore`. It is not a workspace member and is out of scope.

## Findings summary

| ID | Severity | Dim | Status | Title |
|---|---|---|---|---|
| SAFE-D2-2026-10-08-01 | LOW | 2 | NEW | The #4470 Scaleform dialect shim recurses once per nested `DefineSprite` with no depth cap, before Ruffle ever sees the bytes |
| SAFE-D4-2026-10-08-01 | LOW | 4 | NEW | `load_shader_module`'s SAFETY text still cites "the `chunks_exact(4)` decode above", which #5308 rewrote to `as_chunks::<4>()` |

Counts: **0 CRITICAL · 0 HIGH · 0 MEDIUM · 2 LOW** (2 NEW, 0 regressions). **Already tracked and still open** (not re-filed): #5272 (LOW), #5273 (LOW) and #4119.

## Findings

### SAFE-D2-2026-10-08-01: The #4470 Scaleform dialect shim recurses once per nested `DefineSprite` with no depth cap, before Ruffle ever sees the bytes
- **Severity**: LOW.
  - A stack overflow aborts the process and cannot be caught. That is why the Dim 2 stack-overflow bullet asks for this class to be reported.
  - The input is game-archive SWF data, not network input.
  - The pinned Ruffle `swf` reader (`0dde981`, `swf/src/read.rs:594` → `read_define_sprite` :1824 → `read_tag_list` :711) also recurses on nested sprites with no limit. So the shim does not open a new crash class; it moves the first overflow site into our code.
- **Dimension**: Memory Corruption / UB (stack-overflow risk)
- **Location**: `crates/ui/src/prepare.rs:111-155` (`patch_place_object3_stream`, recursive call at `:146`). It is reached from `prepare.rs:206` (`prepare_movie`, every root movie) and `crates/ui/src/navigator.rs:530` (every dependency movie).
- **Status**: NEW. It was introduced by `9813af435` (#4470, CLOSED). No issue or audit names it; searched `DefineSprite recursion` and `normalize_scaleform_dialect`.
- **Description**:
  - `normalize_scaleform_dialect` now runs on **every** SWF the engine loads, not only Starfield's.
  - Its tag walker recurses into each `DefineSprite` body (`code == 39 && len >= 4`) at any depth.
  - The SWF spec does not allow `DefineSprite` inside `DefineSprite`, but the walker does not enforce that. A crafted or corrupt movie can nest sprites at about 10 bytes per level (a 6-byte long-form header plus a 4-byte id and frame count).
  - At that rate, a few hundred KB to about 1 MB of input exhausts a 2–8 MiB thread stack.
  - Every slice index in the walker is bounds-checked (`p + 2`, `p + 6` and `body_end <= stream.len()` guards), so the depth is the only unbounded resource. The other is `decompress_zlib_after_header`'s uncapped `read_to_end` (`:85-92`); Ruffle's own `decompress_swf` has the same property.
- **Evidence**:
  ```rust
  } else if code == define_sprite && len >= 4 {
      // DefineSprite body: character id u16 + frame count u16, then a
      // nested (END-terminated) tag stream.
      patched += patch_place_object3_stream(
          &mut stream[body_start + 4..body_end],
          define_sprite,
          place_object_3,
      );
  }
  ```
- **Impact**: A malformed or hostile `.swf` in a mod archive crashes the engine at menu or HUD load with SIGSEGV (stack overflow). Retail content never nests sprites, so there is no effect on vanilla data.
- **Related**: #4470. `/audit-ui` owns the Scaleform host, and `/audit-parsers` owns parser discipline for untrusted input; flag this to both.
- **Suggested Fix**: Since nested sprites are illegal SWF, pass a `depth` argument and stop recursing past depth 1, so no sprite inside a sprite is walked. Alternatively, convert the walk to an explicit work-list. Optionally cap `read_to_end` at the header's declared `uncompressed_len`, which is a u32 at `[4..8]`. Neither change alters retail behaviour.

### SAFE-D4-2026-10-08-01: `load_shader_module`'s SAFETY text still cites "the `chunks_exact(4)` decode above", which #5308 rewrote to `as_chunks::<4>()`
- **Severity**: LOW. This is doc rot inside a safety justification. The invariant is still true: the decode builds an owned `Vec<u32>`, which is u32-aligned by construction, and `spv.len().is_multiple_of(4)` is asserted first.
- **Dimension**: Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:24-37`. The decode is at `:24-28` and the SAFETY comment at `:33-36`.
- **Status**: NEW. #5308 (CLOSED, `b24cb46b6`) changed the code and missed the comment. Its three sibling rewrites (`context/depth_capture.rs:100`, `context/screenshot.rs:91`, `groundcover/frame.rs:183`) have no SAFETY text that names the old API.
- **Description**: The skill states that the audit's value is the truth of each SAFETY claim. This one now justifies alignment by a call that no longer exists in the function. The next reader who checks the claim will not find it.
- **Evidence**:
  ```rust
  let code: Vec<u32> = spv
      .as_chunks::<4>().0
      .iter()
      .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
      .collect();
  ...
  // SAFETY: `device` is the live logical device; `create_info` borrows
  // `code`, which outlives this call, and the SPIR-V was 4-byte aligned
  // by the `chunks_exact(4)` decode above; ...
  ```
- **Impact**: None at runtime. It is a misleading justification on an FFI call.
- **Related**: #5308.
- **Suggested Fix**: Reword the comment to "`code` is an owned `Vec<u32>` (u32-aligned), decoded from a length asserted to be a multiple of 4". This states the real invariant rather than naming an API.

## Already-tracked items touched by this window (not re-filed)

- **#5272 (OPEN; SAFE-D3-2026-10-05-01).** #5311 (`12ca34a70`) split `crates/physics/src/world.rs` into `world/{mod,queries,recovery}.rs`. `remove_body` moved verbatim to `crates/physics/src/world/mod.rs:430` and still prunes only `body_labels`. `explosion_offences` (`mod.rs:335`, inserted at `recovery.rs:350`) and `keyframe_refusals_logged` (`mod.rs:307`, inserted at `recovery.rs:242`) still grow for the whole session. The sibling #5355 (`articulation_joints` never pruned) is also open; `/audit-physics` owns it.
- **#5273 (OPEN; SAFE-D4-2026-10-05-01).** Unchanged. A re-count puts the renderer's `CStr::from_ptr(…device_name…)` blocks at **four**: `crates/renderer/src/vulkan/device.rs:466`, `:481`, `:524` (selected-GPU log) and `:653`. The baseline listed three. The fix should cover all four.
- **#4119 (OPEN).** `queue_increment_own_i64` still lacks the visibility check (Dim 8).

## Prior-report findings: status at HEAD

| Prior ID | Issue | Status at HEAD |
|---|---|---|
| SAFE-D5-2026-10-05-01 (one-time helper frees on `MaybeInFlight` wait failure) | #5270 CLOSED | **Fixed** (`d194f7d60`); details below. |
| SAFE-D3-2026-10-05-01 (`remove_body` prunes 1 of 3 maps) | #5272 OPEN | Unchanged; the code moved files. |
| SAFE-D4-2026-10-05-01 (renderer `CStr::from_ptr(device_name)`) | #5273 OPEN | Unchanged, at 4 sites. |
| CONC-D3-2026-10-05-01 (lavapipe lane red on non-errors), cross-referenced | fixed | **Fixed.** `ci.yml:409` now lifts only `byroredux_renderer::vulkan::device` to info. HEAD job `113556725946` is **success**, with **0** `[Vulkan]` emissions (the 5 grep hits are the script's own echoed lines) and 0 `VUID-` / `Validation Error` / `SYNC-HAZARD` lines. The "Assert renderer-static scene-state determinism" step **ran** and passed. |
| CONC-D2-2026-10-05-01 (CI demo scene had RT off), cross-referenced | fixed (#5261 gate) | **Fixed.** HEAD lavapipe logs `rt-integrity: … rt_supported=1 rt_flag=1 tlas_build=1 tlas_emitted=4`, so ray-query consumers run under validation again. The lane still boots only the 5-frame default scene and covers no game data. |
| #4268 (Starfield `.mesh` bone indices unbounded), Dim 7 cross-reference | CLOSED | **Fixed** (`0abc86c8b`). `crates/nif/src/import/mesh/skin.rs` declines the whole skin when any `bone_index >= bone_refs.len()` (bind-pose fallback, no clamp). |

**The #5270 fix was verified in detail** (`crates/renderer/src/vulkan/texture.rs`, the wait arm of `with_one_time_commands_inner`):
- The arm branches on `vk::Result::ERROR_DEVICE_LOST`. Only that sub-arm frees `cmd` and destroys the owned fence, which is valid per the spec's Lost Device section.
- On any other error (the OOM codes), `cmd` and the owned fence are leaked. Both are reclaimed when the pool or device is destroyed, and `VulkanContext::drop` waits for the device to go idle first.
- On the reusable-fence path, the mutex is poisoned by a **caught** panic while the `MutexGuard` is alive. The `move` closure owns the guard, and the guard drops during unwinding under `thread::panicking()`, which sets the poison flag. The next `.lock().expect` (#5209) refuses to reuse the fence, and teardown recovers through `lock_recovering`.
- `catch_unwind` is effective in release because `Cargo.toml:311` pins `panic = "unwind"`, with a do-not-change note (#1383).
- Pins `wait_failure_arm_disposes_only_on_device_loss` and `one_time_commands_free_cmd_buffer_on_every_error_path` are green.

## Per-dimension results

### Dimension 1 — FFI lifetime safety: PASS, 0 new
- **`fsr3-sys`, launcher preflight, `cxx-bridge`.** No commits. `Context::create` and `Context::dispatch` keep their `# Safety` sections (`crates/fsr3-sys/src/lib.rs:377/416`), and the Drop note is at `:499`.
- **`frame_upscaler.rs`.** Only its test modules changed (`655b317c9`, which moved them to `source_scan::production_text`).
- **`crates/ui`.** The #4470 dialect shim is safe Rust that byte-patches an owned `Vec`. No borrowed or raw data crosses into the renderer. Its recursion depth is SAFE-D2-2026-10-08-01.

### Dimension 2 — Memory corruption / UB: 1 LOW
- **`Cargo.toml`.** The only change is `kira = { version = "0.10", features = ["wav", "ogg"] }` (`f8950e7cc`), which pulls in safe-Rust decoders. No `unsafe` was added to the tree.
- **LZ4 pin intact.** `Cargo.toml:165` sets `default-features = false` with an explicit list that names `safe-decode`, and `crates/bsa` is the sole dependent (`workspace = true`).
- **pex opcode.** The const assert (`crates/pex/src/opcode.rs:74`) and the runtime bound (`:137`) are intact.
- **NIF POD read.** `read_pod_vec_from` (`crates/nif/src/stream.rs:800-829`) and the big-endian gate (`:21`) are unchanged.
- **Finding:** SAFE-D2-2026-10-08-01.

### Dimension 3 — Leaks and drop ordering: PASS, 0 new (#5272 still open)
- **Guard tests green.** These include `release_sweeps_both_ragdoll_and_rapier_handles`, which still asserts `body_count`, and both `allocator_teardown_order_tests`.
- **Deferred destroy.** Still four production `DeferredDestroyQueue<T>` instantiations: `crates/renderer/src/mesh.rs:381`, `crates/renderer/src/vulkan/scene_buffer/buffers.rs:148` and `crates/renderer/src/vulkan/acceleration/mod.rs:274/308`.
- **#5310 (`ec0e0c8b4`) `detach_victims_from_surviving_parents`** (`byroredux/src/cell_loader/unload.rs`). It reads `Parent`, drops that guard, then makes one `Children` write, which respects lock order. It closes a per-release dangling-child growth on the #5028 gear path. Its doc says it also drops the `Parent` row; in fact `despawn_batch` does that, which is harmless.
- **#3817 (`63bf3347f`) `CinematicReAdoption.pending`** (`byroredux/src/systems/cinematic.rs:482-640`). The list is bounded by the released convoy population. Entries leave it either on adoption into a loaded cell root or when the entity disappears (no `Transform`). This is not per-frame growth.

### Dimension 4 — Unsafe-block discipline: 1 LOW
- **Guard intact.**
  - `#![deny(clippy::undocumented_unsafe_blocks)]` is at `crates/renderer/src/lib.rs:21`, with no `allow` escapes anywhere in `crates/`.
  - Both CI steps survive: `ci.yml:190` (`--keep-going`) and the dedicated renderer gate at `:201`. The gate step was green at HEAD even though the workspace clippy step was red.
  - Locally, every unsafe-bearing crate plus `crates/ui` is clean, apart from the 3 known nif false positives.
- **Invariant truth on changed blocks.** The #5270 wait arm is true (see above). The #5250 `tlas.rs` change touches no `unsafe` token. The volumetrics split carried its SAFETY texts with the moved code.
- **Finding:** SAFE-D4-2026-10-08-01. #5273 is still open.

### Dimension 5 — Vulkan spec compliance: PASS, 0 new
- **#5270.** Fixed; see above.
- **#5250 (`72028d4fd`).** `tlas_scratch_peak_bytes[frame]` now records `max(build_scratch_size, update_scratch_size)`, so `shrink_tlas_scratch_to_fit` can no longer cut a slot below what its refit path needs. This complements #5195's allocation-side max. The scratch tests are green.
- **#5249 / #5368.** The new `traceShadowTransmittanceSkippingInstance` loop (`crates/renderer/shaders/include/shadow_transport.glsl:200`) is bounded by `MAX_TRANSMISSION_SELF_SKIPS = 8`. That constant is defined at `crates/renderer/src/shader_constants_data.rs:120` and emitted into the generated header. Before #5368 it was undefined, and the committed `.spv` pair was stale. The shader-parity CI job is green at HEAD.
- **Live validation.** The lavapipe lane is green at HEAD, with RT on and zero validation messages (see the prior-findings table).
- **Out of scope here.** HEAD CI's `Test + Check + Clippy` (`cargo test`, workspace clippy) and the ABBA lock-order lane are **red** at `00f580e09`. Neither failure is safety-owned; the lock-order lane belongs to `/audit-concurrency`.

### Dimension 6 — GPU struct layout: PASS, 0 new
- **No commits** to `scene_buffer/gpu_types.rs`, `material.rs` or `bindings.glsl`.
- **Pins green.** The size pins and the four `gpu_material_size_claims` tests pass. So does `every_shader_struct_is_classified`. The contract test now concatenates the split `volumetrics/` files and accepts `pub(crate)` identifiers.
- **Instance-index convention.** #5249's own-instance skip compares the hit's custom index with `fragInstanceIndex` (= `gl_InstanceIndex`, the SSBO index). The TLAS packs the SSBO-compacted index (`crates/renderer/src/vulkan/acceleration/tlas.rs:756`), the same convention as the existing `terminusOnSelf` check (`triangle.frag:2483`). There is no index-space mismatch.

### Dimension 7 — GPU-fed data and loop bounds: PASS, 0 new
- **Glass loop.** Bounded at `triangle.frag:2317`; `triangle_frag_scales_glass_interface_depth_with_honest_ray_cost` is green.
- **#5230 (`9f0a8a7cc`).** It only exempts forced mirror panes from the neutral-roughness pass. `resolve_pbr` still runs in the translate path, so no NaN escapes.
- **#3991 latch.** `byroredux/src/app_frame.rs:727` still reads `skin_state_submitted || bind_inverse_upload_failed`, and its pins are green. The bone-palette overflow tests are green. The #5367 dialogue helper added to `app_frame.rs` touches no GPU data.

### Dimension 8 — Sandboxed mod runtime: unchanged (skimmed), 0 new
- **WASI.** None in the tree. The `wasmtime` feature list is unchanged (`Cargo.toml:119`).
- **Gating.** 22 `require_` guards in `crates/mod-runtime/src/runtime/capabilities.rs`, unchanged.
- **#5239 (`2464a52d7`).** Routes `SetBase` on player-derived pools into the modifier layer. It holds `CharacterRuleset` across the `ActorValues` query in the documented #3441 order.
- **#4119** is still open.

## Skill-sync notes (for the next `/audit-safety` edit, not findings)
- **Dim 5's First step** should drop the "lane red on noise" caveat. The lane is severity-correct (`RUST_LOG=error,byroredux_renderer::vulkan::device=info`) and green with RT on since #5261.
- **Dim 2's stack-overflow bullet** could name the Scaleform tag walkers: `crates/ui/src/prepare.rs` and the Ruffle reader.
- **The census note** could mention that `crates/fsr3-sys/examples/vulkan_context_smoke.rs` holds 20 commented blocks outside the `src/` count. CI clippy does not lint examples.
- **Census line for #5273:** four renderer `device_name` sites, not three.

Publish with: `/audit-publish docs/audits/AUDIT_SAFETY_2026-10-08.md`
