# #563 — SK-D3-02: Slot 7 never routed; FaceTint slot 4 misbound as envmap

**Severity:** MEDIUM
**Labels:** bug, medium, nif-parser, import-pipeline
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/563

## Location
- `crates/nif/src/import/material.rs:547-564`

## One-line
FaceTint (4) slot 4 is detail, NOT envmap — misbound today. MultiLayerParallax (11) and FaceTint (4) slot 7 (tint/inner-layer) never read.

## Fix sketch
Branch slot routing on `shader.shader_type`. Add `detail_map`, `tint_map`, `inner_layer_map` Option<String> fields to MaterialInfo + bindless indices to GpuInstance (lockstep 4 shaders).

## Depends on
- #562 (SK-D3-01) dispatch ladder to consume the new fields

## Next
`/fix-issue 563`
