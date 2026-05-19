# REN-D14-NEW-01: water.vert comment cites stale Rust-struct path post-Session-34 split

**Labels**: renderer, low

**Source**: [`docs/audits/AUDIT_RENDERER_2026-05-18_DIM11_DIM14.md`](docs/audits/AUDIT_RENDERER_2026-05-18_DIM11_DIM14.md)
**Dimension**: Material Table (R1 Refactor)
**Severity**: LOW (doc rot, not a code bug)

## Observation

`crates/renderer/shaders/water.vert:36-40`:

```glsl
// We only consume `model`; the rest of the GpuInstance fields are
// not driven by the water material path (which lives in push
// constants — see water.frag). Layout must match the Rust struct
// at `crates/renderer/src/vulkan/instance.rs` byte-for-byte.
struct GpuInstance {
```

`crates/renderer/src/vulkan/instance.rs` no longer exists. Post-Session-34 the struct lives at `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` and is pinned by `gpu_instance_is_112_bytes_std430_compatible` at `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:24`.

## Why bug

Doc rot. A future maintainer following the cited path lands on nothing and may either re-create a duplicate `instance.rs` or fall back to GLSL-only reasoning. Same pattern flagged in the `feedback_audit_findings` memory — `~5 of 30 audit findings in the 2026-04 sweep were stale on premise` specifically because of this kind of path drift.

## Fix

Replace the path in the comment with `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` and reference the byte-pin test (`gpu_instance_is_112_bytes_std430_compatible`) so future readers have a check they can re-run.

Optional sweep: the other 4 shaders that declare `struct GpuInstance` (`triangle.vert`, `triangle.frag`, `ui.vert`, `caustic_splat.comp`) should be checked for the same stale citation.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: check the other 4 GpuInstance-declaring shaders for the same stale `instance.rs` path citation (`triangle.vert`, `triangle.frag`, `ui.vert`, `caustic_splat.comp`)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A (docs only) — but ideally couple this with adding a one-line grep-based CI check that asserts no shader cites `instance.rs`

## Related

- Session-34 refactor — split `instance.rs` into `scene_buffer/gpu_types.rs` + siblings
- `gpu_instance_is_112_bytes_std430_compatible` — the byte-pin test that's the canonical Rust↔shader contract for `GpuInstance`
- `feedback_shader_struct_sync.md` — project memory pinning the 5-shader lockstep requirement
- `feedback_audit_findings.md` — pattern memory: stale doc paths are a common source of false audit findings
