# RT-5: `derive_normal_map_path` is not game-gated — it fabricates `_n.dds` probes on every game, not just Oblivion/FO3

**Issue**: #3551
**Labels**: bug, import-pipeline, medium, performance
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-5.

## Description

`byroredux/src/cell_loader/spawn/mesh_instance.rs` calls `derive_normal_map_path` **unconditionally** when a mesh leaves its normal slot empty. The #1303 rationale in the comment directly above it is explicitly game-specific — *"Oblivion/FO3 ship normal maps via the `<base>_n.dds` load-time convention"* — but `derive_normal_map_path` (`byroredux/src/asset_provider/texture.rs:278`) takes no game parameter, and no caller gates it.

## Evidence

```rust
// byroredux/src/cell_loader/spawn/mesh_instance.rs
// Oblivion/FO3 ship normal maps via the `<base>_n.dds`
// load-time convention, not an explicit NIF slot. ...  (#1303 / OBL-D4-NEW-01)
(textures.normal, sources.normal) =
    resolve_effective(ov.and_then(|o| o.normal), mesh.material.textures.normal);
if textures.normal.is_none() {
    textures.normal = textures.base_color.as_deref().map(derive_normal_map_path);
    if textures.normal.is_some() {
        sources.normal = MaterialTextureSource::DerivedNormal;
    }
}
```

**Measured cost** — share of `tex.missing` entries that are fabricated `src=derived-normal` paths rather than authored ones:

| Game | derived / total |
|---|---|
| oblivion | 7/8 |
| fnv | 5/6 |
| fo3 | 12/12 |
| skyrim_se | 8/10 |
| fo4 | 13/16 |

## Impact

FO4 and Skyrim author normals explicitly via BGSM / `BSLightingShaderProperty` and have **no** `_n.dds` convention, so every one of those derived paths is a wasted archive lookup per shape plus pure telemetry noise that drowns the real misses in `tex.missing`.

## Suggested Fix

Gate the derive on `GameKind::{Oblivion, Fallout3}` (and FNV if the convention holds there), **or** mark derived paths as speculative so a miss is not bucketed by `tex.missing` at all. The second option is cheaper and does not need a per-game answer.

## Completeness Checks
- [ ] **SIBLING**: The second `derive_normal_map_path` caller at `byroredux/src/scene/nif_loader.rs:921` gets the same gate/marking
- [ ] **CANONICAL-BOUNDARY**: Any per-game gate lives at the parser->`Material` boundary, not pushed into the renderer or re-derived at render time
- [ ] **TESTS**: `byroredux/src/asset_provider/tests/material_path.rs` extended to pin the gated/speculative behaviour, not just the string transform
