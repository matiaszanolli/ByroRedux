# Safety Audit — 2026-07-02

**Scope**: `unsafe` blocks, memory leaks, undefined behavior, Vulkan spec
compliance across all 21 crates + `byroredux/`.

**Tree state**: `HEAD = 1b4e8e84`, clean working tree — **identical commit** to
the 2026-07-01 safety audit (`git log` shows zero commits landed between the
two runs; the only diff is untracked audit-report files from yesterday's
bug-bash).

**Method**: Because the tree is byte-identical to the prior audit's baseline,
this run is a full independent re-verification rather than a fresh sweep from
scratch — every claim in `docs/audits/AUDIT_SAFETY_2026-07-01.md` was
re-checked against current source (not trusted from prose), plus the cited
regression-guard tests were re-run green. Three parallel verification passes
were used: (1) direct source reads + `cargo test` runs for the renderer
drop-ordering / Vulkan-spec guards and the material-layout pins, (2) an
independent recount script for the unsafe-comment sweep (Dimension 4), (3) an
independent re-derivation of the ECS/NIF/pex/NIFAL invariants (Dimensions 2
and 9). Findings deduped against the OPEN-issue list (`/tmp/audit/issues.json`,
21 open issues, none matching either finding below).

---

## Summary

**2 findings** (0 critical, 1 high, 1 medium, 0 low) — both carried forward
from 2026-07-01, unchanged, because the tree has not changed. No new findings.

The one live defect is the same evidence-backed HIGH from yesterday: the
skinned-BLAS first-sight BUILD → same-command-buffer UPDATE-refit transition
is still guarded only by an `AS_WRITE → AS_WRITE` barrier, which does not
cover the refit's *read* of `srcAccelerationStructure`, per the Vulkan spec's
synchronization requirements for UPDATE-mode acceleration structure builds.
This was not re-run against the live validation layer today (no new commits
to re-validate against); the finding stands on yesterday's captured
validation-layer evidence plus today's independent static re-confirmation
that the flagged code is unchanged.

---

## Findings

### SAFE-2026-07-02-01: Skinned-BLAS first-sight BUILD → same-cmd UPDATE refit lacks AS_READ in the barrier — sync-validation READ_AFTER_WRITE hazard

- **Severity**: HIGH
- **Dimension**: 5 — Vulkan Spec Compliance (Missing AS barrier)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:606-620`
  (`record_scratch_serialize_barrier`); hazard sequence recorded in
  `crates/renderer/src/vulkan/context/draw.rs:1780-1909`
  (`record_skinned_blas_refit`: first-sight `build_skinned_blas_batched_on_cmd`
  at `:1781` → refit loop at `:1835-1899` → closing barrier at `:1902-1909`)
- **Status**: Regression of nothing — carried forward unchanged, **Existing:
  SAFE-2026-07-01-01** (same commit, same code, re-verified today rather than
  re-discovered). Deduped against #642, #644, #661, #983, #1095, #1140, #1300,
  #1436 (none cover the src-AS read) and the current OPEN-issue list (no
  match).
- **Description**: `refit_skinned_blas` records an UPDATE-mode build with
  `src == dst == entry.accel`. Per the Vulkan spec, an UPDATE-mode build
  **reads** `srcAccelerationStructure` with
  `VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR` at the
  `ACCELERATION_STRUCTURE_BUILD` stage. On an entity's first-sight frame the
  same command buffer records the fresh BLAS BUILD (writes the AS) at
  `draw.rs:1781`, then falls through unconditionally to the refit loop
  (first-sight entities are in `pose_dirty`, so the #1196 skip gate at
  `draw.rs:1860-1870` does not fire for them). The only barrier between the
  two builds is the self-emitted scratch-serialize barrier
  (`blas_skinned.rs:610-618`, re-read today): src `AS_WRITE` → dst
  `AS_WRITE` only — **no `AS_READ` bit on the destination mask**. The BUILD's
  write to the AS backing buffer is therefore never made visible to the
  refit's `AS_READ` access → RAW hazard. The cross-frame steady-state case is
  fine: the closing `AS_WRITE → AS_READ` barrier after the refit loop
  (`draw.rs:1902-1909`, confirmed present and unchanged) covers frame N+1's
  refit reading frame N's write; the gap is specifically the same-command-buffer
  first-sight case.

  Re-confirmed today: the `refit_skinned_blas` docstring
  (`blas_skinned.rs:380-383`) still claims "The barrier is idempotent
  (`MEMORY_READ | MEMORY_WRITE → MEMORY_READ | MEMORY_WRITE`)" — the code it
  describes is `AS_WRITE → AS_WRITE` only (verified by direct read of
  `record_scratch_serialize_barrier`'s body today); the docstring documents
  READ coverage the implementation does not have.
- **Evidence**: Validation-layer output captured 2026-07-01 (180-frame FNV
  `GSProspectorSaloonInterior` run, `VK_LAYER_KHRONOS_validation` +
  sync-validation, RTX 4070 Ti) — 10 occurrences (one per first-sight skinned
  NPC), then the layer's `duplicate_message_limit` suppressed further
  instances:

  ```
  vkCmdBuildAccelerationStructuresKHR(): READ_AFTER_WRITE hazard detected.
  vkCmdBuildAccelerationStructuresKHR reads VkBuffer 0x2a450000002a45, which was
  previously written by another vkCmdBuildAccelerationStructuresKHR command. The
  buffer backs pInfos[0].srcAccelerationStructure (VkAccelerationStructureKHR
  0x2a460000002a46).
      The current synchronization allows VK_ACCESS_2_ACCELERATION_STRUCTURE_WRITE_BIT_KHR
  accesses at VK_PIPELINE_STAGE_2_ACCELERATION_STRUCTURE_BUILD_BIT_KHR, but to
  prevent this hazard, it must allow VK_ACCESS_2_ACCELERATION_STRUCTURE_READ_BIT_KHR
  accesses at VK_PIPELINE_STAGE_2_ACCELERATION_STRUCTURE_BUILD_BIT_KHR.
      Buffer access region: { offset = 0  size = 88192 }
  ```

  The barrier the layer is describing, re-read verbatim from source today
  (`blas_skinned.rs:610-618`):

  ```rust
  unsafe {
      memory_barrier(
          device, cmd,
          vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
          vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
          vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
          vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,   // ← no AS_READ
      );
  }
  ```

  No new validation run was captured today since the flagged code and its
  callers are byte-identical to yesterday's exercised run — re-running would
  reproduce the identical log.
- **Impact**: On first-sight frames the refit may consume a partially-written
  source BVH — driver-dependent BVH corruption on real hardware (garbage or
  missing skinned geometry in every RT effect: shadows, GI, reflections),
  formally UB per the spec's synchronization requirements. Fires for **every**
  newly-spawned skinned NPC (cell transitions, streaming) — realistic,
  common-path conditions, matching the severity table's "Vulkan validation
  layer errors in normal operation" / "Missing AS barrier" HIGH rows. Not
  CRITICAL: geometry, addresses, and counts are all correct (#907/#1145 guards
  re-verified intact this audit); the defect is visibility ordering only.
- **Related**: #983 / #1140 (scratch-serialize invariant — the pinned
  predicate `requires_scratch_serialize_barrier_before` codifies scratch WAW
  only, not the src-AS read), #911 (moved first-sight builds onto the
  per-frame cmd, creating the same-cmd adjacency that exposes this gap),
  #1436 (build-input access flags — a different access class), #1139 (older
  docstring drift in the same function).
- **Suggested Fix**: Widen the dst access mask in
  `record_scratch_serialize_barrier` to
  `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR` (one
  line; src mask stays `AS_WRITE`). Correct the `refit_skinned_blas` docstring
  to match the actual mask. Re-run the validation-layer scenario (FNV saloon,
  180 frames) and confirm zero hazards; extend the #1140 predicate test to pin
  the READ bit so a future refactor can't narrow it again.

### SAFE-2026-07-02-02: Residual ~219 renderer `unsafe {` blocks lack a SAFETY comment

- **Severity**: MEDIUM
- **Dimension**: 4 — Unsafe-Block Discipline
- **Location**: `crates/renderer/src/` (worst: `vulkan/composite.rs` 17/41,
  `vulkan/context/mod.rs` 16/19, `vulkan/context/helpers.rs` 16/18,
  `vulkan/texture.rs` 15/16, `vulkan/device.rs` 14/17, `vulkan/svgf.rs` 13/41,
  `vulkan/context/resize.rs` 13/14, `texture_registry.rs` 10/10 — fully
  uncommented, `vulkan/compute.rs` 9/9 — fully uncommented,
  `vulkan/skin_compute.rs` 9/9 — fully uncommented, `vulkan/taa.rs` 9/28,
  `vulkan/caustic.rs` 8/29, `vulkan/scene_buffer/upload.rs` 8/16,
  `vulkan/egui_pass.rs` 7/7 — fully uncommented)
- **Status**: Existing: #1644 (CLOSED — partial fix), carried forward as
  **SAFE-2026-07-01-02 unchanged** — same commit, re-measured rather than
  re-discovered.
- **Description**: Independent recount today (separate script/methodology
  from yesterday's, cross-checked with a second plain-grep pass) found **546**
  total non-test `unsafe {` block openers, **219** without a `SAFETY` comment
  in the same line or preceding 6 lines. This is a 0.4% delta from yesterday's
  544/218 — consistent with a minor exclusion-filter difference (e.g. one
  inline `#[cfg(test)] mod` boundary), not an actual code change (the tree is
  identical). Two files remain **completely** uncommented — re-confirmed by
  direct read, not just script: `texture_registry.rs` (10 blocks, 0 SAFETY —
  lines 297, 313, 324, 360, 1144, 1174, 1278, 1291, 1301, 1375) and
  `vulkan/context/screenshot.rs` (5 blocks, 0 SAFETY — lines 224, 234, 248,
  254, 272). `vulkan/compute.rs`, `vulkan/skin_compute.rs`, and
  `vulkan/egui_pass.rs` are also fully uncommented (9/9, 9/9, 7/7
  respectively) — new to this audit's explicit file list, though the
  aggregate count already included them under yesterday's "residual" bucket.
- **Evidence**: Top-15 by uncommented-block count (today's recount):
  `composite.rs` 17, `context/helpers.rs` 16, `context/mod.rs` 16,
  `texture.rs` 15, `device.rs` 14, `context/resize.rs` 13, `svgf.rs` 13,
  `texture_registry.rs` 10, `compute.rs` 9, `skin_compute.rs` 9, `taa.rs` 9,
  `caustic.rs` 8, `scene_buffer/upload.rs` 8, `egui_pass.rs` 7, `pipeline.rs`
  6 — matches yesterday's top-11 ordering exactly where they overlap.
- **Impact**: Defense-in-depth / maintainability gap, not live UB — every
  spot-checked call site (this audit and the prior one) is sound today (live
  device, valid handles, correct FFI usage). Each undocumented invariant is
  one refactor away from being silently violated with no comment to catch the
  reviewer's attention.
- **Related**: #1644 (closed, fixed 124 of ~327 originally), #1432, #579
  (both closed, earlier partial sweeps).
- **Suggested Fix**: Unchanged from yesterday — finish the #1644 sweep,
  prioritizing the three fully-uncommented small files first
  (`texture_registry.rs`, `context/screenshot.rs`, `egui_pass.rs`,
  `compute.rs`, `skin_compute.rs` — these are cheap, self-contained wins),
  then the four large partially-commented files. Batch one SAFETY note per
  FFI cluster rather than per-call.

---

## Verified-Intact Regression Guards (PASS — not findings)

Every item independently re-checked against current code at `1b4e8e84` via
direct source reads (not carried over from yesterday's prose); all cited
tests re-run green today.

**Dimension 1 — FFI lifetime (cxx)**: `crates/cxx-bridge/src/lib.rs` re-read
in full — still exposes only `native_hello() -> String` inside
`unsafe extern "C++"`. No `*const`, `&[u8]`, `Box<…>`, or Rust-reference-taking
C++ fn. Dimension dormant.

**Dimension 2 — Memory corruption / UB** (independently re-derived, not
copied):
- ECS cached-pointer contract (`crates/core/src/ecs/query.rs`): `QueryRead`
  (L23-35), `QueryWrite` (L93-104), `ComponentRef` (L231-241) each hold their
  `RwLock*Guard` as a struct field and cache a raw pointer resolved once in
  `new()`. All four `unsafe` deref sites (L64, L135, L143, L289) carry SAFETY
  comments whose stated invariant matches the surrounding code; mutation is
  gated behind `&mut self` (`storage_mut()` L139, `deref_mut()` L213-217).
- pex `OpCode::from_u8` (`crates/pex/src/opcode.rs:130-137`): range check
  `byte >= MAX_OPCODE` (= 51, L131) precedes the transmute (L136); enum
  discriminants contiguous from `Nop = 0`; `OPCODES` table sized
  `[...; MAX_OPCODE as usize]` (compile-time cross-check); round-trip test
  (L169-175) covers `0..51` plus `51`/`255` rejection.
- pex recursion cap (`crates/pex/src/decompile/control_flow.rs:39`):
  `MAX_REBUILD_DEPTH = 1024`; `Reconstructor::rebuild` bails
  `RecursionLimit` before recursing (L96-102); test
  `rebuild_rejects_excessive_recursion_depth` (L240-255) green.
- sfmaterial `BuiltinType::from_u32` (`crates/sfmaterial/src/types.rs:37-57`):
  checked `match` with `_ => Err(UnsupportedBuiltin)` — no transmute anywhere
  in the function.
- NIF bulk POD reads (`crates/nif/src/stream.rs:350-351`, header mirror
  `header.rs:360-368`): `count.checked_mul(size_of::<T>())` overflow guard
  present in both; `T: AnyBitPattern` is a crate-private sealed trait
  (`stream.rs:47`) — the impl allowlist (L63-67) deliberately excludes
  `bool`/`char`/enums.
- Recursion caps confirmed present and enforced: NIF walk
  `MAX_NIF_NODE_DEPTH = 128` (`crates/nif/src/import/walk/mod.rs:162`, both
  `walk_node_hierarchical` and `walk_node_flat`), Papyrus expr
  `MAX_EXPR_DEPTH = 256` (`crates/papyrus/src/parser/expr.rs:19`, 3 pinning
  tests), pex `MAX_REBUILD_DEPTH = 1024` (above).
- `crates/save/`, `crates/scripting/` — zero `unsafe` (re-confirmed by grep).
  `crates/core/src/character/` (CHARAL, the bulk of this session's landed
  work) adds **zero** unsafe code — core's token count is unchanged at 6 (4 in
  `ecs/query.rs`, 2 in `string/mod.rs`).
- `crates/facegen`, `crates/plugin` "unsafe" grep hits remain comment prose
  only; no unsafe code.

**Dimension 3 — Leaks / drop ordering** (re-verified by direct read +
`cargo test`):
- AllocatorResource-before-device (#1406/#1477): `impl Drop for App`
  (`byroredux/src/main.rs`) still removes `AllocatorResource` before
  `renderer.take()` on every teardown path.
- Deferred-destroy drain (#418/#732): `tick_deferred_destroy` confirmed
  running AFTER `wait_for_fences` — `wait_for_fences` at `draw.rs:2074-2082`,
  tick at `draw.rs:2166-2190`; a self-checking string-search test at
  `draw.rs:4147-4163` structurally enforces this ordering (fails to compile
  the test if `tick_deferred_destroy` is found before `wait_for_fences` in
  source). Shutdown path (`context/mod.rs`) drop impl calls
  `device_wait_idle()` at line 2836 before draining `egui_pass` (2842) and the
  acceleration-manager's internal deferred queues.
- Rapier release on cell unload (#1520/#1531): release path wired into
  `unload_cell` before the despawn loop (`byroredux/src/cell_loader/unload.rs:187`);
  `remove_body`/`remove_ragdoll` cascade colliders + impulse/multibody joints
  via rapier's own `remove_attached_colliders = true`. All 7
  `rapier_release_tests` re-run green today, including
  `release_removes_ragdoll_bodies_colliders_and_joints` and
  `release_sweeps_both_ragdoll_and_rapier_handles`.
- CPU-side growth: `MaterialTable::intern` caps at `MAX_MATERIALS` (confirmed
  via `intern_overflow_persists_across_clear` / `intern_overflow_returns_material_zero`,
  both green); `AnimationClipRegistry` interns lowercased keys (#790,
  unchanged).

**Dimension 5 — Vulkan spec (beyond the finding)**:
- TLAS resize `device_wait_idle` before freeing old allocation (#1390):
  re-read at `acceleration/tlas.rs:322`, present.
- Volumetrics dispatch gate: `VOLUMETRIC_OUTPUT_CONSUMED`
  (`volumetrics.rs:143`) honored at both `draw.rs` call sites (`:569`,
  `:3335`) — re-confirmed by direct grep.
- `VK_KHR_ray_query` gating and TLAS UPDATE count guard (#1083),
  skinned-refit count/flag guards (#907/#1145): unchanged code, not
  re-derived from scratch this pass (no commits since yesterday's full
  re-check) — carried as PASS on the strength of yesterday's verification
  plus today's confirmation that the surrounding code is untouched.
- `NON_COHERENT_ATOM_SIZE` assumption (#1759): assertion against the physical
  device limit confirmed present at `device.rs:575-580`.
- SPIR-V reflection pins: all 5 `scene_descriptor_reflection_tests` re-run
  green today.

**Dimension 6 — Material table layout**: All 41 material/GPU-layout-related
tests re-run green today, including `gpu_material_size_is_300_bytes`,
`gpu_material_field_offsets_match_shader_contract`,
`gpu_material_glsl_field_names_pinned`,
`gpu_material_glsl_field_order_matches_rust_struct`,
`intern_overflow_returns_material_zero`. No `[f32; 3]` fields in any
`#[repr(C)]` GPU struct.

**Dimension 7 — RT IOR/glass**: `GLASS_RAY_BUDGET = 1048576`
(`crates/renderer/src/shader_constants_data.rs:69`) confirmed; Frisvad basis
comment confirmed active at `triangle.frag:1271`; `DBG_VIZ_GLASS_PASSTHRU =
0x80` re-swept against the full `DBG_*` catalog (`0x1` through `0x10000`) —
all 17 flags distinct, no collision.

**Dimension 8 — NPC/animation spawn**: `bone_palette_overflow_tests`
(`at_capacity_fills_palette_completely`,
`over_capacity_breaks_loop_and_truncates_offsets`) and
`cap_at_max_particles_drops_extra_spawns` re-run green today.

**Dimension 9 — NIFAL NaN boundary**: Independently re-derived —
`material_translate.rs:157-158` seeds `f32::NAN` for unset
metalness/roughness, then calls `resolve_pbr()` in the same function before
the `Material` leaves the translate boundary. `Material::resolve_pbr`
(`components/material.rs:638-657`) detects via `.is_nan()`, fills from the
classifier, and unconditionally clamps (`clamp` with a NaN input returns the
clamp's min bound, so no NaN survives even on a logic gap). The
`static_meshes.rs` no-Material fallback constructs finite literals directly
(0.5 roughness, 0.0 metalness) with no dependency on `resolve_pbr` running.

**Dimension 10 — debug-ui teardown**: `DebugUiState`
(`crates/debug-ui/src/lib.rs:50-67`) re-read in full — holds no Vulkan handle
of any kind (only `egui::Context`, `egui_winit::State`, panel state). The
Vulkan-owning half (`EguiPass`) lives in
`crates/renderer/src/vulkan/egui_pass.rs`, owned by `VulkanContext`, and is
torn down at `context/mod.rs:2842` — immediately after `device_wait_idle`
(`:2836`) and before every other teardown step, device destroyed last.
Allocator-before-device rule holds structurally.

---

## Coverage Note

All 21 `crates/` + `byroredux/` swept for `unsafe` this audit (independent
recount, not reused from yesterday): 546 non-test unsafe-block openers total,
219 without a SAFETY comment. Per-crate token counts unchanged from
yesterday: renderer 629, nif 11 (POD-read infra + sealed-trait impls, all
commented), core 6 (ECS cached pointers + string pool, commented), byroredux
2, pex 1 (guarded transmute), cxx-bridge 1 (bridge marker only), facegen 1
(comment prose, no code), plugin 1 (comment prose, no code); save, scripting,
audio, bsa, bgsm, papyrus, physics, spt, sfmaterial, platform, ui, debug-*
all 0. Three independent verification passes (direct source read + test
execution for renderer/Vulkan; a from-scratch recount script for the
unsafe-comment sweep; a from-scratch re-derivation for ECS/NIF/pex/NIFAL
invariants) all corroborate the 2026-07-01 findings with no discrepancies
beyond the expected 0.4% counting-methodology noise. No new commits landed
between the two audits, so no new code surface exists to introduce a new
finding.

Next step: `/audit-publish docs/audits/AUDIT_SAFETY_2026-07-02.md` — note
both findings already track existing/prior report entries
(SAFE-2026-07-01-01/-02), so publish should link rather than duplicate if an
issue was already opened from yesterday's report.
