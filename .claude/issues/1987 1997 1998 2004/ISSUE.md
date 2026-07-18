# Batch: #1987, #1997, #1998, #2004

## #1987 — FO4-D2-01: metalness-from-saturation formula inlined and un-unit-tested
- Severity: LOW · Labels: bug, import-pipeline, low, tech-debt
- Location: `byroredux/src/asset_provider/material.rs:728-748` (`merge_bgsm_into_mesh`)
- Fix: extract `pub(crate) fn bgsm_metalness(spec: [f32;3], mult: f32, pbr: bool) -> f32`,
  add regression tests: white spec (pbr=false) → ~0.0, tinted spec (pbr=false) → >0.1.
  Mirrors `conductor_diffuse_tint` (#1591) / `bgsm_blend_to_gamebryo` (#1823) extraction pattern.

## #1997 — REN-D15-01: procedural water-normal fallback hashes absolute world coords
- Severity: MEDIUM · Labels: bug, renderer, medium
- Location: `crates/renderer/shaders/water.frag` (`sampleScrollingNormal`), plus
  `byroredux/src/cell_loader/water.rs`, `crates/core/src/ecs/components/water.rs`,
  `byroredux/src/env_translate.rs` (context only, no rust change expected)
- Fix: rebase procedural branch's hash input to render-origin-relative coords
  (`vWorldPos.xz - renderOrigin.xz`) instead of absolute world XZ. Update stale
  "never a default path" comment. Consider a CPU-side test pinning
  resolve_water_material's procedural-default classification.

## #1998 — REN-D15-02: water-plane comment claims Decal gives depth-bias protection (false)
- Severity: LOW · Labels: documentation, renderer, low
- Location: `byroredux/src/cell_loader/water.rs` (`spawn_water_plane` comment)
- Fix: correct the comment — RenderLayer::Decal is for draw-order placement only,
  not depth-bias z-fight protection (depth_bias_enable is false in water pipeline).
  No functional code change; documentation-accuracy only.

## #2004 — NIF-D1-05: NiTexturingProperty decal-slot count has no upper bound
- Severity: MEDIUM · Labels: bug, nif-parser, medium, nif
- Location: `crates/nif/src/blocks/properties.rs:300-313` (`NiTexturingProperty::parse`)
- Fix: clamp `num_decals` to hard max of 4 (nif.xml fixed slot count); treat
  texture_count implying more as a parse error. Add regression test with
  anomalous texture_count fixture.

## Domain classification
- #1987 → `byroredux` (binary crate, asset_provider) — but effectively self-contained,
  test with `cargo test -p byroredux`
- #1997 → `byroredux-renderer` (shader) + `byroredux` (water spawn, minor) — mixed;
  shader can't be unit tested, Rust side may get a small classification test
- #1998 → `byroredux` (comment-only fix in cell_loader)
- #2004 → `byroredux-nif`
