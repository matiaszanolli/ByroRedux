# SF-D9-2026-08-07-02: BGSM inner_layer_texture parsed with a live populated role, never wired by merge_external_material

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2627
**Finding ID**: SF-D9-2026-08-07-02

**Severity**: MEDIUM
**Dimension**: 9 (BGSM/BGEM External Material Flow)
**Location**: `crates/bgsm/src/bgsm.rs:42,200`, `crates/nif/src/import/material/mod.rs:1108` (NIF path fills the role), `byroredux/src/asset_provider/material.rs:881-975` (BGSM fill block — no `inner_layer` entry)
**Status**: NEW

## Description
The BGSM v≤2 legacy texture list reads `envmap, glow, inner_layer,
wrinkles, displacement`; the merge forwards `envmap`, `glow`, `wrinkles`,
`displacement` and silently drops `inner_layer`. Unlike the documented
#2109 glass-overlay deferral, the sink here already exists (populated by
the NIF `BSLightingShaderProperty` multi-layer-parallax path, resolved to a
real texture handle downstream) — only the BGSM arm fails to wire it.

## Evidence
`crates/bgsm/src/bgsm.rs:42,200` parses `inner_layer_texture`;
`byroredux/src/asset_provider/material.rs:881-975` (the BGSM fill block)
forwards `envmap`/`glow`/`wrinkles`/`displacement` but has no `inner_layer`
entry, even though `MaterialTextureSet::inner_layer` is a live, populated
role via the NIF path (`crates/nif/src/import/material/mod.rs:1108`).

## Impact
A BGSM authoring its inner layer externally (Skyrim SE ice/glass, FO4
layered panes — the multi-layer-parallax slot this dimension's
glass/transmissive coverage targets) renders with the layer absent.

## Suggested Fix
One more `fill(&mut material.textures.inner_layer,
&bgsm.inner_layer_texture, ...)` adjacent to the existing
`displacement → height` fill.

## Completeness Checks
- [ ] **TESTS**: A BGSM fixture with a populated `inner_layer_texture` asserts it reaches `MaterialTextureSet::inner_layer`
