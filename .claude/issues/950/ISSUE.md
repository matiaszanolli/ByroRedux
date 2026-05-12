# #950 — SAFE-25: Main raster pipeline lacks SPIR-V descriptor-set reflection validation

**Severity**: MEDIUM
**Labels**: medium, renderer, vulkan, safety, bug
**Source audit**: [docs/audits/AUDIT_SAFETY_2026-05-11.md](../../../docs/audits/AUDIT_SAFETY_2026-05-11.md) (Dim 7 / Dim 8)
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/950

## Location

[crates/renderer/src/vulkan/pipeline.rs:130-142](../../../crates/renderer/src/vulkan/pipeline.rs#L130-L142) — `build_triangle_pipeline_layout`

## TL;DR

Every compute pipeline (`bloom`, `caustic`, `ssao`, `svgf`, `taa`,
`skin_compute`, `composite`, `compute`, `volumetrics`) validates its
descriptor set layout against the SPIR-V shader bindings via
`reflect::validate_set_layout`. The **main raster pipeline**
(`triangle.vert` + `triangle.frag`) — where R1 lockstep risk is
highest — does not. The reflection layer is the only sound guard
against descriptor binding drift between Rust layout creation and
shader source.

## Why it matters

`triangle.frag` reads ≥ 12 descriptor bindings: `GpuInstance` SSBO,
`MaterialBuffer` SSBO, global vertex/index SSBOs, bindless
`textures[]`, TLAS, light SSBO, cluster SSBO, ray-budget atomic.
Shader Struct Sync tests (#318 / #417 / #806) pin **byte layout** of
the struct fields but cannot catch binding-type / count / stage
drift. That drift is exactly what `validate_set_layout` is for.

## Fix sketch

Hook `validate_set_layout` into the call site that constructs
`descriptor_set_layout` and `scene_set_layout`. SPIR-V bytes are
already available (`include_bytes!` in `pipeline.rs`). Multi-stage
reflection pattern exists at [bloom.rs:231](../../../crates/renderer/src/vulkan/bloom.rs#L231).
Estimated ~30 lines.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit `composite.rs` vertex+fragment reflection path (already uses validate_set_layout per grep, confirm)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add startup `cargo test` that reflects `triangle.{vert,frag}.spv` and asserts against Rust-side descriptor layout

## Suggested next step

```
/fix-issue 950
```
