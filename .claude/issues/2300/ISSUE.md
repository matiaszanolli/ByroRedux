# NIFAL-D5-01: particle emitter texture_path/src_blend/dst_blend override folding copy-pasted outside apply_emitter_overlays at both load sites

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Particles · **Tier Violated**: single-boundary
**Location**: `byroredux/src/scene/nif_loader.rs:520-528` and `byroredux/src/cell_loader/spawn.rs:649-657` (identical 9-line block, outside `apply_emitter_overlays` in `byroredux/src/systems/particle.rs`)
**Status**: NEW

## Description

`texture_path`/`src_blend`/`dst_blend` are authored `NiPSysEmitter` overrides
folded by an identical block copy-pasted at both the loose-NIF load path
(`nif_loader.rs`) and the cell-load path (`cell_loader/spawn.rs`), outside the
declared `apply_emitter_overlays` boundary. The `8a15b064` "streamline
particle emitter selection" refactor touched only the preset-selection
heuristic, not this block — it remains byte-identical, still latent.

## Evidence

```rust
// nif_loader.rs:520-528
if let Some(path) = &emitter.texture_path {
    preset.texture_path = Some(path.clone());
}
if let Some(src) = emitter.src_blend {
    preset.src_blend = src;
}
if let Some(dst) = emitter.dst_blend {
    preset.dst_blend = dst;
}
```
The same shape recurs verbatim at `cell_loader/spawn.rs:649-657` (variable
names `em`/`host` instead of `emitter`/`host_name`, logic identical).

## Impact

Latent drift risk only — both copies currently agree. A future change to one
override-folding site without updating the other would silently diverge
particle emitter texture/blend behavior between loose-NIF and cell-loaded
content.

## Suggested Fix

Extract the shared override-folding block into a helper (or fold it into
`apply_emitter_overlays` itself) and call it from both load sites.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both load-site copies)
- [ ] **CANONICAL-BOUNDARY**: The emitter-override folding stays at the single declared particle boundary (`apply_emitter_overlays`) rather than duplicated at each load site. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

## Filed as

GitHub issue #2300, labels: low, import-pipeline, tech-debt, bug.
