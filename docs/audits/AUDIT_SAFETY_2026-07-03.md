# Safety Audit — 2026-07-03

**Scope**: `unsafe` blocks, memory leaks, undefined behavior, Vulkan spec
compliance across all 21 crates + `byroredux/`.

**Tree state**: `HEAD = 8498e559`. 32 commits landed since the last safety
audit (`AUDIT_SAFETY_2026-07-02.md` at `1b4e8e84`), most from a bug-bash spree
that fixed the two live findings from that report:

- `d688fe06` — Fix #1790 (= yesterday's SAFE-2026-07-02-01, HIGH): added the
  missing `AS_READ` bit to the skinned-BLAS scratch-serialize barrier.
- `6245106c` — Fix #1782: deferred `blas_scratch_buffer` destruction (a
  related, previously-undiscovered GPU use-after-free window in the same
  subsystem, caught by a different audit pass and fixed before today).

Both are re-verified below as PASS, not carried forward as findings.

**Method**: Independent sweep, not a copy of the 07-02 report's carried-over
prose. Steps: (1) recount `unsafe` tokens per crate and the SAFETY-comment gap
from scratch with a fresh script; (2) diff `1b4e8e84..8498e559` to scope which
of the 32 new commits touch safety-relevant surface (renderer accel/, pex
decompiler, ECS resources, NIF import) and read each one; (3) re-run the
regression-guard test suites (acceleration, material layout, bone-palette
overflow, Rapier release) plus a full `cargo test --workspace`; (4) re-derive
the cxx-bridge / ECS-cached-pointer / NIF-POD-read / sfmaterial-decode /
pex-opcode-transmute invariants directly from source rather than trusting
yesterday's prose; (5) dedup every candidate against
`gh issue list --repo matiaszanolli/ByroRedux --limit 200` (400 issues, 70
open) and `docs/audits/`.

---

## Summary

**0 new findings.** The tree is clean of the only two live issues carried in
yesterday's report — both fixed by name (#1790, and #1782 which yesterday's
report hadn't yet surfaced). The residual unsafe-comment gap persists
(expected — see below) but is not a new finding; it is the same rolling
MEDIUM tracked across the last several safety audits, partially addressed by
#1644, and unresolved (no full-sweep issue currently open for the remainder).
`cargo test --workspace` is fully green (0 failures).

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 0 |
| **Total** | **1** |

---

## Findings

### SAFE-2026-07-03-01: Residual ~222 renderer `unsafe {` blocks lack a SAFETY comment

- **Severity**: MEDIUM
- **Dimension**: 4 — Unsafe-Block Discipline
- **Location**: `crates/renderer/src/` (worst by absolute count, fresh
  recount today: `vulkan/composite.rs` 17, `vulkan/context/mod.rs` 16,
  `vulkan/context/helpers.rs` 16, `vulkan/texture.rs` 15, `vulkan/device.rs`
  14, `vulkan/svgf.rs` 13, `vulkan/context/resize.rs` 13,
  `texture_registry.rs` 10, `vulkan/skin_compute.rs` 9, `vulkan/taa.rs` 9,
  `vulkan/compute.rs` 9, `vulkan/caustic.rs` 8, `vulkan/scene_buffer/upload.rs`
  8, `vulkan/egui_pass.rs` 7)
- **Status**: Existing — same rolling finding as
  `SAFE-2026-07-01-02`/`SAFE-2026-07-02-02`, partial prior fix **#1644**
  (closed). No open issue currently tracks the remainder as a single item; not
  reported as NEW since it is unchanged in substance and location from the
  last two audits.
- **Description**: Independent recount (own script, not reused from prior
  audits) over all of `crates/`: **545** non-test `unsafe {` block openers,
  **222** without a `SAFETY` comment on the same line or in the preceding 6
  lines. This is within the same ~0.5% noise band as 07-01→07-02's
  546/219 → 545/222 delta (counting-methodology variance, e.g. how a
  `#[cfg(test)] mod` boundary or a doc-comment mentioning "SAFETY" in prose is
  classified) — not a real regression, since none of the 32 commits since
  07-02 touch any of the files in the top-14 list above. Per
  `_audit-severity.md`'s Special Rules table, an unsafe block without a
  SAFETY comment is MEDIUM regardless of whether the underlying invariant
  actually holds (spot-checked several of the listed files' blocks today —
  all sound, e.g. `texture_registry.rs`'s raw pointer casts on mapped memory
  match the surrounding lock/lifetime guarantees; the gap is documentation,
  not correctness).
- **Evidence**: same top-file ordering as the 07-02 report, re-derived
  independently today rather than copied.
- **Impact**: Defense-in-depth / maintainability gap, not live UB — every
  spot-checked call site today is sound (live device, valid handles, correct
  FFI usage, matching guard scope). Each undocumented invariant is one
  refactor away from being silently violated with nothing to flag the
  reviewer's attention.
- **Related**: #1644 (closed, fixed 124 of ~327 originally), #1432, #579.
- **Suggested Fix**: Unchanged from the last two audits — resume the #1644
  sweep starting with the small fully-uncommented files
  (`texture_registry.rs`, `context/screenshot.rs`, `egui_pass.rs`,
  `compute.rs`, `skin_compute.rs`), then the four large partially-commented
  files (`composite.rs`, `context/mod.rs`, `context/helpers.rs`,
  `texture.rs`). Batch one SAFETY note per FFI cluster rather than per call
  site.

---

## Fixes Verified Since 07-02 (re-confirmed sound, not findings)

- **#1790 / SAFE-2026-07-02-01 (was HIGH)** — `record_scratch_serialize_barrier`
  (`crates/renderer/src/vulkan/acceleration/blas_skinned.rs:610-634`) now
  widens the dst access mask to
  `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR`,
  closing the same-command-buffer BUILD→UPDATE-refit RAW hazard flagged
  yesterday. New pinning test
  `scratch_serialize_barrier_dst_mask_includes_as_read` passes. Docstring
  corrected to no longer claim WRITE-only was "idempotent" READ coverage.
  Re-read the call sequence in `context/draw.rs` around
  `record_skinned_blas_refit` — the barrier still sits exactly between the
  first-sight BUILD and the refit loop, and the closing
  `AS_WRITE → AS_READ` barrier after the refit loop is unchanged. **Confirmed
  fixed, not a regression risk today.**
- **#1782 (new since 07-02, already fixed)** — `blas_scratch_buffer` grow/shrink
  paths in `build_blas` / `build_blas_batched` / `shrink_blas_scratch_to_fit`
  previously destroyed the retired scratch buffer immediately, racing an
  in-flight frame's skinned-BLAS refit that captured the old buffer's device
  address at command-buffer record time — a GPU use-after-free window the
  same class as #1449 but never extended to the scratch buffer. Now routed
  through `pending_destroy_scratch: DeferredDestroyQueue<GpuBuffer>`
  (`acceleration/mod.rs:175`), ticked in `tick_deferred_destroy`
  (`blas_static.rs:90`, called post-fence-wait from `draw.rs:2217`) and
  drained in `drain_pending_destroys` (`blas_static.rs:134`, under the same
  `# Safety: caller's preceding device_wait_idle` contract as the sibling
  `pending_destroy_blas` queue). Verified both call sites are wired (grepped
  `pending_destroy_scratch` across the tree — only `mod.rs`/`memory.rs`
  (fields + pushes), `blas_static.rs` (tick/drain/count), and a
  `unload.rs` doc-comment reference exist; no destroy call outside the
  deferred queue remains). 71 acceleration-module tests green.
- **#1792** — `evict_unused_blas`'s budget gate now takes a `pending_bytes`
  parameter and a new `blas_over_budget(static, pending, budget)` predicate so
  mid-batch eviction actually sees the in-progress batch's uncommitted result
  buffers, closing a structural no-op in the #1698-adjacent BLAS eviction
  path. Unit tests for the threshold math pass.
- **#1791** — Read the full `SkinSlotPool::requeue_pending` /
  `rollback_pending_pose_commits` wiring in `byroredux/src/main.rs:1759-1867`
  and `crates/core/src/ecs/resources.rs`. The `pending_for_requeue` mirror
  list is built in lockstep with the actual upload payload (an entry whose
  `SkinnedMesh` is already gone is correctly excluded from both), and the
  `!ctx.skin_dispatch_ran` gate correctly covers both of `draw_frame`'s
  early-return paths (empty framebuffers, `ERROR_OUT_OF_DATE_KHR`). The `Err`
  arm calls `event_loop.exit()` — no requeue needed since the engine is
  tearing down. Three dedicated unit tests
  (`requeue_pending_restores_entries_for_the_next_drain`,
  `_is_a_no_op_for_an_empty_list`, `_entries_drain_before_newly_queued_ones`)
  pass. **Sound.**
- **#1815 / #1816** — `pex`'s `BoolPass::rebuild` now threads a depth counter
  capped at the same `MAX_REBUILD_DEPTH = 1024` as its `control_flow::Reconstructor`
  sibling (stack-overflow guard, Dimension 2); `translate_pex` now wraps
  `decompile_script` in `catch_unwind(AssertUnwindSafe(...))` so a hostile/corrupt
  `.pex` degrades to `None` instead of aborting cell load. Both closed the
  gap between the corpus-smoke harness's existing panic-catching and the live
  cell-load attach path. No live repro exists in the current 26,640-file
  corpus; these are pre-emptive hardening, consistent with the commit
  messages' own framing.
- **#1828 / #1829** — Starfield `BSGeometry` sentinel-slot (`scale<=0`,
  empty vertices/triangles) iteration fix in
  `crates/nif/src/import/mesh/bs_geometry.rs` — correctness only, zero
  `unsafe` in the file, not a safety-dimension change but reviewed to confirm
  no new UB/leak surface (none).

---

## Verified-Intact Regression Guards (PASS — not findings)

All independently re-derived from source today (not carried from prior
prose), plus a full `cargo test --workspace` run (0 failures):

**Dimension 1 — FFI lifetime (cxx)**: `crates/cxx-bridge/src/lib.rs` re-read
in full — unchanged, still exposes only `native_hello() -> String` inside
`unsafe extern "C++"`. No raw pointer, slice, or `Box<…>` crosses the
boundary. Dimension dormant.

**Dimension 2 — Memory corruption / UB**:
- ECS cached-pointer contract (`crates/core/src/ecs/query.rs`) — unchanged
  since 07-02; core's unsafe token count is still 6, all in `ecs/query.rs`
  (4) and `string/mod.rs` (2).
- pex `OpCode::from_u8` (`crates/pex/src/opcode.rs`) — unchanged transmute +
  range-check pairing; `pex`'s only production `unsafe` token.
- pex recursion caps — now **two** capped recursive passes
  (`control_flow::Reconstructor::rebuild` and, new since 07-02,
  `decompile/boolean.rs::BoolPass::rebuild`), both at `MAX_REBUILD_DEPTH = 1024`.
- sfmaterial `BuiltinType::from_u32` — unchanged checked `match` + `Err` arm,
  zero unsafe in the crate.
- NIF bulk POD reads (`stream.rs`, `header.rs`) — unchanged overflow guard +
  sealed `AnyBitPattern` bound; nif's unsafe token count unchanged at 11.
- Recursion caps: NIF walk `MAX_NIF_NODE_DEPTH = 128`, Papyrus
  `MAX_EXPR_DEPTH = 256`, pex `MAX_REBUILD_DEPTH = 1024` (now covering two
  call sites) — all confirmed present.
- `crates/save/`, `crates/scripting/`, `crates/physics/`, `crates/bsa/`,
  `crates/bgsm/`, `crates/audio/`, `crates/spt/`, `crates/papyrus/`,
  `crates/ui/`, `crates/platform/`, all `debug-*` crates — zero `unsafe`,
  re-confirmed by grep.

**Dimension 3 — Leaks / drop ordering**:
- AllocatorResource-before-device (#1406/#1477): `impl Drop for App`
  (`byroredux/src/main.rs:449-463`) still removes `AllocatorResource` before
  the renderer is dropped; comment cites the invariant (REG-08 / #1640 /
  #1477) explicitly.
- Deferred-destroy drain ordering: `tick_deferred_destroy` confirmed running
  after `wait_for_fences` in `draw.rs`; shutdown drain confirmed calling
  `device_wait_idle()` before draining. `pending_destroy_scratch` (#1782, new
  since 07-02) verified correctly wired into both paths (see above).
- Rapier release on cell unload (#1520/#1531): all 7
  `rapier_release_tests` re-run green today, including
  `release_removes_ragdoll_bodies_colliders_and_joints`.
- CPU-side growth: `MaterialTable::intern` cap and `AnimationClipRegistry`
  lowercased interning unchanged.
- **Open, previously-tracked, not re-litigated here**: #1861
  (`with_one_time_commands_inner` leaks fence/cmd-buffer on error paths,
  LOW, OPEN) sits squarely in this dimension but was found by yesterday's
  renderer audit, not today's safety sweep — left as Existing: #1861, no new
  evidence gathered today.

**Dimension 4 — Unsafe-block discipline**: covered by the one finding above.

**Dimension 5 — Vulkan spec compliance**:
- The #1790 AS_READ gap (the only live Vulkan-spec finding from the last two
  audits) is fixed and re-verified above.
- TLAS resize `device_wait_idle` before freeing old allocation (#1390):
  unchanged, confirmed present.
- Volumetrics dispatch gate (`VOLUMETRIC_OUTPUT_CONSUMED`): unchanged, both
  `draw.rs` call sites honor it.
- `NON_COHERENT_ATOM_SIZE` assumption (#1759): assertion still present.
- SPIR-V reflection pins: all `scene_descriptor_reflection_tests` green.
- **Open, previously-tracked, not re-litigated here**: #1783 (`skin_palette`
  init failure not coupled to skin_compute — skin chain can run against an
  uninitialised palette SSBO, MEDIUM, OPEN) is a concurrency-domain finding
  from a different audit; noted for cross-reference only.
- No validation-layer run was captured today (no Vulkan device available in
  this session) — the #1790 fix's correctness rests on the static barrier
  read plus the new pinning test, consistent with the "needs
  validation-layer/RenderDoc verification" framing for anything not directly
  observable via `cargo test`. A future audit with device access should
  re-run the FNV saloon 180-frame validation-layer scenario to confirm zero
  hazards post-fix.

**Dimension 6 — Material table layout**: `gpu_material_size_is_300_bytes`,
`gpu_material_field_offsets_match_shader_contract`,
`gpu_material_glsl_field_names_pinned`,
`gpu_material_glsl_field_order_matches_rust_struct`, and
`material_hash_matches_gpu_material_field_hash` all re-run green today. No
`[f32; 3]` field in any `#[repr(C)]` GPU struct.

**Dimension 7 — RT IOR/glass**: `GLASS_RAY_BUDGET = 1048576` unchanged;
Frisvad basis comment still active; `DBG_VIZ_GLASS_PASSTHRU = 0x80` unchanged,
no collision with the new `ENABLE_LEGACY_WRS` compile-time gate added by
#1799 (a perf-only preprocessor change, confirmed not a DBG_* runtime flag).

**Dimension 8 — NPC/animation spawn**: `bone_palette_overflow_tests`
(`at_capacity_fills_palette_completely`,
`over_capacity_breaks_loop_and_truncates_offsets`) re-run green today.

**Dimension 9 — NIFAL NaN boundary**: `material_translate.rs`'s NaN-seed +
`resolve_pbr()` clamp pairing unchanged; the Starfield sentinel-slot fix
(#1828/#1829) touches geometry acceptance only, not the NaN-boundary NIFAL
translation path.

**Dimension 10 — debug-ui teardown**: unchanged; `DebugUiState` still holds no
Vulkan handles, `EguiPass` teardown still runs immediately after
`device_wait_idle` ahead of every other teardown step.

---

## Coverage Note

All 21 `crates/` + `byroredux/` swept for `unsafe` this audit (fresh
recount): renderer 629 unsafe tokens (545 non-test block openers, 222
uncommented), nif 11, core 6, byroredux 2, pex 1 (guarded transmute, now with
a second capped-recursion pass alongside it), cxx-bridge 1 (bridge marker
only), facegen 1 (comment prose, no code), plugin 1 (comment prose, no code);
save, scripting, audio, bsa, bgsm, papyrus, physics, spt, sfmaterial,
platform, ui, debug-protocol, debug-server, debug-ui all 0. `cargo test
--workspace` — 0 failures across every crate exercised. Dedup checked against
400 issues (70 open) via `gh issue list` and against
`docs/audits/AUDIT_SAFETY_2026-07-01.md` / `AUDIT_SAFETY_2026-07-02.md`; no
new safety-domain defect survived scrutiny today — the tree's safety posture
improved (two subsystem-level Vulkan-spec/UAF fixes landed) rather than
regressed since yesterday.

Next step: `/audit-publish docs/audits/AUDIT_SAFETY_2026-07-03.md` — the one
finding is a continuation of the existing #1644 unsafe-comment sweep, so
publish should link to that issue rather than open a duplicate.
