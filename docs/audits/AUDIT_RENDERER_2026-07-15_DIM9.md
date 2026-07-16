# Renderer Audit — 2026-07-15 (Dimension 9: GPU Skinning Compute + BLAS Refit, M29)

Scope: `--focus 9` — single-dimension run of `/audit-renderer`, `--depth deep`.
Covers `crates/renderer/src/vulkan/skin_compute.rs`, `skin_vertices.comp` /
`skin_palette.comp`, `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`,
`byroredux/src/render/skinned.rs`, `byroredux/src/render/bone_palette_overflow_tests.rs`,
and `crates/core/src/ecs/resources/skin_slot_pool.rs`.

## Cross-check against prior coverage

This dimension was deeply delta-audited 12 days ago in
[`AUDIT_RENDERER_2026-07-03.md`](AUDIT_RENDERER_2026-07-03.md) at commit
`8498e559`, confirming #1790 (scratch-serialize barrier mask), #1791/#1796
(early-return pose-hash/bind_inverses rollback), and #1794 (bone_world
per-frame fill elimination) all correct. `git log 8498e559..HEAD` over every
skinning-relevant file shows only two commits since: `82a8c76f` (SAFETY
comments, #1868) and `2d823f11` (`SkinSlotPool` split out of `resources.rs`
into its own file, #1869 — pure code motion). Rather than trust that the
prior conclusions still hold, this run independently re-traced every
checklist item against the current, post-split code — a module split is
exactly the kind of change that can silently drop a guard (wrong visibility,
a re-export that changed semantics, a duplicated-instead-of-moved helper).

## Executive Summary

All ten checklist items and every named regression guard are intact against
the current post-#1869 code. The #1790 scratch-serialize barrier's dst mask
still carries `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR`
(the single most load-bearing guard in this dimension); the #1791/#1796
early-return rollback is still correctly gated on `skin_dispatch_ran` (reset
before both early-return guards, set true only in `record_skinned_blas_refit`).
The #1869 module split is a clean code motion: exactly one `SkinSlotPool`
definition in the tree, a correct `pub use` re-export, no duplicated helpers,
and all 58 relevant tests (34 renderer + 22 core pool + 2 bin-level overflow)
pass.

One finding survived scrutiny, LOW severity, doc-only:

- **REN-D9-01 (LOW)** — the `SkinSlotPool` capacity doc comment (carried over
  verbatim by the #1869 split) has an arithmetic error: it states
  `196608 / 144 = 1366` when integer division actually gives `1365`, and then
  separately gets the allocatable count wrong too. `main.rs`'s own comment and
  both authoritative design docs (`shader-pipeline.md`, `memory-budget.md`)
  already state the correct value. No functional impact — every runtime size
  is computed from the constants, never from this comment's literals.

No bench-of-record comparison — this was a static/functional code trace plus
targeted `cargo test` runs, not a live render.

## Findings

### REN-D9-01: `SkinSlotPool` capacity doc comment has an arithmetic error
- **Severity**: LOW
- **Dimension**: Skinning
- **Location**: `crates/core/src/ecs/resources/skin_slot_pool.rs` (struct-level doc comment on `SkinSlotPool`)
- **Status**: NEW
- **Description**: The doc comment states the capacity is "`MAX_TOTAL_BONES / MAX_BONES_PER_MESH`, currently 196608 / 144 = 1366 with slot 0 reserved → 1365 allocatable". This is wrong on two counts: `196608 / 144` (integer division) is `1365` (remainder 48), not `1366`; and the pool is actually constructed as `(MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1`, giving `1364` allocatable slots, not `1365`. `byroredux/src/main.rs`'s own comment already states the correct chain ("floor(196608/144) - 1 = 1364"), as does `crates/renderer/src/vulkan/skin_compute.rs`'s comment ("floor(196608/144) = 1365"), and both `docs/engine/shader-pipeline.md` and `docs/engine/memory-budget.md` state "144 slots × 1 364 skinned meshes".
- **Evidence**:
  ```
  $ python3 -c "print(196608//144)"
  1365
  ```
  ```rust
  // skin_slot_pool.rs doc comment (incorrect)
  /// `MAX_TOTAL_BONES / MAX_BONES_PER_MESH`, currently 196608 / 144 =
  /// 1366 with slot 0 reserved → 1365 allocatable; see #1284
  ```
  ```rust
  // main.rs (correct)
  // 1364. Allocating one slot beyond would push the palette ...
  skin_slot_pool: byroredux_core::ecs::resources::SkinSlotPool::new(
      ((MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1) as u32,
  ```
- **Impact**: Documentation only. No functional effect — all runtime sizing is derived from the `MAX_TOTAL_BONES`/`MAX_BONES_PER_MESH` constants directly, never from the literals quoted in this comment, and `bone_palette_overflow_tests::at_capacity_fills_palette_completely` derives its expectations from the same constants (immune to this comment being wrong). Risk is purely a future reader trusting the comment and mis-sizing a hand-rolled calculation by one slot.
- **Related**: This comment was carried over verbatim by the #1869 `SkinSlotPool` file split (`2d823f11`) from its prior home in `resources.rs` — it appears to predate that split and was never corrected, not something the split itself introduced.
- **Suggested Fix**: Correct the comment to "floor(196608 / 144) = 1365, minus the reserved slot 0 → 1364 allocatable", matching `main.rs` and both design docs.

## Regression Guards Verified Intact (not re-proposed)

1. **Vertex stride constant** — `VERTEX_STRIDE_FLOATS = 25` is the single source in `shader_constants_data.rs`; production code in `skin_compute.rs` consumes the derived `VERTEX_STRIDE_BYTES` constant, never a hardcoded `25`. Pinned by a test asserting `size_of::<Vertex>() == VERTEX_STRIDE_FLOATS * 4 == 100`. Test passes.
2. **Dispatch order + workgroup size** — `skin_palette.comp`'s dispatch is recorded strictly before `skin_vertices.comp`'s (via `record_skinned_blas_refit`); both declare a 64-wide workgroup (`SKIN_WORKGROUP_SIZE = 64`, single source), dispatch counts use `div_ceil(64)`. Pinned by a workgroup-size-match test. Matches `shader-pipeline.md`'s stated submission order.
3. **Push constants** — `SkinPushConstants` (12 B) and `SkinPalettePushConstants` (4 B) match their GLSL counterparts field-for-field, both well under the 128 B floor. Pinned by size-assertion tests.
4. **Buffer usage flags** — the skinned output buffer carries `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. `VERTEX_BUFFER` is deliberately absent (#681/MEM-2-6 — raster-VBO read from this buffer is a deferred phase, and omitting the unused bit tightens the memory-type mask on unified-memory GPUs); confirmed no `cmd_bind_vertex_buffers` site ever binds this buffer as a VBO. The checklist's expectation of a `VERTEX_BUFFER` flag is itself stale relative to that intentional #681 decision — adding it back would be the regression, not its current absence.
5. **Barrier scopes** — compute-write → AS-build-input (`SHADER_READ` at the build stage, correct per #1436, not `ACCELERATION_STRUCTURE_READ`), palette write → dual compute/vertex read, and refit → TLAS-build read are all stage/access-appropriate on inspection.
6. **Refit vs rebuild** — geometry/vertex-count drift between BUILD and UPDATE is rejected (VUID-03667 guard) with a fallback to full rebuild next frame; skinned BLAS are excluded from the static-mesh LRU eviction path entirely (separate lifecycle); the refit-count → rebuild threshold (600 frames) matches `memory-budget.md`.
7. **Scratch-serialize barrier dst mask (#1790)** — `record_scratch_serialize_barrier` (now in `blas_skinned.rs`) still emits `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR`, not WRITE-only, at both call sites (top of `refit_skinned_blas` and before the first-sight build in `build_skinned_blas_batched_on_cmd`). This is the single most load-bearing guard in this dimension and it holds.
8. **Bone-palette overflow guard** — a one-shot warn latch plus a cumulative attempt counter fires at `MAX_TOTAL_BONES` capacity; over-cap entities return `None` and fall back to bind pose rather than silently truncating. `bone_palette_overflow_tests.rs` still exists and still covers both the at-capacity and over-capacity cases.
9. **Post-#1869 split correctness** — exactly one `SkinSlotPool` definition exists in the tree (no duplicate left behind in `resources/mod.rs`); the re-export chain (`pub mod skin_slot_pool; pub use skin_slot_pool::SkinSlotPool;`) resolves correctly for every cross-module caller (`render/skinned.rs`, `bone_palette_overflow_tests.rs`, `main.rs`); every member the skinning pipeline depends on (`pose_dirty`, `try_mark_pose_dirty`, `clear_pose_dirty`, `rollback_pending_pose_commits`, `drain_pending`/`requeue_pending`, `allocate`/`sweep`/`mark_seen`) is present with unchanged semantics.
10. **Early-return rollback (#1791/#1796)** — `skin_dispatch_ran` is still reset to `false` before both early-return guards (empty framebuffers, `ERROR_OUT_OF_DATE_KHR`) and set `true` only inside `record_skinned_blas_refit`; on a bailed frame, `main.rs` still calls `rollback_pending_pose_commits()` and `requeue_pending(...)` to undo the premature pose-hash commit and restore drained first-sight bind_inverses. Pinned by an ordering-specific regression test.

## Test Evidence

- `cargo test -p byroredux-renderer --lib skin` → 34 passed, 0 failed.
- `cargo test -p byroredux-core skin_slot_pool` → 22 passed, 0 failed.
- `cargo test --bin byroredux bone_palette` → 2 passed, 0 failed.

## Doc Consistency

`docs/engine/shader-pipeline.md`'s stated submission order (skin_palette →
skin_vertices → AS build) and `docs/engine/memory-budget.md`'s
`SKINNED_BLAS_REFIT_THRESHOLD = 600`, "skinned BLAS not LRU-evicted", and
"144 slots × 1 364 skinned meshes" all match current code. The only stale
value found anywhere in this dimension's scope is the in-source
`SkinSlotPool` comment (REN-D9-01) — both authoritative design docs already
have the correct number.

---
Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-07-15_DIM9.md`
