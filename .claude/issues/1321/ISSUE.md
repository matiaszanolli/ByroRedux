# #1321 -- TD-D7: Doc rot -- GpuMaterial 260B x8, GpuCamera 288B, classify_pbr

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: LOW | **Dim 7** — Stale Documentation
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (TD7-NEW-01, TD7-NEW-02, TD7-NEW-03, TD7-NEW-04 bundled)
**Domain**: renderer | **Effort**: trivial

**TD7-NEW-01** — `GpuMaterial` struct doc says "260 bytes" in **8 sites** across `material.rs` and `scene_buffer/` files. Actual size is **300 B** since the Disney-BSDF additions (#1248–#1250). The test `gpu_material_size_is_260_bytes` also has a stale name (it asserts 300). Replace all 8 occurrences and optionally rename the test.

**TD7-NEW-02** — `GpuCamera` doc comments in `gpu_types.rs`, `buffers.rs`, and the audit skill say "288 bytes / six trailing vec4". Actual since #1210 is **304 B / seven vec4** (sun_direction field added). The layout test `gpu_camera_is_288_bytes` also has a stale name. Update doc + optionally rename test.

**TD7-NEW-03** — Deleted `Material::classify_pbr` is cited as a live method in **4 doc comments** in `crates/core/src/ecs/components/material.rs` (~lines 396/551/578/587). The method was removed during the NIFAL canonical-material-translation refactor; each reference should be updated to `Material::resolve_pbr` or a `// (deleted — see NIFAL)` note.

**TD7-NEW-04** — `material.rs` doc cites `triangle.frag:83-126` as the location of the PBR-flag `#define` block. The actual location is lines **110–184** post Disney-BSDF additions. Update the cross-reference.

## Completeness Checks
- [ ] **SIBLING**: after TD7-NEW-03, grep for `classify_pbr` across the whole codebase to catch remaining stale refs
- [ ] **TESTS**: no behavior change; doc-only
- [ ] **CANONICAL-BOUNDARY**: TD7-NEW-03 touches material.rs which is the NIFAL canonical material component — no logic change, just doc update
- [ ] **UNSAFE**: no unsafe
