**HEAD**: 670f81917 · **Baseline**: `docs/audits/AUDIT_SAFETY_2026-10-27.md` (@ `fa6be4a6b`, 136 commits ago) · **Audited**: Dim 4/5 (`crates/renderer/src/vulkan`, 6 commits), Dim 3 (1 commit, `app_events.rs`), Dim 7 (4 commits touching its paths, shader/UI constants only), Dim 6 (comment-only) · **Unchanged since baseline (skimmed)**: Dims 1, 2, 8. Dim 1: no commits on `fsr3-sys`, `cxx-bridge`, `crates/ui`, `preflight.rs`, `frame_upscaler.rs`. Dim 2: no source diff (`Cargo.toml` / `Cargo.lock` unchanged). Dim 8: one `crates/sdk/src/relationships.rs` data change (#6504), no mod-runtime change.

# Safety Audit — ByroRedux — 2026-10-31

**Command**: `/audit-safety` (delta scope), run inside the 2026-10-31 comprehensive suite
**Severity scale**: `.claude/commands/_audit-severity.md`

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |

No new findings. The two 10-27 findings are both still present in the source and both have open issues, so neither is re-reported:

- SAFE-2026-10-27-01 is Existing #6506. `SceneBuffers::destroy` (`scene_buffer/descriptors.rs`) still has no `moved_surface_buffers` arm; the field is declared at `buffers.rs:237`.
- SAFE-2026-10-27-02 is Existing #6507. The two comment-less `set_var` blocks at `crates/plugin/src/esm/test_paths.rs:245` and `:249` are unchanged, and the file has no commits since the baseline.

The delta contains one large unsafe-adjacent refactor (#6358, `gpu_timers`), which was verified behavior-preserving.

## Method

- **Scoping.** `git log fa6be4a6b..HEAD -- <Paths>` per dimension, as in the table below.

  | Dim | Commits | Content |
  |---|---|---|
  | 1 | 0 | |
  | 2 | 1 | #6412 comment-only |
  | 3 | 1 | `f7194719f`, `--bench-capture-every` |
  | 4/5 | 6 | #6358, #6415, #6418, #6419, #6412 and a docs commit |
  | 6 | 1 | #6412 comment-only |
  | 7 | 7 | shader constants, soften mapping, `npc_spawn` split, CRFT |
  | 8 | 1 | `sdk/relationships.rs` data |

- **Process.** No sub-agents. No source was edited. No engine or GPU process was launched. Scratch notes are in `/tmp/audit/safety/`. Cargo used the repo `target/` with `CARGO_INCREMENTAL=0`; no private target dir.
- **Commands run at HEAD:**
  - `cargo test -p byroredux-renderer --lib`: **1503 passed, 0 failed, 1 ignored.** This includes the size and offset pins, the `shader_contract_tests`, the allocator-lock poison guards, the layout-walk guards, and the new `every_named_bracket_has_exactly_one_pass_row` and `every_timer_slot_and_active_bit_is_unique_and_in_range`.
  - `cargo test -p byroredux --bin byroredux -- rapier_release bone_palette_overflow allocator_teardown_order unload_walk greyscale bench_capture`: **34 passed, 0 failed.**
  - `cargo clippy -p byroredux-renderer --no-deps -- -D clippy::undocumented_unsafe_blocks`: **exit 0.**
  - `cargo tree -p byroredux-mod-runtime | grep -ic wasi`: **0.** `grep -c require_ capabilities.rs`: **22** (unchanged).
- **Dedup.** Checked against `/tmp/audit/open_issues_snapshot.txt` (#6506, #6507, #6493) and `SUITE_RULES.md` § 6, which was empty at finalization.

## Census (re-measured at 670f81917)

| Scope | Tokens | `unsafe {` blocks | `unsafe fn` | `unsafe impl` | Delta vs 10-27 |
|---|---|---|---|---|---|
| `crates/renderer/src` | **900** (948) | **669** (717) | **102** (102) | **44** (44) | −48 tokens / −48 blocks |
| `fsr3-sys` / `nif` / `core` / `pex` | 12 / 13 / 10 / 4 | 10 / 1 / 6 / 1 | | | unchanged |
| `byroredux`, `byro-launcher`, `plugin` | | 2, 1, 6 | | | unchanged |
| every other crate / tool | 0 | | | | No newcomer. A sweep of `unsafe {`, `unsafe fn`, `unsafe impl`, `unsafe trait` and `unsafe extern` across all crates finds only the known set: `core`, `cxx-bridge`, `fsr3-sys`, `nif`, `pex` and `plugin`. |

The renderer drop of 48 blocks is entirely #6358 (0288f468d). `gpu_timers.rs` went from 66 blocks to 18 by replacing 50 per-pass `cmd_<pass>_start/_end` methods with `cmd_start` / `cmd_end` over `TimerPass` table rows. No unsafe surface was removed or added beyond that; the 102 `unsafe fn` and 44 `unsafe impl` counts are flat.

## Dimension results (PASS / regression guards intact)

1. **Dim 1 — FFI.** Skimmed as unchanged. `fsr3-sys` still has 10 blocks and `cxx-bridge` is untouched. `crates/ui` has no `unsafe`.
2. **Dim 2 — UB.** Skimmed as unchanged.
   - `lz4_flex` is still `default-features = false` with an explicit feature list (`Cargo.toml:172`), and `crates/bsa` is the sole dependent via `workspace = true`.
   - `read_pod_vec_from`, `OpCode::from_u8` and `core/query.rs` have no diff.
   - The diff adds no `transmute`, `from_raw_parts`, `set_len`, `from_utf8_unchecked`, `CStr::from_ptr` or `mem::forget`.
3. **Dim 3 — leaks / drop order.**
   - **Open finding:** Existing #6506 (SAFE-2026-10-27-01) is not yet fixed, so every shutdown still takes the #665 leak path in release and panics in debug.
   - **`f7194719f`** (`--bench-capture-every`) is safe Rust. It claims the `ScreenshotBridge` with `try_claim`, takes the result, writes the PNG, and calls `cancel()` on timeout. The bridge handshake is leak-safe.
   - **`AllocatorResource` removal before the context drop** is still pinned (`app_events.rs:155`; the source-shape test `allocator_teardown_order` passes).
   - **Rapier release and greyscale-LUT collection tests** pass.
4. **Dim 4 — unsafe discipline.**
   - **Lint gate:** the `deny(clippy::undocumented_unsafe_blocks)` gate passes on the renderer.
   - **Truth check of the #6358 `gpu_timers` refactor.** A script compared every `PassRow` against the removed methods, field by field:
     - All 25 passes keep the same start and end slots, active bit, and start and end `PipelineStageFlags`.
     - Every `cmd_start` / `cmd_end` call site names the same pass at both ends of its bracket.
     - `cmd_start` / `cmd_end` keep the `// SAFETY` comments, and callers still go through `if let Some(ref mut timers) = self.gpu_timers`.
     - `QUERIES_PER_FRAME` = 60 and the geometry-phase slots end exactly at the pool size (compile-time assert).
     - Doc-table, slot-uniqueness and one-row-per-bracket tests guard the table.
   - **#6415 (`pack_camera_cloud_sun_transmittance`)** is safe: it clamps to `0..=1` and maps non-finite input to 1.0 before `to_bits`. Tests cover NaN, Inf and range.
5. **Dim 5 — Vulkan spec.**
   - **Barriers:** the delta adds no barrier, layout or descriptor change. The composite / water / `groundcover_blade` / `groundcover_models` shader edits (#6418, #6419) are named-constant substitutions, with source-scan tests against retyped literals.
   - **`.spv` rebuild:** only `composite.frag.spv` is rebuilt, and `composite.vert/frag` reflection tests pass.
   - **Not re-validated here:** the lavapipe lane stays red from #6506's teardown panic. No `[Vulkan]` validation lines were attributable to the delta.
6. **Dim 6 — GPU layout.** #6412 is comment-only. `GpuTerrainTile` is still 224 B, and the `GpuMaterial` (432 B), `GpuInstance` (160 B), `GpuCamera` (368 B) and `GpuLight` (64 B) pins pass. `sky_aperture_count[1]` now carries the f32 bits of the camera cloud transmittance, an existing `[u32; 4]` lane, so the struct size is unchanged and the shader reads it with `uintBitsToFloat`.
7. **Dim 7 — GPU-fed data.**
   - The `MAX_REFRACT_PASSTHRUS` bound and the shader-constants tests pass.
   - The new `WATER_*` constants are emitted through `shader_constants.glsl` and pinned.
   - Known open item: #6493, the moved-surface overflow warn, is unchanged. Per the REN dedup rule it is not a safety finding.
8. **Dim 8 — mod runtime.** WASI count is 0 and there are 22 `require_` guards. The only change since baseline is `sdk/relationships.rs` data (#6504). No new host functions or shared state.

## Next step

No new findings to publish. The standing safety work is the fix for #6506 (and its suggested source-scan guard) and #6507.
