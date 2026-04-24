# #583: FO4-DIM6-01: BGSM scalar PBR fields parsed but never forwarded — FO4 materials render on NIF-fallback defaults

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/583
**Labels**: bug, renderer, high, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 6)
**Severity**: HIGH
**Location**: `byroredux/src/asset_provider.rs:356-403` (`merge_bgsm_into_mesh`)

## Description

`merge_bgsm_into_mesh` is the single seam between BGSM template chains and `ImportedMesh`. It only calls `fill()` on six `Option<String>` texture-path slots:
- BGSM arm: `texture_path`, `normal_map`, `glow_map`, `gloss_map`, `env_map`, `parallax_map`
- BGEM arm: `texture_path`, `normal_map`, `glow_map`, `env_map`, `env_mask`

Every scalar PBR field the BGSM parser decodes — `emittance_color`, `emittance_mult`, `emit_enabled`, `specular_color`, `specular_mult`, `smoothness`, `fresnel_power`, `alpha`, `alpha_test_ref`, `rim_power`, `back_light_power`, wetness suite, translucency suite, hair/skin tint, refraction, SSR, mask-writes — is dropped.

The fill predicate `if slot.is_none()` applies only to `Option<String>`. NIF-side scalars default to concrete values (`emissive_mult = 0.0`, `specular_strength = 1.0`), so even extending the existing predicate wouldn't fire for scalars; a child-first "BGSM wins when authored" rule is needed.

## Evidence

- `ImportedMesh` fields `emissive_mult`, `emissive_color`, `specular_color`, `specular_strength`, `glossiness`, `mat_alpha` — all populated from the NIF path only (`crates/nif/src/import/mod.rs:244-259`).
- `cell_loader.rs:1744-1747` and `scene.rs:1173-1176` copy those fields into ECS `MaterialInfo`.
- BGSM parser already decodes the dropped scalars (`crates/bgsm/src/bgsm.rs`, `bgem.rs`, `base.rs`).

## Impact

Every FO4 emissive mesh (lamp, terminal screen, signage) lights with wrong brightness; every metallic surface (power armor, workbench props) uses NIF-fallback specular; wetness / rim / translucency paths never activate. Consistent with the ROADMAP claim "MedTekResearch01 7434 entities @ 90 FPS" — geometry-only, not shading-accurate. Single highest-leverage FO4 finding (~80% of the visual delta).

## Suggested Fix

Extend `merge_bgsm_into_mesh` with child-first scalar forwarding:

```rust
for step in resolved.walk() {
    let bgsm = &step.file;
    // existing texture fills stay
    if !set_emissive && bgsm.emit_enabled {
        mesh.emissive_mult = bgsm.emittance_mult;
        mesh.emissive_color = bgsm.emittance_color;
        set_emissive = true;
    }
    if !set_specular {
        mesh.specular_strength = bgsm.specular_mult;
        mesh.specular_color = bgsm.specular_color;
        set_specular = true;
    }
    if !set_glossiness { mesh.glossiness = bgsm.smoothness; set_glossiness = true; }
    if (mesh.mat_alpha - 1.0).abs() < 1e-6 { mesh.mat_alpha = bgsm.base.alpha; }
    if mesh.uv_offset == [0.0, 0.0] && mesh.uv_scale == [1.0, 1.0] {
        mesh.uv_offset = [bgsm.base.u_offset, bgsm.base.v_offset];
        mesh.uv_scale  = [bgsm.base.u_scale,  bgsm.base.v_scale];
    }
    mesh.two_sided |= bgsm.base.two_sided;
    mesh.is_decal  |= bgsm.base.decal;
    if bgsm.base.alpha_test {
        mesh.alpha_test = true;
        mesh.alpha_threshold = f32::from(bgsm.base.alpha_test_ref) / 255.0;
    }
}
```

**Second-order**: `fresnel_power` has no `GpuInstance` slot — defer a #344-style shader-struct-sync follow-up across `triangle.vert` / `triangle.frag` / `ui.vert`.

## Completeness Checks

- [ ] **UNSAFE**: n/a — all safe code
- [ ] **SIBLING**: Same override pattern applied to the BGEM arm for effect materials
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Extend `bgsm_merge_fills_only_empty_slots` at `asset_provider.rs:515` with scalar-override coverage on a synthetic `ResolvedMaterial` chain. Lock child-first precedence.
- [ ] **REGRESSION**: Visual regression check against `MedTekResearch01` emissive surfaces.

## Related

- Follow-up to closed #493 (FO4-BGSM-4 `asset_provider` resolver landed the texture fills).
- Cross-ref: SK-D3-03 (#570) — `MaterialInfo.material_kind` u8 truncation will affect FO4 once BGSM forwards shader-type variants.
