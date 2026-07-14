# Renderer Audit — Dimensions 12 (Command Buffer Recording) + 14 (Caustic Splat)

- **Date**: 2026-07-14
- **Command**: `/audit-renderer 12 14` → `--focus 12,14 --depth deep`
- **Branch**: main
- **Method**: Orchestrator + 2 dimension agents (renderer-specialist), adversarial per-finding disproof, symbol-anchored verification against the live tree. Reference docs (`docs/engine/shader-pipeline.md`, `docs/engine/memory-budget.md`) treated as authoritative. Dedup baseline: `gh issue list` (30 open) + prior `docs/audits/` reports — notably the same-pair focused report `AUDIT_RENDERER_2026-06-28_DIM12_DIM14.md`. This pass specifically targets **drift since that audit**, because `draw.rs` saw a substantial refactor wave afterward.

> **Numbering note**: current scheme — Dim 12 = command buffer recording, Dim 14 = caustic splat. Older same-numbered `*_DIM12.md` / `*_DIM14.md` reports predate a dimension renumbering (old Dim 12 = GPU skinning, old Dim 14 = material table) and are unrelated.

---

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 0 | — |
| INFO | 0 | — |

**Both dimensions are zero-finding.** The 2026-06-28 audit closed these same two
dimensions with one LOW (D14-LOW-01, since **FIXED by #1780**) and two INFO items;
this re-audit confirms every previously-verified invariant still holds and finds
**no new drift** introduced by the post-audit refactor wave.

The material change since 2026-06-28 is the `draw.rs` refactor (#1748 extraction of
`record_geometry_pass` / `record_skinned_blas_refit`, plus #1796 pose-hash
rollback, #1811 clean-frame skinning skip, #1812 first-sight refit skip, #1804
z_write-gated blend split, #1713 queue-lock release). All of it preserved the
command-buffer recording invariants: no leaked command buffer, no render pass left
open on an error path, no missing/re-ordered barrier, no counter/telemetry drift.

`cargo test -p byroredux-renderer --lib`: 378 passed, 0 failed.

No release-blocking issues. No speculative Vulkan changes proposed. No
needs-RenderDoc items.

---

## RT Pipeline Assessment

**Command-buffer ordering (Dim 12)** — the recorded pass order matches
`shader-pipeline.md` "Per-Frame Submission Order" item-for-item, and the #1748
extraction did not perturb it: `skin_palette` → `record_skinned_blas_refit`
(skin_vertices + first-sight BUILD batch + refit loop, **all outside any render
pass**) → `build_tlas` (outside pass) → `cluster_cull` → `record_geometry_pass`
(the single `cmd_begin_render_pass`, begun only after `build_tlas`) →
`copy_depth_to_history` → `record_post_passes` (SVGF → caustic_splat →
volumetrics → TAA → SSAO → bloom → composite as last raster) → egui → screenshot
→ submit → present. AS builds are never recorded inside a render pass.

**Caustic accumulation path (Dim 14)** — correct end-to-end and unchanged in
behaviour since the last audit: per-FIF `R32_UINT` accumulator with
`STORAGE | SAMPLED | TRANSFER_DST`, held in `GENERAL` for the whole frame (the
only `UNDEFINED → GENERAL` transition is the one-time init/resize walk);
`imageAtomicAdd` fixed-point deposits with `CAUSTIC_FIXED_SCALE = 65536.0`
single-sourced from `shader_constants_data.rs` through the generated header to both
`caustic_splat.comp` and `composite.frag`; RT-gated on `sceneFlags.x` (shader
early-out **and** CPU dispatch-skip when no TLAS handle); source gate reads
`INSTANCE_FLAG_CAUSTIC_SOURCE` from `GpuInstance.flags` (named macro #1234, not a
hex literal); output added to **direct** lighting only — the SVGF-denoised indirect
never sees the accumulator (double-count guard holds).

## GPU-Struct & Memory Assessment

No layout or lifecycle findings in scope. Command-buffer recording leaks nothing
on the error/unwind paths: the #956/#992 `debug_assert!→log::error!+Once+clamp`
downgrade (to avoid leaking the in-flight cmd buffer on unwind) survived the #1748
extraction and sits outside any pass; the #910 acquire-signal recovery is intact
on all six fallible sites between `acquire_next_image` and `queue_submit`; the
#1713 graphics-queue `Mutex` is released before any one-time fence wait. The
caustic accumulator lifecycle (create / per-frame clear / barrier chain /
resize-recreate / destroy) is balanced, and the source-flag rides
`GpuInstance.flags` (not the material table), so no `GpuMaterial` interaction is in
play.

---

## Findings

**None.** Both dimensions are zero-finding at CRITICAL through INFO.

---

## Coverage Record — invariants verified to HOLD

### Dimension 12 — Command buffer recording (zero findings)

1. **Record lifecycle balanced; no fallible early-return inside the open pass.**
   `reset_command_buffer` → `begin_command_buffer` (ONE_TIME_SUBMIT) →
   `end_command_buffer`. An `awk` scan of the entire begin↔end window found the
   only two `return`s are the begin/end recovery arms themselves — no `?`, no
   `bail!`, no fn-level early-return recording work. The render pass is opened and
   closed entirely inside `record_geometry_pass` (single `cmd_begin_render_pass` ↔
   `cmd_end_render_pass`); between them, only closure-local `return`s (inside
   `dispatch_direct`) and pre-existing `debug_assert!`/`.expect()` invariant traps
   — no `?`, no `bail!`, no fn-level `return`. The #956/#992 downgrade uses
   `log::error!` via `Once` (not `debug_assert!`) and sits before
   `record_geometry_pass`.
2. **Pass ordering** matches `shader-pipeline.md` verbatim (see RT Pipeline
   Assessment). AS builds (`record_skinned_blas_refit`, `build_tlas`) run outside
   the render pass; the single main-pass begin fires only after `build_tlas`.
3. **#1796 pose-hash rollback covers all pre-dispatch bail paths.**
   `skin_dispatch_ran` is reset `false` at `draw_frame` top (before both early
   guards) and set `true` at the head of `record_skinned_blas_refit`. Consumer
   (`main.rs`) rolls back `rollback_pending_pose_commits` + requeues when
   `!skin_dispatch_ran` in the `Ok` arm; both `Ok`-returning early guards
   (`framebuffers.is_empty()`, `ERROR_OUT_OF_DATE_KHR`) return with it `false` so
   rollback fires. The pre-dispatch `Err` bail paths route to `event_loop.exit()`,
   so no subsequent frame reads the stale hash — coherent, not a finding.
   (`skin_dispatch_ran_ordering_tests`.)
4. **#1811 clean-frame skip / #1812 first-sight skip preserve barriers &
   correctness.** #1811 gates the bone-world upload + palette dispatch only after
   the pose has been clean ≥`MAX_FRAMES_IN_FLIGHT+1` frames; any dirty entity
   resets the streak → global palette recompute, so no stale-palette hazard, and
   the `COMPUTE→AS_BUILD` barriers are still emitted when work exists
   (`clean_skin_frames_tests`). #1812 marks `built_this_frame` only on the
   `Ok(())` BUILD arm, and the refit loop skips only those — BUILD-vs-UPDATE
   distinction correct, work-elimination only (`skin_built_this_frame_skip_tests`).
5. **Per-draw recording.** `None`/`u8::MAX` sentinels force first-batch emission of
   depth bias / cull mode / depth function; set 0 (bindless) + set 1 (scene) bound
   once/frame; pipeline rebind only on `pipeline_key` change; #1804 two-sided blend
   split gated on `z_write` (`needs_two_sided_blend_split`, unit-tested).
6. **Counter-independence guards.** #1258 (`batch_count` vs `indirect_call_count`
   vs input `draw_commands.len()` — three independent metrics); #1259 (`all_cached`
   short-circuit); #1260 (off-frustum draws zero `flags` but still push the
   `GpuInstance` SSBO slot for the #516 RT-hit contract); #1235
   (`SceneFlags::from_nif(cached.root_flags)` in `cell_loader/spawn.rs`).
7. **Adjacent safety.** #910 acquire-signal recovery on all six fallible sites;
   #952 `reset_fences` immediately before `queue_submit` (+ SIGNALED recreate on
   submit failure so the next `wait_for_fences` can't deadlock); #1713 queue
   `Mutex` released before one-time fence wait (`#911` removed per-first-sight host
   fence waits); `copy_depth_to_history` outside any pass with paired barriers;
   `resize.rs` records no command buffers; `memory_barrier` (`descriptors.rs`) is a
   real execution+memory dependency, not a no-op.

### Dimension 14 — Caustic splat (zero findings)

1. **Accumulator lifecycle.** Per-FIF `CAUSTIC_FORMAT = R32_UINT`, usage
   `STORAGE | SAMPLED | TRANSFER_DST`; moving-camera `cmd_clear_color_image`
   bracketed by GENERAL→GENERAL (COMPUTE|FRAGMENT→TRANSFER pre, TRANSFER→COMPUTE
   post); HOST→COMPUTE UBO barrier before dispatch; COMPUTE→FRAGMENT before
   composite sample; layout stays `GENERAL` all frame (only init/resize does the
   one-time UNDEFINED→GENERAL).
2. **Atomic accumulation.** `imageAtomicAdd` on u32 fixed-point (no float atomic);
   `CAUSTIC_FIXED_SCALE = 65536.0` single-sourced from `shader_constants_data.rs`
   through the generated `#define` to both the CPU upload (`caustic.rs` `tune[0]`)
   and the composite divide — no hand-written literal.
3. **Source-pixel selection.** Gate reads `instances[instIdx].flags &
   INSTANCE_FLAG_CAUSTIC_SOURCE` (named macro #1234, not a hex literal, not
   `materials[material_id]` — the material path is the deferred #1098 comment).
   CPU sets the bit in `is_caustic_source`, pinned by `is_caustic_source_tests`
   (8 cases) and `instance_flag_bits_match_scene_buffer_consts`.
4. **Output / double-count guard.** Caustic added to **direct** only
   (`combined = direct + indirect * albedo + caustic`); the SVGF-denoised indirect
   is a separate term and `svgf_temporal.comp` never reads the accumulator. Sampled
   via `usampler2D`, divided by scale, then `CAUSTIC_FIREFLY_MAX = 16.0` cap applied
   **after** the divide.
5. **RT gating.** Shader `if (sceneFlags.x < 0.5) return;` + CPU dispatch-skip
   unless `accel_manager.tlas_handle(frame)` is `Some`; TLAS rebound per frame
   before dispatch.
6. **Dispatch coverage.** `width/height.div_ceil(8)` matches
   `WORKGROUP_X/Y = 8`; in-shader bounds + per-tap guards; per-FIF descriptor +
   slot image → no cross-frame WAR.
7. **Boundary comment ↔ live water path.** `caustic_splat.comp`'s "water-side
   caustic is the water shader's responsibility" matches the live `water.frag`
   atomic-add path (`refract` → floor trace → `imageAtomicAdd(waterCausticAccum,…)`
   with the same `CAUSTIC_FIXED_SCALE` scale + `0xFFFFFFFF/scale` clamp), not a stub.

**Regression guards re-verified (not re-reported as findings):**
- **D14-LOW-01 (FIXED #1780)** — both `caustic_splat.comp` and `water.frag` are
  now in `affected_shaders_include_constants_header`.
- **D14-INFO-01 (resolved #1575)** — `composite.frag` promotes each accumulator to
  `float` **before** summing (`float(causticRaw) + float(waterCausticRaw)`), not a
  u32 `float(a+b)` sum; no re-collapse.
- **#1934 (CAUSTIC-D14-01)** — `caustic_splat_comp_uses_named_instance_flag_constant`
  present, guards the #1234 named-macro fix against a `flags & 4u` revert.
- **#1935 (033510af)** — the two rewritten water-caustic consumer comments now
  match the live `water.frag` / `draw.rs` code (no code/comment divergence).

---

## Prioritized Fix Order

Nothing to fix. Both dimensions are clean; the prior LOW item (D14-LOW-01) was
already closed by #1780.

## Needs-RenderDoc

None. No sync/barrier finding in either dimension required capture-based
verification; all barrier chains were read for consistency only and found
consistent.
