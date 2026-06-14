# F-11.D2: GpuCamera doc-comment names a stale size (320 B) + non-existent pin test

**Issue**: #1526
**Severity**: LOW (doc-rot — correctness fully guarded)
**Dimension**: TAA / struct-pin documentation
**Labels**: low, renderer, tech-debt
**Source**: docs/audits/AUDIT_RENDERER_2026-06-14_DIM11.md (F-11.D2)
**Filed**: 2026-06-14

> Snapshot as filed (TD10-001). GitHub is authoritative for current state:
> `gh issue view 1526 --json state`.

## Description

The `GpuCamera` doc-comment (`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:171-176`)
is stale on three counts:

1. **Size**: claims 320 B; struct is 336 B (192 B mat4 + 9 trailing vec4 × 16 = 144 B).
2. **Test name**: names `gpu_camera_is_320_bytes`; live pin is `gpu_camera_is_336_bytes`
   (`crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:56`, asserts == 336).
3. **Layout list**: enumerates only 8 trailing vec4, omitting `render_origin` (the 9th,
   added by #markarth-precision / #1492 — the field that took 320 → 336 B).

The #1484 doc-rot pass (`6400e78b`) missed this comment.

## Impact

Doc-only. `prev_view_proj` (TAA reprojection input, lives in `GpuCamera`) is fully guarded by
`gpu_camera_is_336_bytes` + the cross-shader SPIR-V reflection guard
`camera_ubo_size_matches_gpu_camera_in_every_shader` (`crates/renderer/src/vulkan/reflect.rs:433-435`,
uses `size_of::<GpuCamera>()` dynamically). A real drift fails the test/build; the stale doc
only misdirects a maintainer to a non-existent test + undercounted layout.

## Suggested Fix

Update `gpu_types.rs:171-176`: "336 bytes", `gpu_camera_is_336_bytes`, add `render_origin`
to the trailing-vec4 list ("nine trailing vec4 … → 336 B"). ~3-line doc edit.

## Completeness Checks
- [ ] SIBLING: sweep `GpuInstance` / `GpuMaterial` / `GpuLight` doc-comments
  (`scene_buffer/gpu_types.rs`, `material.rs`) for the same stale "pinned by `<name>` test" /
  byte-count drift pattern.
- [ ] TESTS: N/A — authoritative pin `gpu_camera_is_336_bytes` already exists; comment-only fix.
- [ ] DROP / LOCK_ORDER / FFI / UNSAFE / CANONICAL-BOUNDARY: N/A.
