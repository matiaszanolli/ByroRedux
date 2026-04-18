# NIF-D4-N02: BsTriShape drops BSEffectShaderProperty .bgem material_path (FO4+ effect shaders)

**Issue**: #434 — https://github.com/matiaszanolli/ByroRedux/issues/434
**Labels**: bug, nif-parser, renderer, high

---

## Finding

`crates/nif/src/import/mesh.rs:506-516` (`find_material_path_bs_tri_shape`) only downcasts the shape's `shader_property_ref` to `BSLightingShaderProperty` and extracts `.bgsm`/`.bgem` from its name. When the shape binds a `BSEffectShaderProperty` whose name ends in `.bgem` (FO4/FO76 effect-material files), the path is dropped.

No fallback branch for `BSEffectShaderProperty.net.name`.

## Impact

FO4/FO76/Starfield effect-shader surfaces whose real material data lives in a BGEM file silently lose the material pointer:
- Weapon energy effects (laser, plasma)
- Magic spell surfaces
- Environmental FX (steam vents, electrical arcs)
- Glow decals (terminals, signs)

Renderer then treats them as inline-shader materials with defaulted `base_color` / `uv_scale`, missing artist edits in the BGEM.

The equivalent NiTriShape path in `material.rs:436-475` handles BSEffectShaderProperty correctly (writes `material_path`, captures `emissive_*`, `uv_*`). BsTriShape is the sibling gap.

## Fix

Extend `find_material_path_bs_tri_shape` to also check `BSEffectShaderProperty`:

```rust
fn find_material_path_bs_tri_shape(scene: &NifScene, shape: &BsTriShape) -> Option<String> {
    let idx = shape.shader_property_ref.index()?;
    // Preferred: BSLightingShaderProperty
    if let Some(lit) = scene.get_as::<BSLightingShaderProperty>(idx) {
        if let Some(name) = lit.net.name.as_deref() {
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".bgsm") || lower.ends_with(".bgem") {
                return Some(name.to_string());
            }
        }
    }
    // Fallback: BSEffectShaderProperty (FO4+ effects)
    if let Some(eff) = scene.get_as::<BSEffectShaderProperty>(idx) {
        if let Some(name) = eff.net.name.as_deref() {
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".bgem") || lower.ends_with(".bgsm") {
                return Some(name.to_string());
            }
        }
    }
    None
}
```

## Related

- Sibling of #346 (BsTriShape import path ignores BSEffectShaderProperty) — that one is broader (emissive/uv fields); this one is specifically the material_path bgem drop.
- Unblocks the FO4 BGSM/BGEM parser work tracked at #411 (REN-MEM-C1-adjacent, fo4-audit tier 3).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify `find_effect_shader_bs` (mesh.rs:432) captures the same BSEffectShaderProperty — if that path already writes the bgem path elsewhere, this fix might be redundant. Check `ImportedMesh.effect_shader` structure vs `ImportedMesh.material_path`.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Import a FO4 weapon effect mesh (e.g., a laser rifle `_FXGlow`). Assert `ImportedMesh.material_path == Some("Materials\\...\\laser_beam.BGEM")`.

## Source

Audit: `docs/audits/AUDIT_NIF_2026-04-18.md`, Dim 4 N02.
