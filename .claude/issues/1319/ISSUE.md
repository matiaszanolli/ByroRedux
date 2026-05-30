# #1319 -- TD-D4: Stale constants -- GpuCamera doc, workgroup, bsver literals

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: LOW | **Dim 4** — Magic Numbers / Stale Constants
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (TD4-NEW-10, TD4-NEW-12, TD4-NEW-13, TD4-NEW-15 bundled)
**Domain**: renderer | **Effort**: trivial

**TD4-NEW-10** — `GpuCamera` doc comment in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (and `scene_buffer/buffers.rs`) still says "288 bytes". Actual size since #1210 is **304 B** (pinned by `gpu_camera_is_288_bytes` test which asserts 304). Update the doc comment.

**TD4-NEW-12** — `flat_shading_bit_pinned` test comment in `crates/renderer/src/shader_constants.rs` references "pre-#1190 location" — the bit was moved post-#1190 and the test comment is stale.

**TD4-NEW-13** — `skin_vertices.comp` workgroup size (`local_size_x = 64`) has no Rust-side lockstep constant or assert. The companion `skin_palette.comp` workgroup is 64, and `skin_compute.rs` dispatches with `(count + 63) / 64`. A future workgroup-size change would silently diverge. Add a `const SKIN_WORKGROUP_SIZE: u32 = 64` in `skin_compute.rs` and use it in both the dispatch math and a doc comment cross-ref to the shader.

**TD4-NEW-15** — bsver values 9 and 21 are used in version-gating conditions without named constants (only `BSVER_FO3 = 34`, `BSVER_SKYRIM = 83`, etc. are named). Add `pub const BSVER_OBLIVION_OLD: u32 = 9` and `pub const BSVER_FO76: u32 = 21` (or equivalent) to `crates/nif/src/version.rs`.

## Completeness Checks
- [ ] **SIBLING**: grep for other inline bsver literals (e.g. `>= 9`, `<= 21`) and replace with constants
- [ ] **TESTS**: TD4-NEW-13 addition should pass the existing `vertex_stride_matches_rust_vertex_size` test
- [ ] **CANONICAL-BOUNDARY**: N/A
- [ ] **UNSAFE**: no unsafe
