# NIF-D4-N06: NiTriShape UV transform dropped when NiMaterialProperty precedes NiTexturingProperty

**Issue**: #435 — https://github.com/matiaszanolli/ByroRedux/issues/435
**Labels**: bug, nif-parser, renderer, medium, legacy-compat

---

## Finding

`crates/nif/src/import/material.rs:562-569` — the base-slot `TexTransform` (uv offset/scale) from `NiTexturingProperty` only overwrites `info.uv_offset` / `info.uv_scale` when `!info.has_material_data`:

```rust
if !info.has_material_data {
    if let Some(base_tex) = &tex.base_texture {
        if let Some(transform) = &base_tex.transform {
            info.uv_offset = transform.translation;
            info.uv_scale = transform.scale;
        }
    }
}
```

But `has_material_data` gets set to `true` as soon as `NiMaterialProperty` is found earlier in the property loop. Since NiMaterialProperty is typically listed BEFORE NiTexturingProperty in Oblivion/FO3/FNV property arrays, the texture's own UV transform is silently dropped — **even though NiMaterialProperty doesn't carry a UV transform**. The two are orthogonal.

## Impact

Oblivion / FO3 / FNV meshes with authored UV offset/scale on the base texture slot lose their transform:
- Tapestries with scrolling patterns
- FNV signs with scrolling text
- Oblivion water with UV-animated normal/diffuse
- Banner meshes with authored UV rotation

Visible as slight texture misalignment or stuck-in-place scrolling effects. Not a crash, but an authored-fidelity gap.

BSShader paths (Skyrim+/FO4) are unaffected — they carry `uv_offset`/`uv_scale` directly on the shader property and don't go through this code path.

## Games affected

Oblivion, FO3, FNV.

## Fix

Gate on a separate flag rather than piggybacking on `has_material_data`:

```rust
if !info.has_uv_transform {
    if let Some(base_tex) = &tex.base_texture {
        if let Some(transform) = &base_tex.transform {
            info.uv_offset = transform.translation;
            info.uv_scale = transform.scale;
            info.has_uv_transform = true;
        }
    }
}
```

Add `has_uv_transform: bool` (default `false`) to `MaterialInfo`.

Alternative: always prefer the texture-slot transform (since NiMaterialProperty has none, there's no conflict). Either works; the flag is clearer.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check for other fields that piggyback on `has_material_data` for precedence when they shouldn't. Grep for `!info.has_material_data` in material.rs.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NiTriShape with property list `[NiMaterialProperty, NiTexturingProperty]` where the NiTexturingProperty's base slot has a non-identity transform (offset=[0.5, 0.0], scale=[2.0, 1.0]). Assert `MaterialInfo.uv_offset == [0.5, 0.0]` and `uv_scale == [2.0, 1.0]`.

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 4 N06.
